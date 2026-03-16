//! Search and matching algorithms
//!
//! Finds equations by matching LHS and RHS expressions.

use crate::eval::EvalContext;
use crate::expr::EvaluatedExpr;
use crate::pool::{RankingMode, TopKPool};
use crate::profile::UserConstant;
use crate::thresholds::{
    DEGENERATE_DERIVATIVE, DEGENERATE_RANGE_TOLERANCE, DEGENERATE_TEST_THRESHOLD,
    EXACT_MATCH_TOLERANCE, NEWTON_FINAL_TOLERANCE,
};
use std::collections::HashSet;
use std::time::Duration;

mod db;
mod newton;
#[cfg(test)]
mod tests;

use db::calculate_adaptive_search_radius;
pub use db::{ComplexityTier, ExprDatabase, TieredExprDatabase};
#[cfg(test)]
use newton::newton_raphson;
use newton::newton_raphson_with_constants;

#[derive(Clone, Copy, Debug)]
struct SearchTimer {
    #[cfg(all(target_arch = "wasm32", feature = "wasm"))]
    start_ms: f64,
    #[cfg(not(all(target_arch = "wasm32", feature = "wasm")))]
    start: std::time::Instant,
}

impl SearchTimer {
    #[inline]
    fn start() -> Self {
        #[cfg(all(target_arch = "wasm32", feature = "wasm"))]
        {
            Self {
                start_ms: js_sys::Date::now(),
            }
        }

        #[cfg(not(all(target_arch = "wasm32", feature = "wasm")))]
        {
            Self {
                start: std::time::Instant::now(),
            }
        }
    }

    #[inline]
    fn elapsed(self) -> Duration {
        #[cfg(all(target_arch = "wasm32", feature = "wasm"))]
        {
            let elapsed_ms = (js_sys::Date::now() - self.start_ms).max(0.0);
            Duration::from_secs_f64(elapsed_ms / 1000.0)
        }

        #[cfg(not(all(target_arch = "wasm32", feature = "wasm")))]
        {
            self.start.elapsed()
        }
    }
}

/// Statistics collected during search
#[derive(Clone, Debug, Default)]
pub struct SearchStats {
    /// Time spent generating expressions
    pub gen_time: Duration,
    /// Time spent searching/matching
    pub search_time: Duration,
    /// Number of LHS expressions generated
    pub lhs_count: usize,
    /// Number of RHS expressions generated
    pub rhs_count: usize,
    /// Number of LHS expressions tested (after filtering)
    pub lhs_tested: usize,
    /// Number of candidate pairs tested (coarse filter)
    pub candidates_tested: usize,
    /// Number of Newton-Raphson calls
    pub newton_calls: usize,
    /// Number of successful Newton convergences
    pub newton_success: usize,
    /// Number of matches inserted into pool
    pub pool_insertions: usize,
    /// Number of matches rejected by pool (error threshold)
    pub pool_rejections_error: usize,
    /// Number of matches rejected by pool (dedupe)
    pub pool_rejections_dedupe: usize,
    /// Number of matches evicted from pool
    pub pool_evictions: usize,
    /// Final pool size
    pub pool_final_size: usize,
    /// Final best error in pool
    pub pool_best_error: f64,
    /// Whether search exited early
    pub early_exit: bool,
}

impl SearchStats {
    pub fn new() -> Self {
        Self::default()
    }

    /// Print stats to stdout
    pub fn print(&self) {
        println!();
        println!("  === Search Statistics ===");
        println!();
        println!("  Generation:");
        println!(
            "    Time:            {:>10.3}ms",
            self.gen_time.as_secs_f64() * 1000.0
        );
        println!("    LHS expressions: {:>10}", self.lhs_count);
        println!("    RHS expressions: {:>10}", self.rhs_count);
        println!();
        println!("  Search:");
        println!(
            "    Time:            {:>10.3}ms",
            self.search_time.as_secs_f64() * 1000.0
        );
        println!("    LHS tested:      {:>10}", self.lhs_tested);
        println!("    Candidates:      {:>10}", self.candidates_tested);
        println!("    Newton calls:    {:>10}", self.newton_calls);
        println!(
            "    Newton success:  {:>10} ({:.1}%)",
            self.newton_success,
            if self.newton_calls > 0 {
                100.0 * self.newton_success as f64 / self.newton_calls as f64
            } else {
                0.0
            }
        );
        if self.early_exit {
            println!("    Early exit:      yes");
        }
        println!();
        println!("  Pool:");
        println!("    Insertions:      {:>10}", self.pool_insertions);
        println!("    Rejected (err):  {:>10}", self.pool_rejections_error);
        println!("    Rejected (dup):  {:>10}", self.pool_rejections_dedupe);
        println!("    Evictions:       {:>10}", self.pool_evictions);
        println!("    Final size:      {:>10}", self.pool_final_size);
        println!("    Best error:      {:>14.2e}", self.pool_best_error);
    }
}

/// Compute complexity limits from a search level.
///
/// This function provides a consistent mapping from integer search levels
/// to complexity limits used by the library API (Python, WASM, and adaptive mode).
///
/// # Formula
///
/// - Base LHS complexity: 10
/// - Base RHS complexity: 12
/// - Level multiplier: 4
/// - `max_lhs = 10 + 4 * level`
/// - `max_rhs = 12 + 4 * level`
///
/// # Level Guidelines
///
/// | Level | LHS Max | RHS Max | Expression Count (approx) |
/// |-------|---------|---------|---------------------------|
/// | 0     | 10      | 12      | ~35K                      |
/// | 1     | 14      | 16      | ~130K                     |
/// | 2     | 18      | 20      | ~500K                     |
/// | 3     | 22      | 24      | ~2M                       |
/// | 5     | 30      | 32      | ~15M                      |
///
/// # Note
///
/// The CLI uses a different formula with higher bases (35/35) and multiplier (10)
/// to match the original RIES command-line behavior. This function is for
/// programmatic API consumers.
#[inline]
pub fn level_to_complexity(level: u32) -> (u32, u32) {
    const BASE_LHS: u32 = 10;
    const BASE_RHS: u32 = 12;
    const LEVEL_MULTIPLIER: u32 = 4;

    // Saturating arithmetic avoids panics/wraparound if an API caller passes
    // an out-of-range level without validation.
    let level_factor = LEVEL_MULTIPLIER.saturating_mul(level);
    (
        BASE_LHS.saturating_add(level_factor),
        BASE_RHS.saturating_add(level_factor),
    )
}

/// A matched equation
#[derive(Clone, Debug)]
pub struct Match {
    /// Left-hand side expression (contains x)
    pub lhs: EvaluatedExpr,
    /// Right-hand side expression (constant)
    pub rhs: EvaluatedExpr,
    /// Solved value of x
    pub x_value: f64,
    /// Difference from target: x_value - target
    pub error: f64,
    /// Total complexity (LHS + RHS)
    pub complexity: u32,
}

impl Match {
    /// Format the match for display (used in tests)
    #[cfg(test)]
    pub fn display(&self, _target: f64) -> String {
        let lhs_str = self.lhs.expr.to_infix();
        let rhs_str = self.rhs.expr.to_infix();

        let error_str = if self.error.abs() < EXACT_MATCH_TOLERANCE {
            "('exact' match)".to_string()
        } else {
            let sign = if self.error >= 0.0 { "+" } else { "-" };
            format!("for x = T {} {:.6e}", sign, self.error.abs())
        };

        format!(
            "{:>24} = {:<24} {} {{{}}}",
            lhs_str, rhs_str, error_str, self.complexity
        )
    }
}

/// Configuration for the search process.
///
/// Controls matching thresholds, result limits, symbol filtering, and search behavior.
/// This struct is the main entry point for customizing how RIES searches for equations.
///
/// # Example
///
/// ```rust
/// use ries_rs::search::SearchConfig;
/// use ries_rs::pool::RankingMode;
///
/// let config = SearchConfig {
///     target: std::f64::consts::PI,
///     max_matches: 50,
///     max_error: 1e-10,
///     stop_at_exact: true,
///     ranking_mode: RankingMode::Complexity,
///     ..SearchConfig::default()
/// };
/// ```
///
/// # Fields Overview
///
/// - **Target**: `target` - the value to search for equations matching it
/// - **Limits**: `max_matches`, `max_error` - control result quantity and quality
/// - **Stopping**: `stop_at_exact`, `stop_below` - early termination conditions
/// - **Refinement**: `newton_iterations`, `refine_with_newton`, `derivative_margin` - Newton-Raphson settings
/// - **Filtering**: `zero_value_threshold`, `rhs_allowed_symbols`, `rhs_excluded_symbols` - prune unwanted results
/// - **Extensions**: `user_constants`, `user_functions` - custom symbols
/// - **Diagnostics**: `show_newton`, `show_match_checks`, etc. - debug output
/// - **Ranking**: `ranking_mode` - how to order results
#[derive(Clone, Debug)]
pub struct SearchConfig {
    /// Target value to find equations for.
    ///
    /// The search will find equations where LHS ≈ RHS ≈ target.
    /// This is the number you're trying to "solve" or represent symbolically.
    ///
    /// Default: 0.0
    pub target: f64,

    /// Maximum number of matches to return in results.
    ///
    /// Once this many matches are found and confirmed, the pool will start
    /// evicting lower-quality matches to make room for better ones.
    ///
    /// Default: 100
    pub max_matches: usize,

    /// Maximum acceptable error for a match to be included.
    ///
    /// Only expressions within this absolute error from the target are considered matches.
    /// Smaller values give more precise but fewer results.
    ///
    /// Default: 1.0
    pub max_error: f64,

    /// Stop search when an exact match is found.
    ///
    /// When true and an expression matches within the exact match tolerance,
    /// the search terminates early. Useful when you only need one good solution.
    ///
    /// Default: false
    pub stop_at_exact: bool,

    /// Stop search when error goes below this threshold.
    ///
    /// If set, the search will terminate once a match is found with error
    /// below this value. Set to `Some(1e-12)` for high-precision early stopping.
    ///
    /// Default: None
    pub stop_below: Option<f64>,

    /// Threshold for pruning LHS expressions with near-zero values.
    ///
    /// LHS expressions with absolute values below this threshold are pruned
    /// to prevent flooding results with trivial matches like `cospi(2.5) = 0`.
    /// Set to 0.0 to disable this filtering.
    ///
    /// Default: 1e-4
    pub zero_value_threshold: f64,

    /// Maximum Newton-Raphson iterations for root refinement.
    ///
    /// Controls how many iterations are used to refine candidate solutions.
    /// Higher values may find more precise roots but take longer.
    ///
    /// Default: 15
    pub newton_iterations: usize,

    /// User-defined constants for evaluation.
    ///
    /// Custom constants that can be used in expressions, defined via `-N\'name=value\'`.
    /// Each constant has a name, value, and optional description.
    ///
    /// Default: empty
    pub user_constants: Vec<UserConstant>,

    /// User-defined functions for evaluation.
    ///
    /// Custom functions that can be used in expressions, defined via `-F\'name:formula\'`.
    /// These extend the built-in functions (sin, cos, etc.).
    ///
    /// Default: empty
    pub user_functions: Vec<crate::udf::UserFunction>,

    /// Argument scale for `sinpi/cospi/tanpi` evaluation.
    ///
    /// The default is π, matching original `sinpi(x) = sin(πx)` semantics.
    /// Override this for alternate trig conventions without relying on global state.
    pub trig_argument_scale: f64,

    /// Whether to refine candidate roots with Newton-Raphson iteration.
    ///
    /// When true, initial coarse matches are refined using Newton-Raphson
    /// to find more precise solutions. Disable for faster but less precise search.
    ///
    /// Default: true
    pub refine_with_newton: bool,

    /// Optional RHS-only allowed symbol set.
    ///
    /// If set, all symbols used on the RHS must be in this set (specified as ASCII bytes).
    /// This allows restricting RHS expressions to a subset of available symbols.
    /// Combined with `rhs_excluded_symbols`, both filters apply.
    ///
    /// Default: None (all symbols allowed)
    pub rhs_allowed_symbols: Option<HashSet<u8>>,

    /// Optional RHS-only excluded symbol set.
    ///
    /// If set, RHS expressions using any of these symbols are rejected.
    /// This allows excluding specific symbols from RHS expressions.
    /// Combined with `rhs_allowed_symbols`, both filters apply.
    ///
    /// Default: None (no symbols excluded)
    pub rhs_excluded_symbols: Option<HashSet<u8>>,

    /// Show Newton-Raphson iteration diagnostic output.
    ///
    /// When true, prints detailed information about each Newton-Raphson iteration.
    /// Enabled by `-Dn` command-line flag. Useful for debugging convergence issues.
    ///
    /// Default: false
    pub show_newton: bool,

    /// Show match check diagnostic output.
    ///
    /// When true, prints information about each candidate match evaluation.
    /// Enabled by `-Do` command-line flag.
    ///
    /// Default: false
    pub show_match_checks: bool,

    /// Show pruned arithmetic diagnostic output.
    ///
    /// When true, prints information about arithmetic expressions that were pruned.
    /// Enabled by `-DA` command-line flag.
    ///
    /// Default: false
    #[allow(dead_code)]
    pub show_pruned_arith: bool,

    /// Show pruned range/zero diagnostic output.
    ///
    /// When true, prints information about expressions pruned due to range
    /// constraints or near-zero values. Enabled by `-DB` command-line flag.
    ///
    /// Default: false
    pub show_pruned_range: bool,

    /// Show database adds diagnostic output.
    ///
    /// When true, prints information about expressions added to the database.
    /// Enabled by `-DG` command-line flag.
    ///
    /// Default: false
    pub show_db_adds: bool,

    /// When true, matches must match all significant digits of the target.
    ///
    /// When enabled, the tolerance is computed based on the magnitude of the
    /// target value to require full precision matching. The actual tolerance
    /// is computed in `main.rs` and passed as `max_error`.
    ///
    /// Default: false
    #[allow(dead_code)]
    pub match_all_digits: bool,

    /// Threshold below which derivative is considered degenerate in Newton-Raphson.
    ///
    /// If `|derivative| < derivative_margin` during Newton-Raphson iteration,
    /// the refinement is skipped to avoid numerical instability and division
    /// by near-zero values.
    ///
    /// Default: 1e-12 (from `DEGENERATE_DERIVATIVE` constant)
    pub derivative_margin: f64,

    /// Ranking mode for pool ordering and eviction.
    ///
    /// Determines how matches are ranked in the result pool:
    /// - `Complexity`: Sort by expression complexity (simpler is better)
    /// - `Error`: Sort by match error (more precise is better)
    ///
    /// Default: `RankingMode::Complexity`
    pub ranking_mode: RankingMode,
}

impl Default for SearchConfig {
    fn default() -> Self {
        Self {
            target: 0.0,
            max_matches: 100,
            max_error: 1.0,
            stop_at_exact: false,
            stop_below: None,
            zero_value_threshold: 1e-4,
            newton_iterations: 15,
            user_constants: Vec::new(),
            user_functions: Vec::new(),
            trig_argument_scale: crate::eval::DEFAULT_TRIG_ARGUMENT_SCALE,
            refine_with_newton: true,
            rhs_allowed_symbols: None,
            rhs_excluded_symbols: None,
            show_newton: false,
            show_match_checks: false,
            show_pruned_arith: false,
            show_pruned_range: false,
            show_db_adds: false,
            match_all_digits: false,
            derivative_margin: DEGENERATE_DERIVATIVE,
            ranking_mode: RankingMode::Complexity,
        }
    }
}

impl SearchConfig {
    /// Build an explicit search context for this configuration.
    pub fn context(&self) -> SearchContext<'_> {
        SearchContext::new(self)
    }

    #[inline]
    fn rhs_symbol_allowed(&self, rhs: &crate::expr::Expression) -> bool {
        let symbols = rhs.symbols();

        if let Some(allowed) = &self.rhs_allowed_symbols {
            if symbols.iter().any(|s| !allowed.contains(&(*s as u8))) {
                return false;
            }
        }

        if let Some(excluded) = &self.rhs_excluded_symbols {
            if symbols.iter().any(|s| excluded.contains(&(*s as u8))) {
                return false;
            }
        }

        true
    }
}

/// Explicit per-run search context.
#[derive(Clone, Copy, Debug)]
pub struct SearchContext<'a> {
    /// Immutable search configuration for this run.
    pub config: &'a SearchConfig,
    /// Evaluation context derived from the search configuration.
    pub eval: EvalContext<'a>,
}

impl<'a> SearchContext<'a> {
    pub fn new(config: &'a SearchConfig) -> Self {
        Self {
            config,
            eval: EvalContext::from_slices(&config.user_constants, &config.user_functions)
                .with_trig_argument_scale(config.trig_argument_scale),
        }
    }
}

/// Perform a complete search
///
/// This function is part of the public API for library consumers who want a simple
/// search interface without statistics collection.
#[allow(dead_code)]
pub fn search(target: f64, gen_config: &crate::gen::GenConfig, max_matches: usize) -> Vec<Match> {
    let (matches, _stats) = search_with_stats(target, gen_config, max_matches);
    matches
}

/// Perform a complete search with statistics collection
///
/// This function is part of the public API for library consumers who want
/// detailed statistics about the search process.
#[allow(dead_code)]
pub fn search_with_stats(
    target: f64,
    gen_config: &crate::gen::GenConfig,
    max_matches: usize,
) -> (Vec<Match>, SearchStats) {
    search_with_stats_and_options(target, gen_config, max_matches, false, None)
}

/// Perform a complete search with statistics collection and early exit options
pub fn search_with_stats_and_options(
    target: f64,
    gen_config: &crate::gen::GenConfig,
    max_matches: usize,
    stop_at_exact: bool,
    stop_below: Option<f64>,
) -> (Vec<Match>, SearchStats) {
    let config = SearchConfig {
        target,
        max_matches,
        stop_at_exact,
        stop_below,
        user_constants: gen_config.user_constants.clone(),
        user_functions: gen_config.user_functions.clone(),
        ..Default::default()
    };

    search_with_stats_and_config(gen_config, &config)
}

/// Perform a complete search with a fully specified search configuration.
///
/// This function includes a safety fallback: if expression generation would
/// exceed ~2M expressions (which would risk OOM), it automatically switches
/// to streaming mode to avoid memory exhaustion.
pub fn search_with_stats_and_config(
    gen_config: &crate::gen::GenConfig,
    config: &SearchConfig,
) -> (Vec<Match>, SearchStats) {
    use crate::gen::generate_all_with_limit_and_context;

    const MAX_EXPRESSIONS_BEFORE_STREAMING: usize = 2_000_000;
    let context = SearchContext::new(config);

    let gen_start = SearchTimer::start();

    // Try bounded generation first — if limit exceeded, fall back to streaming
    if let Some(generated) = generate_all_with_limit_and_context(
        gen_config,
        config.target,
        &context.eval,
        MAX_EXPRESSIONS_BEFORE_STREAMING,
    ) {
        let gen_time = gen_start.elapsed();

        // Build database
        let mut db = ExprDatabase::new();
        db.insert_rhs(generated.rhs);

        // Find matches with stats
        let (matches, mut stats) = db.find_matches_with_stats_and_context(&generated.lhs, &context);

        // Add generation stats
        stats.gen_time = gen_time;
        stats.lhs_count = generated.lhs.len();
        stats.rhs_count = db.rhs_count();

        (matches, stats)
    } else {
        // Limit exceeded — fall back to streaming mode which avoids OOM
        search_streaming_with_config(gen_config, config)
    }
}

// =============================================================================
// ADAPTIVE SEARCH - Iterative LHS/RHS expansion like original RIES
// =============================================================================

/// Perform an adaptive search that iteratively expands LHS/RHS complexity
///
/// This implements the original RIES algorithm where expressions are generated
/// incrementally, expanding the side (LHS or RHS) that has fewer expressions.
/// This ensures balanced growth and matches the original's expression counts.
///
/// # Algorithm
///
/// 1. Start with minimal complexity limits (LHS: 1, RHS: 1)
/// 2. Generate expressions at current complexity
/// 3. Track how many expressions were generated for each side
/// 4. Expand the side with fewer expressions (increase complexity by 1)
/// 5. Repeat until target expression count is reached
/// 6. Then perform matching on all generated expressions
///
/// # Advantages
///
/// - Matches original RIES behavior exactly
/// - Generates similar number of expressions as original
/// - Better memory efficiency than generating all at once
/// - Can leverage parallelization more effectively
///
/// # Expression Count Formula
///
/// Target expressions = 2000 × 4^(2 + level)
/// - Level 0: ~32,000 expressions
/// - Level 1: ~128,000 expressions
/// - Level 2: ~512,000 expressions
/// - Level 3: ~2,048,000 expressions
pub fn search_adaptive(
    base_config: &crate::gen::GenConfig,
    search_config: &SearchConfig,
    level: u32,
) -> (Vec<Match>, SearchStats) {
    use crate::expr::EvaluatedExpr;
    use crate::gen::{quantize_value, LhsKey};
    use std::collections::HashMap;

    let gen_start = SearchTimer::start();
    let context = SearchContext::new(search_config);
    // Use HashMap so dedup keeps the simplest expression, not first-seen.
    // With parallel generation the arrival order is non-deterministic, so
    // first-seen could discard a simpler equivalent expression.
    let mut lhs_map: HashMap<LhsKey, EvaluatedExpr> = HashMap::new();
    let mut rhs_map: HashMap<i64, EvaluatedExpr> = HashMap::new();

    // For adaptive mode, use the standard complexity formula
    // See level_to_complexity() for the standard mapping
    let (std_lhs, std_rhs) = level_to_complexity(level);

    let mut config = base_config.clone();
    config.max_lhs_complexity = std_lhs.max(base_config.max_lhs_complexity);
    config.max_rhs_complexity = std_rhs.max(base_config.max_rhs_complexity);

    // Generate all expressions at the calculated complexity
    // Use parallel generation when available for significantly better performance
    let generated = {
        #[cfg(feature = "parallel")]
        {
            crate::gen::generate_all_parallel_with_context(
                &config,
                search_config.target,
                &context.eval,
            )
        }
        #[cfg(not(feature = "parallel"))]
        {
            crate::gen::generate_all_with_context(&config, search_config.target, &context.eval)
        }
    };

    // Deduplicate LHS expressions, keeping the simplest equivalent
    for expr in generated.lhs {
        let key = (quantize_value(expr.value), quantize_value(expr.derivative));
        lhs_map
            .entry(key)
            .and_modify(|existing| {
                if expr.expr.complexity() < existing.expr.complexity() {
                    *existing = expr.clone();
                }
            })
            .or_insert(expr);
    }

    // Deduplicate RHS expressions, keeping the simplest equivalent
    for expr in generated.rhs {
        let key = quantize_value(expr.value);
        rhs_map
            .entry(key)
            .and_modify(|existing| {
                if expr.expr.complexity() < existing.expr.complexity() {
                    *existing = expr.clone();
                }
            })
            .or_insert(expr);
    }

    let all_lhs: Vec<EvaluatedExpr> = lhs_map.into_values().collect();
    let all_rhs: Vec<EvaluatedExpr> = rhs_map.into_values().collect();

    let gen_time = gen_start.elapsed();

    // Now perform the actual matching
    let mut db = ExprDatabase::new();
    db.insert_rhs(all_rhs);

    let search_start = SearchTimer::start();
    let (matches, match_stats) = db.find_matches_with_stats_and_context(&all_lhs, &context);
    let search_time = search_start.elapsed();

    // Combine stats
    let mut stats = SearchStats::new();
    stats.gen_time = gen_time;
    stats.search_time = search_time;
    stats.lhs_count = all_lhs.len();
    stats.rhs_count = db.rhs_count();
    stats.lhs_tested = match_stats.lhs_tested;
    stats.candidates_tested = match_stats.candidates_tested;
    stats.newton_calls = match_stats.newton_calls;
    stats.newton_success = match_stats.newton_success;
    stats.pool_insertions = match_stats.pool_insertions;
    stats.pool_rejections_error = match_stats.pool_rejections_error;
    stats.pool_rejections_dedupe = match_stats.pool_rejections_dedupe;
    stats.pool_evictions = match_stats.pool_evictions;
    stats.pool_final_size = match_stats.pool_final_size;
    stats.pool_best_error = match_stats.pool_best_error;
    stats.early_exit = match_stats.early_exit;

    (matches, stats)
}

// =============================================================================
// STREAMING SEARCH - Memory-efficient search for high complexity levels
// =============================================================================

/// Perform a streaming search that processes expressions as they're generated
///
/// This is the memory-efficient version of search that builds the RHS database
/// incrementally. Instead of generating ALL expressions into memory first,
/// it processes RHS expressions immediately and matches LHS expressions as
/// they arrive.
///
/// # Memory Efficiency
///
/// - Traditional: O(expressions) memory - all expressions stored
/// - Streaming: O(database) memory - only RHS database stored, LHS processed on-the-fly
///
/// # When to Use
///
/// Use streaming search when:
/// - Complexity levels are high (L4+, where expressions count > 10M)
/// - Memory is limited
/// - You want to see results progressively
///
/// # Example
///
/// ```no_run
/// use ries_rs::gen::GenConfig;
/// use ries_rs::search::search_streaming;
/// let config = GenConfig::default();
/// let (matches, stats) = search_streaming(2.5, &config, 100, false, None);
/// ```
#[allow(dead_code)]
pub fn search_streaming(
    target: f64,
    gen_config: &crate::gen::GenConfig,
    max_matches: usize,
    stop_at_exact: bool,
    stop_below: Option<f64>,
) -> (Vec<Match>, SearchStats) {
    let config = SearchConfig {
        target,
        max_matches,
        stop_at_exact,
        stop_below,
        user_constants: gen_config.user_constants.clone(),
        user_functions: gen_config.user_functions.clone(),
        ..Default::default()
    };

    search_streaming_with_config(gen_config, &config)
}

/// Perform a streaming search with a fully specified search configuration.
pub fn search_streaming_with_config(
    gen_config: &crate::gen::GenConfig,
    search_config: &SearchConfig,
) -> (Vec<Match>, SearchStats) {
    use crate::gen::{generate_streaming_with_context, StreamingCallbacks};
    use std::collections::HashMap;

    let gen_start = SearchTimer::start();
    let mut stats = SearchStats::new();
    let context = SearchContext::new(search_config);

    // Respect configured max error (with a tiny floor for numerical stability)
    let initial_max_error = search_config.max_error.max(1e-12);

    // Create bounded pool with configured capacity
    let mut pool = TopKPool::new_with_diagnostics(
        search_config.max_matches,
        initial_max_error,
        search_config.show_db_adds,
        search_config.ranking_mode,
    );

    // Build RHS database first using streaming
    let mut rhs_db = TieredExprDatabase::new();
    let mut rhs_map: HashMap<i64, crate::expr::EvaluatedExpr> = HashMap::new();

    // Collect LHS expressions to process later
    let mut lhs_exprs: Vec<crate::expr::EvaluatedExpr> = Vec::new();

    // First pass: collect all RHS and LHS via streaming
    {
        let mut callbacks = StreamingCallbacks {
            on_rhs: &mut |expr| {
                // Deduplicate by value, keeping simplest.
                // Use quantize_value (not inline * 1e8) to avoid i64 overflow for large values.
                let key = crate::gen::quantize_value(expr.value);
                rhs_map
                    .entry(key)
                    .and_modify(|existing| {
                        if expr.expr.complexity() < existing.expr.complexity() {
                            *existing = expr.clone();
                        }
                    })
                    .or_insert_with(|| expr.clone());
                true
            },
            on_lhs: &mut |expr| {
                lhs_exprs.push(expr.clone());
                true
            },
        };

        // Generate with streaming callbacks
        generate_streaming_with_context(
            gen_config,
            search_config.target,
            &context.eval,
            &mut callbacks,
        );
    }

    // Build the tiered database from deduplicated RHS
    for expr in rhs_map.into_values() {
        rhs_db.insert(expr);
    }
    rhs_db.finalize();

    stats.rhs_count = rhs_db.total_count();
    stats.lhs_count = lhs_exprs.len();
    stats.gen_time = gen_start.elapsed();

    // Now match LHS expressions against the RHS database
    let search_start = SearchTimer::start();

    // Sort LHS by complexity so simpler expressions are processed first
    lhs_exprs.sort_by_key(|e| e.expr.complexity());

    // Early exit tracking
    let mut early_exit = false;

    'outer: for lhs in &lhs_exprs {
        if early_exit {
            break;
        }

        // Skip LHS with value too close to 0
        if lhs.value.abs() < search_config.zero_value_threshold {
            if search_config.show_pruned_range {
                eprintln!(
                    "  [pruned range] value={:.6e} reason=\"near-zero\" expr=\"{}\"",
                    lhs.value,
                    lhs.expr.to_infix()
                );
            }
            continue;
        }

        // Skip degenerate expressions
        if lhs.derivative.abs() < DEGENERATE_TEST_THRESHOLD {
            let test_x = search_config.target + std::f64::consts::E;
            // Use the full evaluator (including user_functions) so that UDF-containing
            // expressions are not silently skipped due to evaluate_with_constants
            // returning Err for user-function symbols.
            if let Ok(test_result) =
                crate::eval::evaluate_fast_with_context(&lhs.expr, test_x, &context.eval)
            {
                let value_unchanged =
                    (test_result.value - lhs.value).abs() < DEGENERATE_TEST_THRESHOLD;
                let deriv_still_zero = test_result.derivative.abs() < DEGENERATE_TEST_THRESHOLD;
                if deriv_still_zero || value_unchanged {
                    continue;
                }
            }

            // Check for value match
            stats.lhs_tested += 1;
            for rhs in rhs_db.iter_tiers_in_range(
                lhs.value - DEGENERATE_RANGE_TOLERANCE,
                lhs.value + DEGENERATE_RANGE_TOLERANCE,
            ) {
                if !search_config.rhs_symbol_allowed(&rhs.expr) {
                    continue;
                }
                stats.candidates_tested += 1;
                if search_config.show_match_checks {
                    eprintln!(
                        "  [match] checking lhs={:.6} rhs={:.6}",
                        lhs.value, rhs.value
                    );
                }
                let val_diff = (lhs.value - rhs.value).abs();
                if val_diff < DEGENERATE_RANGE_TOLERANCE && pool.would_accept(0.0, true) {
                    let m = Match {
                        lhs: lhs.clone(),
                        rhs: rhs.clone(),
                        x_value: search_config.target,
                        error: 0.0,
                        complexity: lhs.expr.complexity() + rhs.expr.complexity(),
                    };
                    pool.try_insert(m);
                }
            }
            continue;
        }

        stats.lhs_tested += 1;

        // Use adaptive search radius
        let search_radius = calculate_adaptive_search_radius(
            lhs.derivative,
            lhs.expr.complexity(),
            pool.len(),
            search_config.max_matches,
            pool.best_error,
        );
        let low = lhs.value - search_radius;
        let high = lhs.value + search_radius;

        // Search using tiered iterator (lower tiers first)
        for rhs in rhs_db.iter_tiers_in_range(low, high) {
            if !search_config.rhs_symbol_allowed(&rhs.expr) {
                continue;
            }
            stats.candidates_tested += 1;
            if search_config.show_match_checks {
                eprintln!(
                    "  [match] checking lhs={:.6} rhs={:.6}",
                    lhs.value, rhs.value
                );
            }

            // Compute initial error estimate (coarse filter)
            let val_diff = lhs.value - rhs.value;
            let x_delta = -val_diff / lhs.derivative;
            let coarse_error = x_delta.abs();

            // Skip if coarse estimate won't pass threshold
            let is_potentially_exact = coarse_error < NEWTON_FINAL_TOLERANCE;
            if !pool.would_accept_strict(coarse_error, is_potentially_exact) {
                continue;
            }

            if !search_config.refine_with_newton {
                let refined_x = search_config.target + x_delta;
                let refined_error = x_delta;
                let is_exact = refined_error.abs() < EXACT_MATCH_TOLERANCE;

                if pool.would_accept(refined_error.abs(), is_exact) {
                    let m = Match {
                        lhs: lhs.clone(),
                        rhs: rhs.clone(),
                        x_value: refined_x,
                        error: refined_error,
                        complexity: lhs.expr.complexity() + rhs.expr.complexity(),
                    };

                    pool.try_insert(m);

                    if search_config.stop_at_exact && is_exact {
                        early_exit = true;
                        break 'outer;
                    }
                    if let Some(threshold) = search_config.stop_below {
                        if refined_error.abs() < threshold {
                            early_exit = true;
                            break 'outer;
                        }
                    }
                }
                continue;
            }

            // Refine with Newton-Raphson
            stats.newton_calls += 1;
            if let Some(refined_x) = newton_raphson_with_constants(
                &lhs.expr,
                rhs.value,
                search_config.target,
                search_config.newton_iterations,
                &context.eval,
                search_config.show_newton,
                search_config.derivative_margin,
            ) {
                stats.newton_success += 1;
                let refined_error = refined_x - search_config.target;
                let is_exact = refined_error.abs() < EXACT_MATCH_TOLERANCE;

                // Check if this is acceptable
                if pool.would_accept(refined_error.abs(), is_exact) {
                    let m = Match {
                        lhs: lhs.clone(),
                        rhs: rhs.clone(),
                        x_value: refined_x,
                        error: refined_error,
                        complexity: lhs.expr.complexity() + rhs.expr.complexity(),
                    };

                    // Insert into pool
                    pool.try_insert(m);

                    // Check early exit conditions
                    if search_config.stop_at_exact && is_exact {
                        early_exit = true;
                        break 'outer;
                    }
                    if let Some(threshold) = search_config.stop_below {
                        if refined_error.abs() < threshold {
                            early_exit = true;
                            break 'outer;
                        }
                    }
                }
            }
        }
    }

    // Collect pool stats
    stats.pool_insertions = pool.stats.insertions;
    stats.pool_rejections_error = pool.stats.rejections_error;
    stats.pool_rejections_dedupe = pool.stats.rejections_dedupe;
    stats.pool_evictions = pool.stats.evictions;
    stats.pool_final_size = pool.len();
    stats.pool_best_error = pool.best_error;
    stats.search_time = search_start.elapsed();
    stats.early_exit = early_exit;

    (pool.into_sorted(), stats)
}

/// Perform one-sided search: generate RHS expressions only and match `x = RHS`.
pub fn search_one_sided_with_stats_and_config(
    gen_config: &crate::gen::GenConfig,
    config: &SearchConfig,
) -> (Vec<Match>, SearchStats) {
    use crate::eval::evaluate_with_context;
    use crate::expr::Expression;
    use crate::gen::generate_all_with_context;
    use crate::symbol::Symbol;

    let gen_start = SearchTimer::start();
    let context = SearchContext::new(config);

    let mut rhs_only = gen_config.clone();
    rhs_only.generate_lhs = false;
    rhs_only.generate_rhs = true;

    let generated = generate_all_with_context(&rhs_only, config.target, &context.eval);
    let gen_time = gen_start.elapsed();

    let search_start = SearchTimer::start();
    let initial_max_error = config.max_error.max(1e-12);
    let mut pool = TopKPool::new_with_diagnostics(
        config.max_matches,
        initial_max_error,
        config.show_db_adds,
        config.ranking_mode,
    );
    let mut stats = SearchStats::new();
    let mut early_exit = false;

    let mut lhs_expr = Expression::new();
    lhs_expr.push_with_table(Symbol::X, &gen_config.symbol_table);
    let lhs_eval = evaluate_with_context(&lhs_expr, config.target, &context.eval);
    let lhs_eval = match lhs_eval {
        Ok(v) => v,
        Err(_) => {
            stats.gen_time = gen_time;
            stats.search_time = search_start.elapsed();
            return (Vec::new(), stats);
        }
    };
    let lhs = EvaluatedExpr::new(
        lhs_expr,
        lhs_eval.value,
        lhs_eval.derivative,
        lhs_eval.num_type,
    );

    stats.lhs_count = 1;
    stats.rhs_count = generated.rhs.len();
    stats.lhs_tested = 1;

    for rhs in generated.rhs {
        if !config.rhs_symbol_allowed(&rhs.expr) {
            continue;
        }
        stats.candidates_tested += 1;
        if config.show_match_checks {
            eprintln!(
                "  [match] checking lhs={:.6} rhs={:.6}",
                lhs.value, rhs.value
            );
        }

        let error = rhs.value - config.target;
        let is_exact = error.abs() < EXACT_MATCH_TOLERANCE;
        if !pool.would_accept(error.abs(), is_exact) {
            continue;
        }

        let m = Match {
            lhs: lhs.clone(),
            rhs: rhs.clone(),
            x_value: rhs.value,
            error,
            complexity: lhs.expr.complexity() + rhs.expr.complexity(),
        };

        pool.try_insert(m);

        if config.stop_at_exact && is_exact {
            early_exit = true;
            break;
        }
        if let Some(threshold) = config.stop_below {
            if error.abs() < threshold {
                early_exit = true;
                break;
            }
        }
    }

    stats.pool_insertions = pool.stats.insertions;
    stats.pool_rejections_error = pool.stats.rejections_error;
    stats.pool_rejections_dedupe = pool.stats.rejections_dedupe;
    stats.pool_evictions = pool.stats.evictions;
    stats.pool_final_size = pool.len();
    stats.pool_best_error = pool.best_error;
    stats.gen_time = gen_time;
    stats.search_time = search_start.elapsed();
    stats.early_exit = early_exit;

    (pool.into_sorted(), stats)
}

/// Perform a parallel search using Rayon
///
/// This function is part of the public API for library consumers who want
/// parallel search without statistics collection.
#[cfg(feature = "parallel")]
#[allow(dead_code)]
pub fn search_parallel(
    target: f64,
    gen_config: &crate::gen::GenConfig,
    max_matches: usize,
) -> Vec<Match> {
    let (matches, _stats) = search_parallel_with_stats(target, gen_config, max_matches);
    matches
}

/// Perform a parallel search with statistics collection
///
/// This function is part of the public API for library consumers who want
/// detailed statistics about the parallel search process.
#[cfg(feature = "parallel")]
#[allow(dead_code)]
pub fn search_parallel_with_stats(
    target: f64,
    gen_config: &crate::gen::GenConfig,
    max_matches: usize,
) -> (Vec<Match>, SearchStats) {
    search_parallel_with_stats_and_options(target, gen_config, max_matches, false, None)
}

/// Perform a parallel search with statistics collection and early exit options
#[cfg(feature = "parallel")]
pub fn search_parallel_with_stats_and_options(
    target: f64,
    gen_config: &crate::gen::GenConfig,
    max_matches: usize,
    stop_at_exact: bool,
    stop_below: Option<f64>,
) -> (Vec<Match>, SearchStats) {
    let config = SearchConfig {
        target,
        max_matches,
        stop_at_exact,
        stop_below,
        user_constants: gen_config.user_constants.clone(),
        user_functions: gen_config.user_functions.clone(),
        ..Default::default()
    };

    search_parallel_with_stats_and_config(gen_config, &config)
}

/// Perform a parallel search with a fully specified search configuration.
///
/// This function includes a safety fallback: if expression generation would
/// exceed ~2M expressions (which would risk OOM), it automatically switches
/// to streaming mode to avoid memory exhaustion.
#[cfg(feature = "parallel")]
pub fn search_parallel_with_stats_and_config(
    gen_config: &crate::gen::GenConfig,
    config: &SearchConfig,
) -> (Vec<Match>, SearchStats) {
    use crate::gen::generate_all_with_limit_and_context;

    const MAX_EXPRESSIONS_BEFORE_STREAMING: usize = 2_000_000;
    let context = SearchContext::new(config);

    let gen_start = SearchTimer::start();

    // Try bounded generation first — if limit exceeded, fall back to streaming
    if let Some(generated) = generate_all_with_limit_and_context(
        gen_config,
        config.target,
        &context.eval,
        MAX_EXPRESSIONS_BEFORE_STREAMING,
    ) {
        let gen_time = gen_start.elapsed();

        // Build database
        let mut db = ExprDatabase::new();
        db.insert_rhs(generated.rhs);

        // Find matches with stats
        let (matches, mut stats) = db.find_matches_with_stats_and_context(&generated.lhs, &context);

        // Add generation stats
        stats.gen_time = gen_time;
        stats.lhs_count = generated.lhs.len();
        stats.rhs_count = db.rhs_count();

        (matches, stats)
    } else {
        // Limit exceeded — fall back to streaming mode which avoids OOM
        search_streaming_with_config(gen_config, config)
    }
}
