//! Manifest building for RIES CLI
//!
//! This module provides functions for creating run manifests that capture
//! the full configuration and results of a search run for reproducibility.

use ries_rs::pool::RankingMode;
use ries_rs::{
    Match, MatchInfo, MatchMetrics, RunManifest, SearchConfigInfo, UserConstant, UserConstantInfo,
    EXACT_MATCH_TOLERANCE,
};

/// Build a manifest from the search results
#[allow(clippy::too_many_arguments)]
pub fn build_manifest(
    target: f64,
    level: f32,
    max_lhs_complexity: u32,
    max_rhs_complexity: u32,
    deterministic: bool,
    parallel: bool,
    max_error: f64,
    max_matches: usize,
    ranking_mode: RankingMode,
    user_constants: &[UserConstant],
    excluded_symbols: &Option<String>,
    allowed_symbols: &Option<String>,
    matches: &[Match],
) -> RunManifest {
    let config = SearchConfigInfo {
        target,
        level,
        max_lhs_complexity,
        max_rhs_complexity,
        deterministic,
        parallel: !deterministic && parallel,
        max_error,
        max_matches,
        ranking_mode: match ranking_mode {
            RankingMode::Complexity => "complexity".to_string(),
            RankingMode::Parity => "parity".to_string(),
        },
        user_constants: user_constants
            .iter()
            .map(|uc| UserConstantInfo {
                name: uc.name.clone(),
                value: uc.value,
                description: uc.description.clone(),
            })
            .collect(),
        excluded_symbols: excluded_symbols
            .as_ref()
            .map(|s| s.chars().map(|c| c.to_string()).collect())
            .unwrap_or_default(),
        allowed_symbols: allowed_symbols
            .as_ref()
            .map(|s| s.chars().map(|c| c.to_string()).collect()),
    };

    let results: Vec<MatchInfo> = matches
        .iter()
        .take(max_matches)
        .map(|m| {
            let stability = MatchMetrics::from_match(m, None).stability;
            MatchInfo {
                lhs_postfix: m.lhs.expr.to_postfix(),
                rhs_postfix: m.rhs.expr.to_postfix(),
                lhs_infix: m.lhs.expr.to_infix(),
                rhs_infix: m.rhs.expr.to_infix(),
                error: m.error.abs(),
                is_exact: m.error.abs() < EXACT_MATCH_TOLERANCE,
                complexity: m.complexity,
                x_value: m.x_value,
                stability: Some(stability),
            }
        })
        .collect();

    RunManifest::new(config, results)
}
