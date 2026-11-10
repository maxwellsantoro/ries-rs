//! Manifest building for RIES CLI
//!
//! This module provides functions for creating run manifests that capture
//! the full configuration and results of a search run for reproducibility.

use std::collections::{BTreeMap, HashMap, HashSet};

use ries_rs::gen::GenConfig;
use ries_rs::search::{SearchConfig, SearchStats};
use ries_rs::udf::UdfOp;
use ries_rs::{
    Match, MatchInfo, MatchMetrics, RunManifest, SearchConfigInfo, Symbol, UserConstantInfo,
};
use serde_json::{json, Value};

use super::json_types::{effective_thread_count, ranking_mode_name, uses_parallelism};
use super::Args;

/// Build a manifest from the effective settings and the ranked results.
pub fn build_manifest(
    args: &Args,
    gen: &GenConfig,
    search: &SearchConfig,
    level: f32,
    max_matches: usize,
    stats: &SearchStats,
    matches: &[Match],
) -> RunManifest {
    let matcher = stats.matcher;
    let parallel = uses_parallelism(matcher, args.parallel && !args.deterministic, gen);
    let config = SearchConfigInfo {
        target: search.target,
        level,
        max_lhs_complexity: gen.max_lhs_complexity,
        max_rhs_complexity: gen.max_rhs_complexity,
        deterministic: args.deterministic,
        parallel,
        max_error: search.max_error,
        max_matches,
        ranking_mode: ranking_mode_name(search.ranking_mode).to_string(),
        lhs_value_dedup: Some(search.lhs_value_dedup),
        lhs_value_dedup_applied: matcher
            .map(|path| path.applies_lhs_value_dedup(search.lhs_value_dedup)),
        matcher,
        user_constants: search
            .user_constants
            .iter()
            .map(|uc| UserConstantInfo {
                name: uc.name.clone(),
                value: uc.value,
                description: uc.description.clone(),
            })
            .collect(),
        excluded_symbols: args
            .exclude
            .as_ref()
            .map(|s| s.chars().map(|c| c.to_string()).collect())
            .unwrap_or_default(),
        allowed_symbols: args
            .only_symbols
            .as_ref()
            .map(|s| s.chars().map(|c| c.to_string()).collect()),
        replay: Some(replay_settings(args, gen, search, parallel)),
    };

    let results: Vec<MatchInfo> = matches
        .iter()
        .take(max_matches)
        .map(|m| {
            let stability = MatchMetrics::from_match(m, None).stability;
            MatchInfo {
                lhs_postfix: m.lhs.expr.to_postfix(),
                rhs_postfix: m.rhs.expr.to_postfix(),
                lhs_infix: m.lhs.expr.to_infix_with_context(
                    ries_rs::OutputFormat::Default,
                    Some(&gen.symbol_table),
                    search.trig_argument_scale,
                ),
                rhs_infix: m.rhs.expr.to_infix_with_context(
                    ries_rs::OutputFormat::Default,
                    Some(&gen.symbol_table),
                    search.trig_argument_scale,
                ),
                error: m.error.abs(),
                is_exact: m.exact,
                complexity: m.complexity,
                x_value: m.x_value,
                stability: Some(stability),
            }
        })
        .collect();

    RunManifest::new(config, results)
}

fn symbol_bytes(symbols: &[Symbol]) -> Vec<u8> {
    symbols.iter().map(|symbol| *symbol as u8).collect()
}

fn count_limits(limits: &HashMap<Symbol, u32>) -> BTreeMap<u8, u32> {
    limits
        .iter()
        .map(|(&symbol, &limit)| (symbol as u8, limit))
        .collect()
}

fn sorted_set(set: &HashSet<u8>) -> Vec<u8> {
    let mut symbols: Vec<_> = set.iter().copied().collect();
    symbols.sort_unstable();
    symbols
}

fn replay_settings(args: &Args, gen: &GenConfig, search: &SearchConfig, parallel: bool) -> Value {
    let symbol_table: BTreeMap<_, _> = (0..=u8::MAX)
        .filter_map(Symbol::from_byte)
        .map(|symbol| {
            (
                symbol as u8,
                json!({
                    "weight": gen.symbol_table.weight(symbol),
                    "name": gen.symbol_table.name(symbol),
                }),
            )
        })
        .collect();

    let constants: Vec<_> = search
        .user_constants
        .iter()
        .map(|constant| {
            json!({
                "name": constant.name,
                "description": constant.description,
                "value": constant.value,
                "weight": constant.weight,
                "num_type": constant.num_type as u8,
            })
        })
        .collect();
    let functions: Vec<_> = search
        .user_functions
        .iter()
        .map(|function| {
            let body: Vec<_> = function
                .body
                .iter()
                .map(|op| match op {
                    UdfOp::Symbol(symbol) => *symbol as u8,
                    UdfOp::Dup => b'|',
                    UdfOp::Swap => b'@',
                })
                .collect();
            json!({
                "name": function.name,
                "description": function.description,
                "body": body,
                "weight": function.weight,
                "num_type": function.num_type as u8,
            })
        })
        .collect();

    json!({
        "cli_args": replay_cli_args(args, search),
        "generation": {
            "max_length": gen.max_length,
            "min_num_type": gen.min_num_type as u8,
            "generate_lhs": gen.generate_lhs,
            "generate_rhs": gen.generate_rhs,
            "constants": symbol_bytes(&gen.constants),
            "unary_ops": symbol_bytes(&gen.unary_ops),
            "binary_ops": symbol_bytes(&gen.binary_ops),
            "symbol_max_counts": count_limits(&gen.symbol_max_counts),
            "rhs_constants": gen.rhs_constants.as_deref().map(symbol_bytes),
            "rhs_unary_ops": gen.rhs_unary_ops.as_deref().map(symbol_bytes),
            "rhs_binary_ops": gen.rhs_binary_ops.as_deref().map(symbol_bytes),
            "rhs_symbol_max_counts": gen.rhs_symbol_max_counts.as_ref().map(count_limits),
            "symbol_table": symbol_table,
            "user_constants": constants,
            "user_functions": functions,
        },
        "matching": {
            "pool_capacity": search.max_matches,
            "threads": effective_thread_count(parallel),
            "stop_at_exact": search.stop_at_exact,
            "stop_below": search.stop_below,
            "zero_value_threshold": search.zero_value_threshold,
            "newton_iterations": search.newton_iterations,
            "refine_with_newton": search.refine_with_newton,
            "derivative_margin": search.derivative_margin,
            "trig_argument_scale": search.trig_argument_scale,
            "rhs_allowed_symbols": search.rhs_allowed_symbols.as_ref().map(sorted_set),
            "rhs_excluded_symbols": search.rhs_excluded_symbols.as_ref().map(sorted_set),
            "min_match_distance": search.filters.min_match_distance,
            "min_equate_value": search.filters.min_equate_value,
            "max_equate_value": search.filters.max_equate_value,
            "numeric_anagram": search.filters.numeric_anagram,
            "canonical": search.filters.canonical,
            "rational_exponents": args.rational_exponents && !args.any_exponents,
            "rational_trig_args": args.rational_trig_args && !args.any_trig_args,
            "max_trig_cycles": args.max_trig_cycles,
        },
        "output": {
            "report": args.report && !args.classic,
            "top_k": args.top_k,
            "stable_category": !args.no_stable,
            "format": args.format,
            "absolute_roots": args.absolute,
            "solve_for_x": args.solve && !args.no_solve,
            "explicit_multiply": args.explicit_multiply,
            "precision_bits": args.precision,
            "highprec_available": cfg!(feature = "highprec"),
        },
    })
}

// Record an allowlisted, self-contained invocation. Profile files cannot safely be
// replayed from an untrusted manifest, and may change after the original run.
fn replay_cli_args(args: &Args, search: &SearchConfig) -> Option<Vec<String>> {
    if args.target.is_none() || args.profile.is_some() || !args.include.is_empty() {
        return None;
    }
    let mut out = vec![search.target.to_string()];
    macro_rules! value {
        ($name:literal, $value:expr) => {
            out.push(format!("--{}={}", $name, $value));
        };
    }
    macro_rules! optional {
        ($name:literal, $value:expr) => {
            if let Some(value) = &$value {
                value!($name, value);
            }
        };
    }
    macro_rules! flag {
        ($name:literal, $value:expr) => {
            if $value {
                out.push(format!("--{}", $name));
            }
        };
    }
    value!("level", args.level);
    value!("max-matches", args.max_matches);
    value!("report", args.report);
    value!("format", args.format);
    value!("top-k", args.top_k);
    value!("newton-iterations", args.newton_iterations);
    // Preserve the tolerance derived from the original literal's significant digits.
    if args.match_all_digits && args.max_match_distance.is_none() {
        value!("max-match-distance", search.max_error);
    } else {
        optional!("max-match-distance", args.max_match_distance);
    }
    optional!("exclude", args.exclude);
    optional!("enable", args.enable);
    optional!("only-symbols", args.only_symbols);
    optional!("op-limits", args.op_limits);
    optional!("S-RHS", args.only_symbols_rhs);
    optional!("N-RHS", args.exclude_rhs);
    optional!("E-RHS", args.enable_rhs);
    optional!("O-RHS", args.op_limits_rhs);
    optional!("symbol-weights", args.symbol_weights);
    optional!("symbol-names", args.symbol_names);
    optional!("preset", args.preset);
    optional!("stop-below", args.stop_below);
    optional!("min-match-distance", args.min_match_distance);
    optional!("precision", args.precision);
    optional!("zero-threshold", args.zero_threshold);
    optional!("canon-reduction", args.canon_reduction);
    optional!("derivative-margin", args.derivative_margin);
    optional!("max-equate-value", args.max_equate_value);
    optional!("min-equate-value", args.min_equate_value);
    optional!("max-memory", args.max_memory);
    optional!("min-memory", args.min_memory);
    optional!("memory-abort-threshold", args.memory_abort_threshold);
    optional!("max-trig-cycles", args.max_trig_cycles);
    optional!("significance-loss-margin", args.significance_loss_margin);
    optional!("trig-argument-scale", args.trig_argument_scale);
    for spec in &args.user_constant {
        value!("user-constant", spec);
    }
    for spec in &args.define {
        value!("define", spec);
    }
    flag!("absolute", args.absolute);
    flag!("solve", args.solve);
    flag!("no-solve-for-x", args.no_solve);
    flag!("algebraic", args.algebraic);
    flag!("constructible", args.constructible);
    flag!("rational", args.rational);
    flag!("integer", args.integer);
    flag!("ie", args.integer_exact);
    flag!("re", args.rational_exact);
    flag!("liouvillian-subexpressions", args.liouvillian);
    flag!("parallel", args.parallel);
    flag!("turbo", args.turbo);
    flag!("deterministic", args.deterministic);
    flag!("streaming", args.streaming);
    flag!("lhs-value-dedup", args.lhs_value_dedup);
    flag!("adaptive", args.adaptive);
    flag!("classic", args.classic);
    flag!("parity-ranking", args.parity_ranking);
    flag!("complexity-ranking", args.complexity_ranking);
    flag!("no-stable", args.no_stable);
    flag!("stability-check", args.stability_check);
    flag!("stability-thorough", args.stability_thorough);
    flag!("stop-at-exact", args.stop_at_exact);
    flag!("one-sided", args.one_sided);
    flag!("no-refinement", args.no_refinement);
    flag!("any-exponents", args.any_exponents);
    flag!("any-subexpressions", args.any_subexpressions);
    flag!("any-trig-args", args.any_trig_args);
    flag!("canon-simplify", args.canon_simplify);
    flag!("explicit-multiply", args.explicit_multiply);
    flag!("match-all-digits", args.match_all_digits);
    flag!("no-canon-simplify", args.no_canon_simplify);
    flag!("numeric-anagram", args.numeric_anagram);
    flag!("rational-exponents", args.rational_exponents);
    flag!("rational-trig-args", args.rational_trig_args);
    Some(out)
}
