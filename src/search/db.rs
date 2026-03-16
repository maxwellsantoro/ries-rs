use super::newton::newton_raphson_with_constants;
use super::{Match, SearchConfig, SearchContext, SearchStats, SearchTimer};

use crate::expr::EvaluatedExpr;

use crate::pool::TopKPool;

use crate::thresholds::{
    ADAPTIVE_COMPLEXITY_SCALE, ADAPTIVE_EXACT_MATCH_FACTOR, ADAPTIVE_POOL_FULLNESS_SCALE,
    BASE_SEARCH_RADIUS_FACTOR, DEGENERATE_RANGE_TOLERANCE, DEGENERATE_TEST_THRESHOLD,
    EXACT_MATCH_TOLERANCE, MAX_SEARCH_RADIUS_FACTOR, NEWTON_FINAL_TOLERANCE, TIER_0_MAX,
    TIER_1_MAX, TIER_2_MAX,
};

/// Database for storing expressions sorted by value
/// Uses a flat sorted vector for cache-friendly range scans
pub struct ExprDatabase {
    /// RHS expressions sorted by value (flat vector for cache locality)
    rhs_sorted: Vec<EvaluatedExpr>,
}

// =============================================================================
// TIERED DATABASE FOR MULTI-LEVEL INDEXING
// =============================================================================

/// Complexity tier for tiered search
///
/// Lower tiers contain simpler expressions and are searched first.
/// This allows early exit when good matches are found in simpler tiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ComplexityTier {
    /// Tier 0: complexity 0-15 (simplest expressions)
    Tier0,
    /// Tier 1: complexity 16-25
    Tier1,
    /// Tier 2: complexity 26-35
    Tier2,
    /// Tier 3: complexity 36+ (most complex)
    Tier3,
}

impl ComplexityTier {
    /// Determine the tier for a given complexity value
    #[inline]
    pub fn from_complexity(complexity: u32) -> Self {
        if complexity <= TIER_0_MAX {
            ComplexityTier::Tier0
        } else if complexity <= TIER_1_MAX {
            ComplexityTier::Tier1
        } else if complexity <= TIER_2_MAX {
            ComplexityTier::Tier2
        } else {
            ComplexityTier::Tier3
        }
    }
}

/// Database with tiered storage for efficient priority-based searching
///
/// Expressions are organized by complexity tiers, allowing searches to
/// process simpler expressions first and potentially skip higher tiers
/// when good matches are found.
pub struct TieredExprDatabase {
    /// RHS expressions organized by tier, each sorted by value
    tiers: [Vec<EvaluatedExpr>; 4],
    /// Total count across all tiers
    total_count: usize,
}

impl TieredExprDatabase {
    /// Create a new empty tiered database
    pub fn new() -> Self {
        Self {
            tiers: [Vec::new(), Vec::new(), Vec::new(), Vec::new()],
            total_count: 0,
        }
    }

    /// Insert an expression into the appropriate tier
    pub fn insert(&mut self, expr: EvaluatedExpr) {
        let tier = ComplexityTier::from_complexity(expr.expr.complexity());
        let tier_idx = tier as usize;
        self.tiers[tier_idx].push(expr);
        self.total_count += 1;
    }

    /// Finalize the database by sorting each tier by value
    pub fn finalize(&mut self) {
        for tier in &mut self.tiers {
            tier.sort_by(|a, b| a.value.total_cmp(&b.value));
        }
    }

    /// Get total count of expressions across all tiers
    pub fn total_count(&self) -> usize {
        self.total_count
    }

    /// Get count for a specific tier
    #[allow(dead_code)]
    pub fn tier_count(&self, tier: ComplexityTier) -> usize {
        self.tiers[tier as usize].len()
    }

    #[cfg(test)]
    pub(super) fn tier(&self, tier: ComplexityTier) -> &[EvaluatedExpr] {
        &self.tiers[tier as usize]
    }

    /// Find expressions in a specific tier within the value range [low, high]
    #[allow(dead_code)]
    pub fn range_in_tier(&self, tier: ComplexityTier, low: f64, high: f64) -> &[EvaluatedExpr] {
        let tier_vec = &self.tiers[tier as usize];
        let start = tier_vec.partition_point(|e| e.value < low);
        let end = tier_vec.partition_point(|e| e.value <= high);
        &tier_vec[start..end]
    }

    /// Create an iterator that yields expressions from all tiers in order
    /// (Tier 0 first, then Tier 1, etc.) within a value range
    pub fn iter_tiers_in_range(&self, low: f64, high: f64) -> TieredRangeIter<'_> {
        TieredRangeIter::new(self, low, high)
    }
}

impl Default for TieredExprDatabase {
    fn default() -> Self {
        Self::new()
    }
}

/// Iterator over expressions in a value range, yielding from lower tiers first
pub struct TieredRangeIter<'a> {
    db: &'a TieredExprDatabase,
    low: f64,
    high: f64,
    current_tier: usize,
    current_start: usize,
    current_end: usize,
}

impl<'a> TieredRangeIter<'a> {
    fn new(db: &'a TieredExprDatabase, low: f64, high: f64) -> Self {
        let mut iter = Self {
            db,
            low,
            high,
            current_tier: 0,
            current_start: 0,
            current_end: 0,
        };
        iter.find_next_nonempty_tier();
        iter
    }

    /// Calculate the range [start, end) of expressions in a tier that fall within [low, high]
    fn calculate_tier_range(&self, tier_idx: usize) -> (usize, usize) {
        let tier_vec = &self.db.tiers[tier_idx];
        let start = tier_vec.partition_point(|e| e.value < self.low);
        let end = tier_vec.partition_point(|e| e.value <= self.high);
        (start, end)
    }

    /// Advance to the next tier with matching expressions
    fn find_next_nonempty_tier(&mut self) {
        while self.current_tier < 4 {
            let (start, end) = self.calculate_tier_range(self.current_tier);
            self.current_start = start;
            self.current_end = end;

            if self.current_start < self.current_end {
                // Found expressions in this tier
                return;
            }
            self.current_tier += 1;
        }
    }
}

impl<'a> Iterator for TieredRangeIter<'a> {
    type Item = &'a EvaluatedExpr;

    fn next(&mut self) -> Option<Self::Item> {
        while self.current_tier < 4 {
            if self.current_start < self.current_end {
                let expr = &self.db.tiers[self.current_tier][self.current_start];
                self.current_start += 1;
                return Some(expr);
            }
            self.current_tier += 1;
            self.find_next_nonempty_tier();
        }
        None
    }
}

// =============================================================================
// ADAPTIVE SEARCH RADIUS
// =============================================================================

/// Calculate adaptive search radius based on multiple factors
///
/// The search radius determines how far from an LHS value we look for
/// matching RHS expressions. A tighter radius means fewer candidates
/// but faster search; a wider radius means more candidates but slower.
///
/// # Factors
///
/// 1. **Derivative magnitude**: Larger derivative = tighter radius (faster convergence)
/// 2. **Complexity**: Higher complexity = tighter radius (prefer simpler matches)
/// 3. **Pool fullness**: Fuller pool = tighter radius (be more selective)
/// 4. **Best error found**: If we have exact matches, be very selective
///
/// # Returns
///
/// The search radius as an absolute value (not relative to derivative).
#[inline]
pub(super) fn calculate_adaptive_search_radius(
    derivative: f64,
    complexity: u32,
    pool_size: usize,
    pool_capacity: usize,
    best_error: f64,
) -> f64 {
    let deriv_abs = derivative.abs();

    // Base radius: proportional to derivative
    let base_radius = BASE_SEARCH_RADIUS_FACTOR * deriv_abs;

    // Complexity factor: reduce radius for complex expressions
    // normalized_complexity is roughly 0-1 for typical complexity ranges
    let normalized_complexity = (complexity as f64) / 50.0;
    let complexity_factor = 1.0 / (1.0 + ADAPTIVE_COMPLEXITY_SCALE * normalized_complexity);

    // Pool fullness factor: reduce radius as pool fills up
    let pool_fraction = if pool_capacity > 0 {
        pool_size as f64 / pool_capacity as f64
    } else {
        0.0
    };
    let pool_factor = (1.0 - ADAPTIVE_POOL_FULLNESS_SCALE * pool_fraction).max(0.1);

    // Exact match factor: if we have good matches, be very selective
    let exact_factor = if best_error < NEWTON_FINAL_TOLERANCE {
        ADAPTIVE_EXACT_MATCH_FACTOR
    } else {
        1.0
    };

    // Combined radius
    let radius = base_radius * complexity_factor * pool_factor * exact_factor;

    // Ensure we have a reasonable minimum and cap at maximum
    let min_radius = 0.1 * deriv_abs; // At least 0.1 * derivative
    radius
        .max(min_radius)
        .min(MAX_SEARCH_RADIUS_FACTOR * deriv_abs)
}

impl ExprDatabase {
    pub fn new() -> Self {
        Self {
            rhs_sorted: Vec::new(),
        }
    }

    /// Insert RHS expressions into the database
    /// Sorts by value for efficient range queries using partition_point
    pub fn insert_rhs(&mut self, mut exprs: Vec<EvaluatedExpr>) {
        // Sort by value for binary search range queries
        // Use total_cmp for consistent ordering (NaN sorts as greater than all floats)
        exprs.sort_by(|a, b| a.value.total_cmp(&b.value));
        self.rhs_sorted = exprs;
    }

    /// Get total count of RHS expressions
    pub fn rhs_count(&self) -> usize {
        self.rhs_sorted.len()
    }

    /// Find RHS expressions in the value range [low, high]
    /// Returns a slice of matching expressions (contiguous, cache-friendly)
    #[inline]
    pub fn range(&self, low: f64, high: f64) -> &[EvaluatedExpr] {
        // Binary search for range bounds using partition_point
        let start = self.rhs_sorted.partition_point(|e| e.value < low);
        let end = self.rhs_sorted.partition_point(|e| e.value <= high);
        &self.rhs_sorted[start..end]
    }

    /// Find matches for LHS expressions using streaming collection
    ///
    /// This method is part of the public API for library consumers who want
    /// to perform matching without statistics collection.
    #[allow(dead_code)]
    pub fn find_matches(&self, lhs_exprs: &[EvaluatedExpr], config: &SearchConfig) -> Vec<Match> {
        let (matches, _stats) = self.find_matches_with_stats(lhs_exprs, config);
        matches
    }

    /// Find matches with an explicit per-run search context.
    pub fn find_matches_with_context(
        &self,
        lhs_exprs: &[EvaluatedExpr],
        context: &SearchContext<'_>,
    ) -> Vec<Match> {
        let (matches, _stats) = self.find_matches_with_stats_and_context(lhs_exprs, context);
        matches
    }

    /// Find matches with statistics collection
    pub fn find_matches_with_stats(
        &self,
        lhs_exprs: &[EvaluatedExpr],
        config: &SearchConfig,
    ) -> (Vec<Match>, SearchStats) {
        let context = SearchContext::new(config);
        self.find_matches_with_stats_and_context(lhs_exprs, &context)
    }

    /// Find matches with statistics collection using an explicit per-run search context.
    pub fn find_matches_with_stats_and_context(
        &self,
        lhs_exprs: &[EvaluatedExpr],
        context: &SearchContext<'_>,
    ) -> (Vec<Match>, SearchStats) {
        let config = context.config;
        let mut stats = SearchStats::new();
        let search_start = SearchTimer::start();

        // Respect configured max error (with a tiny floor for numerical stability)
        let initial_max_error = config.max_error.max(1e-12);

        // Create bounded pool with configured capacity
        let mut pool = TopKPool::new_with_diagnostics(
            config.max_matches,
            initial_max_error,
            config.show_db_adds,
            config.ranking_mode,
        );

        // Sort LHS by complexity so simpler expressions are processed first
        let mut sorted_lhs: Vec<_> = lhs_exprs.iter().collect();
        sorted_lhs.sort_by_key(|e| e.expr.complexity());

        // Early exit tracking
        let mut early_exit = false;

        'outer: for lhs in sorted_lhs {
            // Check early exit conditions
            if early_exit {
                break;
            }
            // Skip LHS with value too close to 0 - these produce floods of
            // trivial matches (like cospi(2.5)=0 matching many RHS near 0)
            // Original RIES: "Prune zero subexpressions"
            if lhs.value.abs() < config.zero_value_threshold {
                if config.show_pruned_range {
                    eprintln!(
                        "  [pruned range] value={:.6e} reason=\"near-zero\" expr=\"{}\"",
                        lhs.value,
                        lhs.expr.to_infix()
                    );
                }
                continue;
            }

            // Skip degenerate expressions: contain x but derivative is 0
            // These are trivial identities like 1^x=1, x/x=1, log_x(x)=1
            if lhs.derivative.abs() < DEGENERATE_TEST_THRESHOLD {
                // To distinguish true repeated roots from degenerate expressions,
                // evaluate at a different x value. Degenerate expressions have
                // derivative 0 everywhere; true repeated roots only at specific x.
                // Use an irrational offset to avoid hitting special values
                let test_x = config.target + std::f64::consts::E;
                // Use the full evaluator (including user_functions) so that UDF-containing
                // expressions are not silently skipped due to evaluate_with_constants
                // returning Err for user-function symbols.
                if let Ok(test_result) =
                    crate::eval::evaluate_fast_with_context(&lhs.expr, test_x, &context.eval)
                {
                    // Check both: derivative still ~0, AND value unchanged
                    // This catches x*(1/x)=1 type expressions
                    let value_unchanged =
                        (test_result.value - lhs.value).abs() < DEGENERATE_TEST_THRESHOLD;
                    let deriv_still_zero = test_result.derivative.abs() < DEGENERATE_TEST_THRESHOLD;
                    if deriv_still_zero || value_unchanged {
                        // Degenerate expression - skip
                        continue;
                    }
                }
                // Derivative is non-zero at test_x, so this might be a true repeated root
                // Check if LHS(target) ≈ some RHS
                let val_error = DEGENERATE_RANGE_TOLERANCE;
                let low = lhs.value - val_error;
                let high = lhs.value + val_error;

                stats.lhs_tested += 1;
                for rhs in self.range(low, high) {
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
                    let val_diff = (lhs.value - rhs.value).abs();
                    if val_diff < val_error && pool.would_accept(0.0, true) {
                        let m = Match {
                            lhs: lhs.clone(),
                            rhs: rhs.clone(),
                            x_value: config.target,
                            error: 0.0,
                            complexity: lhs.expr.complexity() + rhs.expr.complexity(),
                        };
                        pool.try_insert(m);
                    }
                }
                continue;
            }

            stats.lhs_tested += 1;

            // Search for RHS expressions near this LHS value
            // Use adaptive search radius based on current thresholds
            let min_search_radius = 0.5 * lhs.derivative.abs(); // Allow ~0.5 error in x
            let search_radius = (pool.accept_error * lhs.derivative.abs()).max(min_search_radius);
            let low = lhs.value - search_radius;
            let high = lhs.value + search_radius;

            let rhs_slice = self.range(low, high);
            // Track slice sizes for optimization analysis
            // println!("LHS {} (val={:.4}): slice size = {}", lhs.expr.to_postfix(), lhs.value, rhs_slice.len());
            for rhs in rhs_slice {
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

                // Compute initial error estimate (coarse filter)
                let val_diff = lhs.value - rhs.value;
                let x_delta = -val_diff / lhs.derivative;
                let coarse_error = x_delta.abs();

                // Skip if coarse estimate won't pass threshold
                // Use strict gate to avoid expensive Newton calls for marginal candidates
                let is_potentially_exact = coarse_error < NEWTON_FINAL_TOLERANCE;
                if !pool.would_accept_strict(coarse_error, is_potentially_exact) {
                    continue;
                }

                if !config.refine_with_newton {
                    let refined_x = config.target + x_delta;
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

                        if config.stop_at_exact && is_exact {
                            early_exit = true;
                            break 'outer;
                        }
                        if let Some(threshold) = config.stop_below {
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
                    config.target,
                    config.newton_iterations,
                    &context.eval,
                    config.show_newton,
                    config.derivative_margin,
                ) {
                    stats.newton_success += 1;
                    let refined_error = refined_x - config.target;
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

                        // Insert into pool (handles thresholds and eviction)
                        pool.try_insert(m);

                        // Check early exit conditions
                        if config.stop_at_exact && is_exact {
                            early_exit = true;
                            break 'outer;
                        }
                        if let Some(threshold) = config.stop_below {
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

        // Return sorted matches from pool
        (pool.into_sorted(), stats)
    }
}

impl Default for ExprDatabase {
    fn default() -> Self {
        Self::new()
    }
}
