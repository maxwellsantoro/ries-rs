//! Manifest building for RIES CLI
//!
//! This module provides functions for creating run manifests that capture
//! the full configuration and results of a search run for reproducibility.

use std::collections::{BTreeMap, HashMap, HashSet};

use ries_rs::gen::GenConfig;
use ries_rs::search::{MatcherPath, SearchConfig, SearchStats};
use ries_rs::udf::UdfOp;
use ries_rs::{
    Match, MatchInfo, MatchMetrics, RunManifest, SearchConfigInfo, Symbol, UserConstantInfo,
};
use serde_json::{json, Value};

use super::json_types::{effective_thread_count, ranking_mode_name};
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
    let parallel = cfg!(feature = "parallel")
        && match matcher {
            Some(MatcherPath::Parallel | MatcherPath::Turbo) => true,
            Some(MatcherPath::Adaptive) => args.parallel && !args.deterministic,
            _ => false,
        };
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
                lhs_infix: m.lhs.expr.to_infix_with_table(&gen.symbol_table),
                rhs_infix: m.rhs.expr.to_infix_with_table(&gen.symbol_table),
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
