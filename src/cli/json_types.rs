//! JSON output types and building for RIES CLI
//!
//! This module provides types and functions for serializing search results
//! to JSON format.

use ries_rs::expr::OutputFormat;
use ries_rs::pool::RankingMode;
use ries_rs::search::SearchStats;
use ries_rs::{SymbolTable, EXACT_MATCH_TOLERANCE};
use serde::Serialize;

use super::DisplayFormat;

/// JSON output structure for a complete search run
#[derive(Serialize)]
pub struct JsonRunOutput {
    pub target: f64,
    pub search_level: f32,
    pub max_lhs_complexity: u32,
    pub max_rhs_complexity: u32,
    pub max_matches: usize,
    pub results_returned: usize,
    pub ranking_mode: &'static str,
    pub deterministic: bool,
    pub parallel: bool,
    pub streaming: bool,
    pub adaptive: bool,
    pub one_sided: bool,
    pub report_mode_requested: bool,
    pub output_format: String,
    pub results: Vec<JsonMatch>,
    pub search_stats: JsonSearchStats,
}

/// JSON structure for a single match result
#[derive(Serialize)]
pub struct JsonMatch {
    pub equation: String,
    pub lhs: String,
    pub rhs: String,
    pub lhs_postfix: String,
    pub rhs_postfix: String,
    pub solve_for_x: Option<String>,
    pub solve_for_x_postfix: Option<String>,
    pub canonical_key: String,
    pub x_value: f64,
    pub error: f64,
    pub exact: bool,
    pub complexity: u32,
    pub operator_count: usize,
    pub tree_depth: usize,
}

/// JSON structure for search statistics
#[derive(Serialize)]
pub struct JsonSearchStats {
    pub expressions_generated_lhs: usize,
    pub expressions_generated_rhs: usize,
    pub expressions_generated_total: usize,
    pub lhs_expressions_tested: usize,
    pub lhs_expressions_pruned: usize,
    pub candidate_pairs_tested: usize,
    pub newton_calls: usize,
    pub newton_success: usize,
    pub pool_insertions: usize,
    pub duplicates_eliminated: usize,
    pub pool_rejections_error: usize,
    pub pool_evictions: usize,
    pub pool_final_size: usize,
    pub best_error: f64,
    pub generation_ms: f64,
    pub search_ms: f64,
    pub elapsed_ms: f64,
    pub threads: usize,
    pub peak_memory_bytes: Option<u64>,
    pub early_exit: bool,
}

/// Get the ranking mode as a string
pub fn ranking_mode_name(mode: RankingMode) -> &'static str {
    match mode {
        RankingMode::Complexity => "complexity",
        RankingMode::Parity => "parity",
    }
}

/// Get effective thread count based on parallel setting
pub fn effective_thread_count(parallel_enabled: bool) -> usize {
    if !parallel_enabled {
        return 1;
    }

    #[cfg(feature = "parallel")]
    {
        rayon::current_num_threads()
    }
    #[cfg(not(feature = "parallel"))]
    {
        1
    }
}

/// Build JSON output from search results
#[allow(clippy::too_many_arguments)]
pub fn build_json_output(
    target: f64,
    search_level: f32,
    max_lhs_complexity: u32,
    max_rhs_complexity: u32,
    max_matches: usize,
    ranking_mode: RankingMode,
    deterministic: bool,
    parallel: bool,
    streaming: bool,
    adaptive: bool,
    one_sided: bool,
    report_mode_requested: bool,
    output_format: DisplayFormat,
    explicit_multiply: bool,
    symbol_table: &SymbolTable,
    matches: &[crate::search::Match],
    stats: &SearchStats,
    elapsed: std::time::Duration,
    include_solve_for_x: bool,
) -> JsonRunOutput {
    let output_format_name = match output_format {
        DisplayFormat::Infix(OutputFormat::Default) => "default",
        DisplayFormat::Infix(OutputFormat::Pretty) => "pretty",
        DisplayFormat::Infix(OutputFormat::Mathematica) => "mathematica",
        DisplayFormat::Infix(OutputFormat::SymPy) => "sympy",
        DisplayFormat::PostfixCompact => "postfix-compact",
        DisplayFormat::PostfixVerbose => "postfix-verbose",
        DisplayFormat::Condensed => "condensed",
    }
    .to_string();

    let results = matches
        .iter()
        .map(|m| {
            use super::output::format_expression_for_display;
            use crate::{canonical_expression_key, solve_for_x_rhs_expression};

            let lhs = format_expression_for_display(
                &m.lhs.expr,
                output_format,
                explicit_multiply,
                Some(symbol_table),
            );
            let rhs = format_expression_for_display(
                &m.rhs.expr,
                output_format,
                explicit_multiply,
                Some(symbol_table),
            );

            // Analytical solver
            let (solve_for_x, solve_for_x_postfix) = if include_solve_for_x {
                let solved = solve_for_x_rhs_expression(&m.lhs.expr, &m.rhs.expr);
                (
                    solved
                        .as_ref()
                        .map(|e: &crate::expr::Expression| format!("x = {}", e.to_infix())),
                    solved
                        .as_ref()
                        .map(|e: &crate::expr::Expression| e.to_postfix()),
                )
            } else {
                (None, None)
            };

            // Canonical key
            let canonical_key = canonical_expression_key(&m.lhs.expr)
                .zip(canonical_expression_key(&m.rhs.expr))
                .map(|(l, r)| format!("{}={}", l, r))
                .unwrap_or_else(|| {
                    format!("{}={}", m.lhs.expr.to_postfix(), m.rhs.expr.to_postfix())
                });

            JsonMatch {
                equation: format!("{lhs} = {rhs}"),
                lhs,
                rhs,
                lhs_postfix: m.lhs.expr.to_postfix(),
                rhs_postfix: m.rhs.expr.to_postfix(),
                solve_for_x,
                solve_for_x_postfix,
                canonical_key,
                x_value: m.x_value,
                error: m.error,
                exact: m.error.abs() < EXACT_MATCH_TOLERANCE,
                complexity: m.complexity,
                operator_count: m.lhs.expr.operator_count() + m.rhs.expr.operator_count(),
                tree_depth: m.lhs.expr.tree_depth().max(m.rhs.expr.tree_depth()),
            }
        })
        .collect::<Vec<_>>();

    let thread_count = effective_thread_count(parallel);
    let lhs_pruned = stats.lhs_count.saturating_sub(stats.lhs_tested);
    let peak_memory = peak_memory_bytes();
    JsonRunOutput {
        target,
        search_level,
        max_lhs_complexity,
        max_rhs_complexity,
        max_matches,
        results_returned: results.len(),
        ranking_mode: ranking_mode_name(ranking_mode),
        deterministic,
        parallel,
        streaming,
        adaptive,
        one_sided,
        report_mode_requested,
        output_format: output_format_name,
        results,
        search_stats: JsonSearchStats {
            expressions_generated_lhs: stats.lhs_count,
            expressions_generated_rhs: stats.rhs_count,
            expressions_generated_total: stats.lhs_count.saturating_add(stats.rhs_count),
            lhs_expressions_tested: stats.lhs_tested,
            lhs_expressions_pruned: lhs_pruned,
            candidate_pairs_tested: stats.candidates_tested,
            newton_calls: stats.newton_calls,
            newton_success: stats.newton_success,
            pool_insertions: stats.pool_insertions,
            duplicates_eliminated: stats.pool_rejections_dedupe,
            pool_rejections_error: stats.pool_rejections_error,
            pool_evictions: stats.pool_evictions,
            pool_final_size: stats.pool_final_size,
            best_error: stats.pool_best_error,
            generation_ms: stats.gen_time.as_secs_f64() * 1000.0,
            search_ms: stats.search_time.as_secs_f64() * 1000.0,
            elapsed_ms: elapsed.as_secs_f64() * 1000.0,
            threads: thread_count,
            peak_memory_bytes: peak_memory,
            early_exit: stats.early_exit,
        },
    }
}

/// Format bytes in binary units (B, KiB, MiB, GiB, TiB)
pub fn format_bytes_binary(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    let mut value = bytes as f64;
    let mut unit_idx = 0;
    while value >= 1024.0 && unit_idx + 1 < UNITS.len() {
        value /= 1024.0;
        unit_idx += 1;
    }
    if unit_idx == 0 {
        format!("{} {}", bytes, UNITS[unit_idx])
    } else {
        format!("{value:.2} {}", UNITS[unit_idx])
    }
}

/// Get peak memory usage in bytes (Unix only)
#[cfg(unix)]
pub fn peak_memory_bytes() -> Option<u64> {
    let mut usage = std::mem::MaybeUninit::<libc::rusage>::uninit();
    // SAFETY: `usage` points to valid writable memory for `getrusage`.
    let rc = unsafe { libc::getrusage(libc::RUSAGE_SELF, usage.as_mut_ptr()) };
    if rc != 0 {
        return None;
    }
    // SAFETY: `getrusage` succeeded and initialized the struct.
    let usage = unsafe { usage.assume_init() };
    let raw = usage.ru_maxrss;
    if raw < 0 {
        return None;
    }
    let rss = raw as u64;

    #[cfg(any(target_os = "linux", target_os = "android"))]
    {
        Some(rss.saturating_mul(1024))
    }
    #[cfg(not(any(target_os = "linux", target_os = "android")))]
    {
        // macOS/BSD report ru_maxrss in bytes.
        Some(rss)
    }
}

#[cfg(not(unix))]
pub fn peak_memory_bytes() -> Option<u64> {
    None
}
