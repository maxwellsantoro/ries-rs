//! Post-search match filtering and stability analysis for the CLI.

use ries_rs::gen::{expression_respects_constraints, ExpressionConstraintOptions, GenConfig};
use ries_rs::profile::Profile;
use ries_rs::search::{Match, SearchConfig};
use ries_rs::symbol::NumType;
use ries_rs::{canonical_expression_key, StabilityAnalyzer, StabilityConfig, StabilityResult};
use std::collections::HashSet;

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

/// Options for post-search match filtering.
#[derive(Clone, Copy, Debug)]
pub struct PostProcessOptions<'a> {
    pub min_equate_value: Option<f64>,
    pub max_equate_value: Option<f64>,
    pub min_match_distance: Option<f64>,
    pub expression_constraints: Option<&'a ExpressionConstraintOptions>,
    pub numeric_anagram: bool,
    pub canon_enabled: bool,
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
    let options = ExpressionConstraintOptions {
        rational_exponents: args.rational_exponents && !args.any_exponents,
        rational_trig_args: args.rational_trig_args && !args.any_trig_args,
        max_trig_cycles: args.max_trig_cycles,
        user_constant_types,
        user_function_types,
    };
    let active = options.rational_exponents
        || options.rational_trig_args
        || options.max_trig_cycles.is_some();
    (options, active)
}

/// Apply CLI post-search filters to the match list.
pub fn filter_matches(matches: &mut Vec<Match>, options: PostProcessOptions<'_>) {
    if let (Some(min), Some(max)) = (options.min_equate_value, options.max_equate_value) {
        matches.retain(|m| match_in_equate_bounds(m, Some(min), Some(max)));
    } else if options.min_equate_value.is_some() || options.max_equate_value.is_some() {
        matches.retain(|m| {
            match_in_equate_bounds(m, options.min_equate_value, options.max_equate_value)
        });
    }

    if let Some(min_match_distance) = options.min_match_distance {
        matches.retain(|m| m.error.abs() >= min_match_distance);
    }

    if let Some(constraints) = options.expression_constraints {
        matches.retain(|m| {
            expression_respects_constraints(&m.lhs.expr, *constraints)
                && expression_respects_constraints(&m.rhs.expr, *constraints)
        });
    }

    if options.numeric_anagram {
        matches.retain(match_is_numeric_anagram);
    }

    if options.canon_enabled {
        let mut seen = HashSet::<(String, String)>::new();
        matches.retain(|m| {
            let lhs_key =
                canonical_expression_key(&m.lhs.expr).unwrap_or_else(|| m.lhs.expr.to_postfix());
            let rhs_key =
                canonical_expression_key(&m.rhs.expr).unwrap_or_else(|| m.rhs.expr.to_postfix());
            seen.insert((lhs_key, rhs_key))
        });
    }
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
    pub level: u32,
}

/// Run optional stability analysis across progressively tighter tolerances.
pub fn run_stability_check(
    base_matches: Vec<Match>,
    config: StabilityRunConfig<'_>,
) -> Vec<StabilityResult> {
    let stability_config = if config.thorough {
        StabilityConfig::thorough()
    } else {
        StabilityConfig::default()
    };
    let tolerance_factors = stability_config.tolerance_factors.clone();
    let mut analyzer = StabilityAnalyzer::new(stability_config);
    analyzer.add_level(base_matches);

    let base_error = config.search_config.max_error;
    for factor in tolerance_factors.into_iter().skip(1) {
        let mut tighter_config = config.search_config.clone();
        tighter_config.max_error = base_error * factor;
        let result = run_search(
            config.gen_config,
            &tighter_config,
            config.use_streaming,
            config.use_parallel,
            config.one_sided,
            config.adaptive,
            config.level,
        );
        analyzer.add_level(result.matches);
    }

    analyzer.analyze()
}

fn match_in_equate_bounds(
    m: &Match,
    min_equate_value: Option<f64>,
    max_equate_value: Option<f64>,
) -> bool {
    let lhs = m.lhs.value;
    let rhs = m.rhs.value;
    let min_ok = min_equate_value.is_none_or(|min| lhs >= min && rhs >= min);
    let max_ok = max_equate_value.is_none_or(|max| lhs <= max && rhs <= max);
    min_ok && max_ok
}

fn digit_signature(expression: &ries_rs::expr::Expression) -> String {
    let mut digits: Vec<char> = expression
        .symbols()
        .iter()
        .filter_map(|sym| {
            let b = *sym as u8;
            (b'1'..=b'9').contains(&b).then_some(b as char)
        })
        .collect();
    digits.sort_unstable();
    digits.into_iter().collect()
}

fn match_is_numeric_anagram(m: &Match) -> bool {
    let lhs = digit_signature(&m.lhs.expr);
    let rhs = digit_signature(&m.rhs.expr);
    !lhs.is_empty() && lhs == rhs
}

#[cfg(test)]
mod tests {
    use super::*;
    use ries_rs::expr::{EvaluatedExpr, Expression};
    use ries_rs::symbol::NumType;

    fn make_match(lhs: &str, rhs: &str, lhs_value: f64, rhs_value: f64, error: f64) -> Match {
        let lhs_expr = Expression::parse(lhs).unwrap();
        let rhs_expr = Expression::parse(rhs).unwrap();
        Match {
            lhs: EvaluatedExpr::new(lhs_expr, lhs_value, 1.0, NumType::Integer),
            rhs: EvaluatedExpr::new(rhs_expr, rhs_value, 0.0, NumType::Integer),
            x_value: 2.5,
            error,
            complexity: 10,
        }
    }

    #[test]
    fn filter_matches_applies_equate_bounds() {
        let mut matches = vec![
            make_match("x", "1", 0.5, 0.5, 0.0),
            make_match("x1+", "2", 3.0, 2.0, 0.1),
        ];
        filter_matches(
            &mut matches,
            PostProcessOptions {
                min_equate_value: Some(1.0),
                max_equate_value: None,
                min_match_distance: None,
                expression_constraints: None,
                numeric_anagram: false,
                canon_enabled: false,
            },
        );
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].lhs.expr.to_postfix(), "x1+");
    }

    #[test]
    fn filter_matches_applies_numeric_anagram() {
        let mut matches = vec![
            make_match("x1+", "1", 0.0, 0.0, 0.0),
            make_match("x2*", "1", 0.0, 0.0, 0.0),
        ];
        filter_matches(
            &mut matches,
            PostProcessOptions {
                min_equate_value: None,
                max_equate_value: None,
                min_match_distance: None,
                expression_constraints: None,
                numeric_anagram: true,
                canon_enabled: false,
            },
        );
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].lhs.expr.to_postfix(), "x1+");
    }
}
