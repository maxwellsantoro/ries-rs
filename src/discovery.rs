//! Precision-aware discovery, additive to the raw search interfaces.
//!
//! Discovery reuses the matchers' minimum-complexity stop contract rather than
//! hoping that a large error-ranked pool retains a simple explanation. When a
//! candidate resolves the entered precision, its total complexity bounds the
//! retained search. Thus a run can return fewer hypotheses than requested.

use std::cmp::Ordering;
use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::gen::{build_gen_config_from_profile, GenConfig};
use crate::pool::{compare_matches, RankingMode};
use crate::search::{Match, MatcherPath, SearchConfig, SearchStats};
use crate::{Expression, MatchSummary, Preset, Profile, Symbol};

/// Interactive API limits; the lower-level Rust/CLI search remains available.
pub const MAX_DISCOVERY_LEVEL: u32 = 5;
pub const MAX_DISCOVERY_MATCHES: usize = 1000;
pub const MAX_DISCOVERY_VERIFICATION_BITS: u32 = 4096;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct DiscoveryOptions {
    pub level: u32,
    pub max_matches: usize,
    pub preset: Option<String>,
    pub parallel: bool,
    /// Optional numerical corroboration, never symbolic proof. Unsupported
    /// builds/operators/precision report unavailable rather than success.
    pub verification_bits: Option<u32>,
}

impl Default for DiscoveryOptions {
    fn default() -> Self {
        Self {
            level: 2,
            max_matches: 8,
            preset: None,
            parallel: false,
            verification_bits: None,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DiscoverySymbol {
    /// Postfix symbol, including the fixed user-constant slots.
    pub symbol: String,
    pub name: String,
    pub weight: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ResolvedDiscoveryConfig {
    pub level: u32,
    pub max_matches: usize,
    pub preset: Option<String>,
    pub parallel_requested: bool,
    pub verification_bits: Option<u32>,
    pub max_lhs_complexity: u32,
    pub max_rhs_complexity: u32,
    pub input_tolerance: f64,
    pub max_error: f64,
    pub candidate_limit: usize,
    pub selection_policy: String,
    /// The minimum total cost that resolved the input precision, if found.
    /// In this case more costly equations were excluded before selection.
    pub effective_total_complexity_bound: Option<u32>,
    pub grammar: Vec<DiscoverySymbol>,
    pub constants: Vec<(String, f64)>,
    pub trig_argument_scale: f64,
    pub max_expression_length: usize,
    pub zero_value_threshold: f64,
    pub newton_iterations: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DiscoveryStats {
    pub matcher: Option<MatcherPath>,
    pub lhs_count: usize,
    pub rhs_count: usize,
    pub candidates_tested: usize,
    pub newton_calls: usize,
    pub gen_ms: f64,
    pub search_ms: f64,
    pub elapsed_ms: f64,
    pub returned_candidates: usize,
    pub retained_hypotheses: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DiscoveryCheck {
    /// `numerical`, `not_requested`, `verified`, `failed`, or `unavailable`.
    pub status: String,
    pub reason: String,
    pub precision_bits: Option<u32>,
    /// Only finite available estimates are serialized; failures use status.
    pub root_error: Option<f64>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DiscoveryValidation {
    pub numerical: DiscoveryCheck,
    pub high_precision: DiscoveryCheck,
    pub polynomial_certificate: DiscoveryCheck,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DiscoveryHypothesis {
    /// A structural key for this displayed root candidate, not a proof that
    /// all supporting equations have identical global solution sets.
    pub key: String,
    pub representative: MatchSummary,
    /// Other retained equations supporting this candidate; excludes the
    /// representative. Distinct inverse branches are never proximity-grouped.
    pub supporting_equations: Vec<MatchSummary>,
    /// Root distance from the parsed f64 is within half the last typed place.
    pub within_input_precision: bool,
    pub validation: DiscoveryValidation,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DiscoveryRun {
    pub schema: String,
    pub engine_version: String,
    /// Preserved verbatim, including trailing zeroes and surrounding space.
    pub target_literal: String,
    pub target: f64,
    pub config: ResolvedDiscoveryConfig,
    pub stats: DiscoveryStats,
    pub hypotheses: Vec<DiscoveryHypothesis>,
}

/// Discover a small set of precision-aware root hypotheses from a decimal.
///
/// The original literal is required: numeric-only callers can explicitly use
/// `value.to_string()`, but that cannot recover the original entered precision.
/// Existing [`crate::search()`] and binding `search` calls retain raw ordering.
pub fn discover(target_literal: &str, options: &DiscoveryOptions) -> Result<DiscoveryRun, String> {
    validate_options(options)?;
    let text = target_literal.trim();
    if text.len() > 4096 {
        return Err("Target decimal literal must contain 1..4096 characters".into());
    }
    let target = text
        .parse::<f64>()
        .map_err(|_| "Target must be a finite decimal literal".to_owned())?;
    crate::target::validate_decimal_target(target, text, true)?;
    let input_tolerance = crate::search::significant_digits_tolerance(target, Some(text));
    let profile = match options.preset.as_deref() {
        None | Some("") => Profile::new(),
        Some(name) => Preset::from_str(name)
            .ok_or_else(|| format!("Unknown preset '{name}'"))?
            .to_profile(),
    };
    let (lhs_budget, rhs_budget) = crate::search::level_to_complexity(options.level);
    let gen = build_gen_config_from_profile(lhs_budget, rhs_budget, &profile)?;
    let candidate_limit = options.max_matches.saturating_mul(8).max(64);
    let search_config = build_search_config(target, input_tolerance, candidate_limit, &gen);
    let (mut matches, stats) = run_search(&gen, &search_config, options.parallel);
    let complexity_bound = matches
        .iter()
        .filter(|m| m.error.abs() <= input_tolerance)
        .map(|m| m.complexity)
        .min();
    matches.sort_by(|a, b| compare_discovery_matches(a, b, input_tolerance));
    let returned_candidates = matches.len();
    let context = search_config.context().eval;
    let mut hypotheses: Vec<DiscoveryHypothesis> = Vec::new();
    let mut indices = HashMap::new();
    for matched in matches {
        let summary = MatchSummary::from_match_with_context(&matched, &context, &gen.symbol_table);
        let key = hypothesis_key(&summary);
        if let Some(&index) = indices.get(&key) {
            let hypothesis: &mut DiscoveryHypothesis = &mut hypotheses[index];
            hypothesis.supporting_equations.push(summary);
        } else if hypotheses.len() < options.max_matches {
            let validation = validation_for(&matched, target, options.verification_bits, &gen);
            indices.insert(key.clone(), hypotheses.len());
            hypotheses.push(DiscoveryHypothesis {
                key,
                within_input_precision: matched.error.abs() <= input_tolerance,
                representative: summary,
                supporting_equations: Vec::new(),
                validation,
            });
        }
    }
    let config = ResolvedDiscoveryConfig {
        level: options.level,
        max_matches: options.max_matches,
        preset: options.preset.clone().filter(|name| !name.is_empty()),
        parallel_requested: options.parallel,
        verification_bits: options.verification_bits,
        max_lhs_complexity: lhs_budget,
        max_rhs_complexity: rhs_budget,
        input_tolerance,
        max_error: search_config.max_error,
        candidate_limit,
        selection_policy: "minimum_resolving_complexity_then_precision_and_simplicity".into(),
        effective_total_complexity_bound: complexity_bound,
        grammar: resolved_grammar(&gen),
        constants: gen
            .user_constants
            .iter()
            .map(|c| (c.name.clone(), c.value))
            .collect(),
        trig_argument_scale: search_config.trig_argument_scale,
        max_expression_length: gen.max_length,
        zero_value_threshold: search_config.zero_value_threshold,
        newton_iterations: search_config.newton_iterations,
    };
    Ok(DiscoveryRun {
        schema: "ries-discovery-v1".into(),
        engine_version: env!("CARGO_PKG_VERSION").into(),
        target_literal: target_literal.into(),
        target,
        config,
        stats: summarize_stats(&stats, returned_candidates, hypotheses.len()),
        hypotheses,
    })
}

fn build_search_config(
    target: f64,
    input_tolerance: f64,
    candidate_limit: usize,
    gen: &GenConfig,
) -> SearchConfig {
    SearchConfig {
        target,
        max_matches: candidate_limit,
        max_error: (target.abs() * 0.01).max(1e-12).max(input_tolerance),
        // StopBound observes every admitted candidate before pool truncation
        // and independently retains the least-cost qualifying one. Its stop
        // comparison is strict; next_up implements inclusive half-place bounds,
        // including a zero tolerance (only an exactly zero f64 distance passes).
        stop_at_exact: false,
        stop_below: Some(input_tolerance.next_up()),
        user_constants: gen.user_constants.clone(),
        user_functions: gen.user_functions.clone(),
        ranking_mode: RankingMode::Complexity,
        ..SearchConfig::default()
    }
}

fn validate_options(options: &DiscoveryOptions) -> Result<(), String> {
    if options.level > MAX_DISCOVERY_LEVEL {
        return Err(format!("Discovery level must be 0..={MAX_DISCOVERY_LEVEL}"));
    }
    if !(1..=MAX_DISCOVERY_MATCHES).contains(&options.max_matches) {
        return Err(format!(
            "Discovery max_matches must be 1..={MAX_DISCOVERY_MATCHES}"
        ));
    }
    if options
        .verification_bits
        .is_some_and(|bits| bits > MAX_DISCOVERY_VERIFICATION_BITS)
    {
        return Err(format!(
            "Discovery verification_bits must not exceed {MAX_DISCOVERY_VERIFICATION_BITS}"
        ));
    }
    Ok(())
}

fn run_search(gen: &GenConfig, config: &SearchConfig, parallel: bool) -> (Vec<Match>, SearchStats) {
    #[cfg(feature = "parallel")]
    if parallel {
        return crate::search::search_parallel_with_stats_and_config(gen, config);
    }
    #[cfg(not(feature = "parallel"))]
    let _ = parallel;
    crate::search::search_with_stats_and_config(gen, config)
}

/// Shared precision-aware prefix used by discovery and CLI Best reporting.
/// None means that raw ordering should decide between two unresolved matches.
pub(crate) fn compare_precision_resolved(
    a_error: f64,
    a_complexity: u32,
    b_error: f64,
    b_complexity: u32,
    tolerance: f64,
) -> Option<Ordering> {
    let a_resolved = a_error.abs() <= tolerance;
    let b_resolved = b_error.abs() <= tolerance;
    match (a_resolved, b_resolved) {
        (true, true) => Some(a_complexity.cmp(&b_complexity)),
        (true, false) => Some(Ordering::Less),
        (false, true) => Some(Ordering::Greater),
        (false, false) => None,
    }
}

fn compare_discovery_matches(a: &Match, b: &Match, tolerance: f64) -> Ordering {
    compare_precision_resolved(a.error, a.complexity, b.error, b.complexity, tolerance)
        .unwrap_or_else(|| compare_matches(a, b, RankingMode::Complexity))
        .then_with(|| compare_matches(a, b, RankingMode::Complexity))
}

fn hypothesis_key(summary: &MatchSummary) -> String {
    if let Some(postfix) = &summary.solve_for_x_postfix {
        if let Some(mut expression) = Expression::parse(postfix) {
            // Double negation is safe for every real domain. Do not cancel
            // reciprocals, roots, powers, logs, or trig functions by proximity.
            while expression.symbols().ends_with(&[Symbol::Neg, Symbol::Neg]) {
                expression.pop();
                expression.pop();
            }
            if let Some(key) = crate::canonical_expression_key(&expression) {
                return format!("solved:{key}");
            }
        }
    }
    format!("equation:{}", summary.canonical_key)
}

fn validation_for(
    matched: &Match,
    target: f64,
    bits: Option<u32>,
    gen: &GenConfig,
) -> DiscoveryValidation {
    let high_precision = if let Some(bits) = bits {
        let result = crate::verify_matches_highprec(
            vec![matched.clone()],
            target,
            bits,
            &gen.user_constants,
        )
        .into_iter()
        .next()
        .expect("one input match has one verification outcome");
        let (status, reason) = match result.status() {
            crate::VerificationStatus::Verified => ("verified", "Higher-precision numerical reevaluation corroborated this root; the target and custom constants remain their parsed f64 values."),
            crate::VerificationStatus::Failed => ("failed", "Higher-precision numerical reevaluation did not corroborate this root."),
            crate::VerificationStatus::Unavailable => ("unavailable", "This build, operator, or requested precision does not support higher-precision reevaluation."),
        };
        DiscoveryCheck {
            status: status.into(),
            reason: reason.into(),
            precision_bits: Some(bits),
            root_error: result.highprec_error.filter(|error| error.is_finite()),
        }
    } else {
        DiscoveryCheck {
            status: "not_requested".into(),
            reason: "Higher-precision numerical reevaluation was not requested.".into(),
            precision_bits: None,
            root_error: None,
        }
    };
    DiscoveryValidation {
        numerical: DiscoveryCheck {
            status: "numerical".into(),
            reason: "Search and root refinement use f64; numerical exactness and a small residual are not symbolic proof or a certified root interval.".into(),
            precision_bits: Some(f64::MANTISSA_DIGITS), root_error: Some(matched.error.abs()),
        },
        high_precision,
        polynomial_certificate: DiscoveryCheck {
            status: "unavailable".into(),
            reason: "Not computed by discovery. Export the original equation postfix and target literal to scripts/algebraic_certificate.py; only its documented restricted polynomial grammar can be independently bounded.".into(),
            precision_bits: None, root_error: None,
        },
    }
}

fn resolved_grammar(gen: &GenConfig) -> Vec<DiscoverySymbol> {
    std::iter::once(&Symbol::X)
        .chain(gen.constants.iter())
        .chain(gen.unary_ops.iter())
        .chain(gen.binary_ops.iter())
        .map(|&symbol| DiscoverySymbol {
            symbol: char::from(symbol as u8).to_string(),
            name: gen.symbol_table.name(symbol).into(),
            weight: gen.symbol_table.weight(symbol),
        })
        .collect()
}

fn summarize_stats(stats: &SearchStats, candidates: usize, hypotheses: usize) -> DiscoveryStats {
    let gen_ms = stats.gen_time.as_secs_f64() * 1000.0;
    let search_ms = stats.search_time.as_secs_f64() * 1000.0;
    DiscoveryStats {
        matcher: stats.matcher,
        lhs_count: stats.lhs_count,
        rhs_count: stats.rhs_count,
        candidates_tested: stats.candidates_tested,
        newton_calls: stats.newton_calls,
        gen_ms,
        search_ms,
        elapsed_ms: gen_ms + search_ms,
        returned_candidates: candidates,
        retained_hypotheses: hypotheses,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::expr::EvaluatedExpr;

    fn root_match(lhs: &str, rhs: &str, root: f64) -> Match {
        let lhs = Expression::parse(lhs).unwrap();
        let rhs = Expression::parse(rhs).unwrap();
        let left = crate::eval::evaluate(&lhs, root).unwrap();
        let right = crate::eval::evaluate(&rhs, root).unwrap();
        Match {
            complexity: lhs.complexity().saturating_add(rhs.complexity()),
            lhs: EvaluatedExpr::new(lhs, left.value, left.derivative, left.num_type),
            rhs: EvaluatedExpr::new(rhs, right.value, right.derivative, right.num_type),
            x_value: root,
            error: 0.0,
            exact: true,
        }
    }

    #[test]
    fn grouping_keeps_inverse_branches_and_unrelated_formulas_distinct() {
        let positive = MatchSummary::from_match(&root_match("xs", "2", 2.0_f64.sqrt()));
        let negative = MatchSummary::from_match(&root_match("xs", "2", -2.0_f64.sqrt()));
        let direct = MatchSummary::from_match(&root_match("x", "2q", 2.0_f64.sqrt()));
        assert_eq!(hypothesis_key(&positive), hypothesis_key(&direct));
        assert_ne!(hypothesis_key(&positive), hypothesis_key(&negative));

        let pi = MatchSummary::from_match(&root_match("x", "p", std::f64::consts::PI));
        let correction = MatchSummary::from_match(&root_match(
            "x",
            "p99^r-",
            std::f64::consts::PI - 9.0_f64.powi(-9),
        ));
        assert_ne!(hypothesis_key(&pi), hypothesis_key(&correction));
    }

    #[test]
    fn precision_order_uses_the_supplied_cost_not_default_symbol_weights() {
        let mut cheap = root_match("x", "p", std::f64::consts::PI);
        let mut precise = root_match("x", "p99^r-", std::f64::consts::PI - 9.0_f64.powi(-9));
        cheap.error = 3.6e-9;
        precise.error = 1e-9;
        cheap.exact = false;
        precise.exact = false;
        // Custom profiles can reverse the ordinary symbol costs. Selection must
        // use the actual recorded equation costs, not reparse default weights.
        cheap.complexity = 100;
        precise.complexity = 10;
        assert!(compare_discovery_matches(&precise, &cheap, 5e-9).is_lt());
        cheap.complexity = 1;
        assert!(compare_discovery_matches(&cheap, &precise, 5e-9).is_lt());
        assert!(compare_discovery_matches(&precise, &cheap, 2e-9).is_lt());
    }

    fn boundary_grammar(first: f64) -> GenConfig {
        let mut profile = Profile::new();
        profile
            .add_constant(1, "edge".into(), "Boundary control".into(), first)
            .unwrap();
        profile
            .add_constant(2, "zero".into(), "Exact control".into(), 0.0)
            .unwrap();
        let mut gen = build_gen_config_from_profile(20, 2, &profile).unwrap();
        gen.constants = vec![Symbol::UserConstant0, Symbol::UserConstant1];
        gen.unary_ops.clear();
        gen.binary_ops.clear();
        gen
    }

    #[test]
    fn inclusive_half_place_can_bound_search_at_the_cheaper_endpoint() {
        let gen = boundary_grammar(0.05);
        let mut config = build_search_config(0.0, 0.05, 1, &gen);
        // Keep both controls inside the matcher's independent adaptive
        // admission window, so this tests inclusive stopping rather than gates.
        config.max_error = 1.0;
        let (matches, _) = run_search(&gen, &config, false);
        // At a strict 0.05 stop threshold only the more costly exact zero
        // qualifies. Inclusive precision must keep the cheaper endpoint.
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].x_value, 0.05);
        assert_eq!(matches[0].rhs.expr.symbols(), &[Symbol::UserConstant0]);
    }

    #[test]
    fn zero_tolerance_does_not_treat_the_next_float_as_resolved() {
        let gen = boundary_grammar(f64::from_bits(1));
        let config = build_search_config(0.0, 0.0, 1, &gen);
        let (matches, _) = run_search(&gen, &config, false);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].x_value, 0.0);
        assert_eq!(matches[0].rhs.expr.symbols(), &[Symbol::UserConstant1]);
    }
}
