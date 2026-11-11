//! Search constraints and stability analysis for the CLI.

use ries_rs::gen::{ExpressionConstraintOptions, GenConfig, UserFunctionConstraints};
use ries_rs::profile::Profile;
use ries_rs::search::{Match, SearchConfig};
use ries_rs::symbol::NumType;
use ries_rs::{StabilityAnalyzer, StabilityConfig, StabilityResult};

use super::search_runner::run_search;

/// CLI flags that control expression-shape constraints.
#[derive(Clone, Copy, Debug)]
pub struct ExpressionConstraintArgs {
    pub rational_exponents: bool,
    pub any_exponents: bool,
    pub rational_trig_args: bool,
    pub any_trig_args: bool,
    pub max_trig_cycles: Option<u32>,
}

/// Build per-profile user type arrays for expression constraint checks.
pub fn user_type_arrays(profile: &Profile) -> ([NumType; 16], [NumType; 16]) {
    let mut user_constant_types = [NumType::Transcendental; 16];
    for (idx, uc) in profile.constants.iter().take(16).enumerate() {
        user_constant_types[idx] = uc.num_type;
    }

    let mut user_function_types = [NumType::Transcendental; 16];
    for (idx, uf) in profile.functions.iter().take(16).enumerate() {
        user_function_types[idx] = uf.num_type;
    }

    (user_constant_types, user_function_types)
}

/// Build expression constraint options from CLI flags and profile metadata.
pub fn expression_constraints_from_args(
    profile: &Profile,
    args: ExpressionConstraintArgs,
) -> (ExpressionConstraintOptions, bool) {
    let (user_constant_types, user_function_types) = user_type_arrays(profile);
    // Classify user functions from their bodies so that, e.g., `sq(2)` is an
    // integer while `sq(pi)` is not.
    let mut user_function_constraints = [None; 16];
    for (slot, function) in user_function_constraints
        .iter_mut()
        .zip(profile.functions.iter())
    {
        *slot = Some(UserFunctionConstraints::from_function(
            function,
            &user_constant_types,
        ));
    }
    let options = ExpressionConstraintOptions {
        rational_exponents: args.rational_exponents && !args.any_exponents,
        rational_trig_args: args.rational_trig_args && !args.any_trig_args,
        max_trig_cycles: args.max_trig_cycles,
        user_constant_types,
        user_function_types,
        user_function_constraints,
    };
    let active = options.rational_exponents
        || options.rational_trig_args
        || options.max_trig_cycles.is_some();
    (options, active)
}

/// Parameters for optional stability analysis across tighter tolerances.
#[derive(Clone, Copy)]
pub struct StabilityRunConfig<'a> {
    pub gen_config: &'a GenConfig,
    pub search_config: &'a SearchConfig,
    pub thorough: bool,
    pub use_streaming: bool,
    pub use_parallel: bool,
    pub one_sided: bool,
    pub adaptive: bool,
    pub turbo: bool,
    pub level: u32,
}

/// Run optional stability analysis across progressively tighter tolerances.
pub fn run_stability_check(
    base_matches: Option<Vec<Match>>,
    config: StabilityRunConfig<'_>,
) -> Vec<StabilityResult> {
    let stability_config = if config.thorough {
        StabilityConfig::thorough()
    } else {
        StabilityConfig::default()
    };
    let tolerance_factors = stability_config.tolerance_factors.clone();
    let mut analyzer = StabilityAnalyzer::new(stability_config);

    let base_error = config.search_config.max_error;
    let mut base_matches = base_matches;
    let mut full_config = config.search_config.clone();
    full_config.stop_at_exact = false;
    full_config.stop_below = None;
    for (index, factor) in tolerance_factors.into_iter().enumerate() {
        if index == 0 {
            if let Some(matches) = base_matches.take() {
                analyzer.add_level(matches);
                continue;
            }
        }
        let mut tighter_config = full_config.clone();
        tighter_config.max_error = base_error * factor;
        let result = run_search(
            config.gen_config,
            &tighter_config,
            config.use_streaming,
            config.use_parallel,
            config.one_sided,
            config.adaptive,
            config.turbo,
            config.level,
        );
        analyzer.add_level(result.matches);
    }

    analyzer.analyze()
}
