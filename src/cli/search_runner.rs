//! Search execution and orchestration
//!
//! This module handles the coordination of search execution,
//! including configuration building and result processing.

use crate::search::{Match, SearchConfig, SearchStats};
use std::time::{Duration, Instant};

/// Result of a search operation
pub struct SearchResult {
    /// The matches found during the search
    pub matches: Vec<Match>,
    /// Statistics collected during the search
    pub stats: SearchStats,
    /// Time elapsed during the search
    pub elapsed: Duration,
}

/// Orchestrates the search process
///
/// This function handles the complexity of choosing the appropriate search mode
/// (adaptive, streaming, parallel, one-sided) based on the configuration flags.
#[allow(clippy::too_many_arguments)]
pub fn run_search(
    gen_config: &crate::gen::GenConfig,
    search_config: &SearchConfig,
    streaming: bool,
    parallel: bool,
    one_sided: bool,
    adaptive: bool,
    level: u32,
) -> SearchResult {
    let start = Instant::now();

    let (matches, stats) = if one_sided {
        crate::search::search_one_sided_with_stats_and_config(gen_config, search_config)
    } else if adaptive {
        crate::search::search_adaptive(gen_config, search_config, level)
    } else if streaming {
        crate::search::search_streaming_with_config(gen_config, search_config)
    } else {
        #[cfg(feature = "parallel")]
        {
            if parallel {
                crate::search::search_parallel_with_stats_and_config(gen_config, search_config)
            } else {
                crate::search::search_with_stats_and_config(gen_config, search_config)
            }
        }
        #[cfg(not(feature = "parallel"))]
        {
            let _ = parallel;
            crate::search::search_with_stats_and_config(gen_config, search_config)
        }
    };

    let elapsed = start.elapsed();

    SearchResult {
        matches,
        stats,
        elapsed,
    }
}
