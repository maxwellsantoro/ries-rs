use super::newton::{newton_raphson_with_constants, NewtonHit};
use super::{Match, SearchConfig, SearchContext, SearchStats, SearchTimer};

use crate::expr::EvaluatedExpr;
use crate::gen::compare_expression_order;

use crate::pool::TopKPool;

use crate::thresholds::{
    ADAPTIVE_COMPLEXITY_SCALE, ADAPTIVE_EXACT_MATCH_FACTOR, ADAPTIVE_POOL_FULLNESS_SCALE,
    BASE_SEARCH_RADIUS_FACTOR, MAX_SEARCH_RADIUS_FACTOR, NEWTON_FINAL_TOLERANCE,
    STRICT_GATE_CAPACITY_FRACTION, STRICT_GATE_FACTOR, TIER_0_MAX, TIER_1_MAX, TIER_2_MAX,
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
            tier.sort_by(|a, b| {
                a.value
                    .total_cmp(&b.value)
                    .then_with(|| compare_expression_order(a, b))
            });
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

    /// Count expressions across all tiers within the value range [low, high].
    pub fn count_in_range(&self, low: f64, high: f64) -> usize {
        self.tiers
            .iter()
            .map(|tier| {
                let start = tier.partition_point(|e| e.value < low);
                let end = tier.partition_point(|e| e.value <= high);
                end.saturating_sub(start)
            })
            .sum()
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
#[allow(clippy::too_many_arguments)]
pub(super) fn calculate_adaptive_search_radius(
    derivative: f64,
    complexity: u32,
    pool_size: usize,
    pool_capacity: usize,
    best_error: f64,
    accept_error: f64,
    target: f64,
) -> f64 {
    let deriv_abs = derivative.abs();
    let scaled_derivative = deriv_abs * super::target_search_scale(target);

    // Convert a relative x window to equation-value units using the slope.
    let base_radius = BASE_SEARCH_RADIUS_FACTOR * scaled_derivative;

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

    // Cap the scan radius by the same coarse-error envelope that the pool gate
    // already enforces. Candidates outside this bound are guaranteed to be
    // rejected before Newton, so scanning them only inflates candidate windows.
    let gate_factor = if pool_capacity > 0
        && pool_size as f64 >= pool_capacity as f64 * STRICT_GATE_CAPACITY_FRACTION
    {
        STRICT_GATE_FACTOR
    } else {
        1.0
    };
    let coarse_error_cap = (accept_error * gate_factor).max(8.0 * super::ulp_of(target.abs()));
    let gated_radius_cap = deriv_abs * coarse_error_cap;

    // Ensure we have a reasonable minimum and cap at maximum
    let min_radius = 0.1 * scaled_derivative;
    radius
        .max(min_radius)
        .min(MAX_SEARCH_RADIUS_FACTOR * scaled_derivative)
        .min(gated_radius_cap)
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
        exprs.sort_by(|a, b| {
            a.value
                .total_cmp(&b.value)
                .then_with(|| compare_expression_order(a, b))
        });
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
        self.find_matches_in_order(lhs_exprs, context, false)
    }

    fn find_matches_in_order(
        &self,
        lhs_exprs: &[EvaluatedExpr],
        context: &SearchContext<'_>,
        parallel_refinement: bool,
    ) -> (Vec<Match>, SearchStats) {
        let mut stats = SearchStats::new();
        let search_start = SearchTimer::start();

        let mut sorted_lhs: Vec<_> = lhs_exprs.iter().collect();
        sorted_lhs.sort_unstable_by(|a, b| compare_expression_order(a, b));

        // Phase 1 finds the stop bound, if any. Before the bound is known the
        // pool can admit costlier matches that displace cheaper ones, so
        // phase 2 ranks the pairs within the bound with a fresh pool.
        let mut stop = super::StopBound::default();
        let (mut pool, mut early_exit) = self.match_sorted_lhs(
            &sorted_lhs,
            context,
            &mut stats,
            &mut stop,
            parallel_refinement,
        );
        if let Some(mut capped) = stop.capped() {
            (pool, _) = self.match_sorted_lhs(
                &sorted_lhs,
                context,
                &mut stats,
                &mut capped,
                parallel_refinement,
            );
            early_exit = true;
        }

        // Collect pool stats
        stats.record_pool(&pool);
        stats.search_time = search_start.elapsed();
        stats.early_exit = early_exit;

        // Return sorted matches from pool
        (stop.finish(pool.into_sorted(), context.config), stats)
    }

    /// Match complexity-sorted LHS expressions into a fresh pool. Returns the
    /// pool and whether the scan ended early.
    fn match_sorted_lhs(
        &self,
        sorted_lhs: &[&EvaluatedExpr],
        context: &SearchContext<'_>,
        stats: &mut SearchStats,
        stop: &mut super::StopBound,
        parallel_refinement: bool,
    ) -> (TopKPool, bool) {
        let mut pool = context.config.pool();
        let mut min_rhs_complexity = None;
        for &lhs in sorted_lhs {
            // LHS are visited in complexity order, so once the cheapest
            // possible pair exceeds the stop bound no later LHS can qualify.
            if stop.is_set() {
                let min_rhs = *min_rhs_complexity.get_or_insert_with(|| {
                    self.rhs_sorted
                        .iter()
                        .map(|rhs| rhs.expr.complexity())
                        .min()
                        .unwrap_or(0)
                });
                if stop.excludes(lhs.expr.complexity().saturating_add(min_rhs)) {
                    return (pool, true);
                }
            }
            if !match_one_lhs(
                self,
                lhs,
                context,
                &mut pool,
                stats,
                stop,
                parallel_refinement,
            ) {
                return (pool, true);
            }
        }
        (pool, false)
    }

    /// Refine wide candidate windows in parallel and admit matches in serial order.
    ///
    /// Both paths share one pool and traversal order, so they apply the same
    /// adaptive thresholds and rank the same results for a materialized search.
    #[cfg(feature = "parallel")]
    pub fn find_matches_turbo_with_stats_and_context(
        &self,
        lhs_exprs: &[EvaluatedExpr],
        context: &SearchContext<'_>,
    ) -> (Vec<Match>, SearchStats) {
        self.find_matches_in_order(lhs_exprs, context, true)
    }
}

fn refine_candidate(
    lhs: &EvaluatedExpr,
    rhs: &EvaluatedExpr,
    x_delta: f64,
    context: &SearchContext<'_>,
) -> Option<NewtonHit> {
    let config = context.config;
    newton_raphson_with_constants(
        &lhs.expr,
        rhs.value,
        config.target + x_delta,
        config.newton_iterations,
        &context.eval,
        config.show_newton,
        config.derivative_margin,
    )
}

/// Match a single LHS expression against the RHS database, inserting any
/// accepted matches into `pool` and updating `stats`.
///
/// Returns `false` when the complexity ceiling is reached. A stop match
/// tightens `stop` but does not end the window: a later candidate in the same
/// window can be simpler. This is the shared loop for serial and turbo matching.
fn match_one_lhs(
    db: &ExprDatabase,
    lhs: &EvaluatedExpr,
    context: &SearchContext<'_>,
    pool: &mut TopKPool,
    stats: &mut SearchStats,
    stop: &mut super::StopBound,
    parallel_refinement: bool,
) -> bool {
    let config = context.config;

    #[cfg(not(feature = "parallel"))]
    let _ = parallel_refinement;

    // A ceiling is available only for complexity ranking. Since LHS are sorted
    // by complexity, an LHS above the ceiling rules out all remaining pairs.
    // At the ceiling an RHS with zero weight can still tie on complexity.
    let exact_ceiling = pool.exact_complexity_ceiling();
    if let Some(ceiling) = exact_ceiling {
        if lhs.expr.complexity() > ceiling {
            return false;
        }
    }

    if !config.filters.allows_side(lhs) {
        return true;
    }

    // Retain complexity ties for the canonical postfix tie-break.
    let rhs_is_eligible = |rhs: &EvaluatedExpr| {
        config.rhs_symbol_allowed(&rhs.expr)
            && crate::eval::value_in_expression_range(&lhs.expr, rhs.value)
            && config.filters.allows_side(rhs)
            && config.filters.allows_pair(&lhs.expr, &rhs.expr)
            && exact_ceiling
                .is_none_or(|ceiling| lhs.expr.complexity() + rhs.expr.complexity() <= ceiling)
    };

    // Skip LHS with value too close to 0 - these produce floods of
    // trivial matches (like cospi(2.5)=0 matching many RHS near 0).
    // Original RIES: "Prune zero subexpressions".
    // Target 0 is exempt: `x` itself is under the threshold there.
    if super::prune_near_zero_lhs(lhs.value, config.target, config.zero_value_threshold) {
        if config.show_pruned_range {
            eprintln!(
                "  [pruned range] value={:.6e} reason=\"near-zero\" expr=\"{}\"",
                lhs.value,
                lhs.expr.to_infix()
            );
        }
        return true;
    }

    if lhs.derivative == 0.0 || super::should_skip_degenerate_lhs(lhs, config.target, &context.eval)
    {
        return true;
    }
    stats.lhs_tested += 1;

    // Search for RHS expressions near this LHS value using the adaptive radius.
    let search_radius = calculate_adaptive_search_radius(
        lhs.derivative,
        lhs.expr.complexity(),
        pool.len(),
        config.max_matches,
        pool.best_error,
        pool.accept_error,
        config.target,
    );
    let low = lhs.value - search_radius;
    let high = lhs.value + search_radius;

    let rhs_slice = db.range(low, high);
    stats.record_candidate_window(lhs, rhs_slice.len());
    // Speculate only on wide windows. Newton depends on the expressions and
    // the starting guess, while the acceptance gates depend on earlier hits.
    #[cfg(feature = "parallel")]
    let refined = if parallel_refinement
        && config.refine_with_newton
        && !config.show_newton
        && !config.stop_at_exact
        && config.stop_below.is_none()
        && rhs_slice.len() >= 64
        && rayon::current_num_threads() > 1
    {
        use rayon::prelude::*;
        let hits: Vec<_> = rhs_slice
            .par_iter()
            .map(|rhs| {
                let x_delta = -(lhs.value - rhs.value) / lhs.derivative;
                let coarse_error = x_delta.abs();
                let passes_gate =
                    pool.would_accept_strict(coarse_error, coarse_error < NEWTON_FINAL_TOLERANCE);
                (passes_gate && rhs_is_eligible(rhs))
                    .then(|| refine_candidate(lhs, rhs, x_delta, context))
            })
            .collect();
        stats.newton_calls += hits.iter().flatten().count();
        Some(hits)
    } else {
        None
    };
    #[cfg(not(feature = "parallel"))]
    let refined: Option<Vec<Option<Option<NewtonHit>>>> = None;

    for (index, rhs) in rhs_slice.iter().enumerate() {
        if !rhs_is_eligible(rhs) || stop.excludes(lhs.expr.complexity() + rhs.expr.complexity()) {
            continue;
        }

        stats.candidates_tested += 1;
        if config.show_match_checks {
            eprintln!(
                "  [match] checking lhs={:.6} rhs={:.6}",
                lhs.value, rhs.value
            );
        }

        // Compute initial error estimate (coarse filter).
        let x_delta = -(lhs.value - rhs.value) / lhs.derivative;
        let coarse_error = x_delta.abs();

        // Skip if coarse estimate won't pass threshold. The strict gate avoids
        // expensive Newton calls for marginal candidates.
        let is_potentially_exact = coarse_error < NEWTON_FINAL_TOLERANCE;
        if !pool.would_accept_strict(coarse_error, is_potentially_exact) {
            stats.strict_gate_rejections += 1;
            continue;
        }

        let (x_value, error) = if config.refine_with_newton {
            let hit = match refined.as_ref().and_then(|hits| hits[index]) {
                Some(hit) => hit,
                None => {
                    stats.newton_calls += 1;
                    refine_candidate(lhs, rhs, x_delta, context)
                }
            };
            let Some(hit) = hit.filter(|hit| hit.real) else {
                continue;
            };
            stats.newton_success += 1;
            (hit.x, hit.x - config.target)
        } else {
            (config.target + x_delta, x_delta)
        };

        let Some(exact) =
            super::match_exactness(lhs.value, rhs.value, lhs.derivative, x_value, config.target)
        else {
            continue;
        };
        if pool.would_accept(error.abs(), exact) && config.filters.allows_error(error) {
            let m = Match {
                lhs: lhs.clone(),
                rhs: rhs.clone(),
                x_value,
                error,
                complexity: lhs.expr.complexity() + rhs.expr.complexity(),
                exact,
            };
            stop.observe(&m, config);
            pool.try_insert(m);
        }
    }

    true
}

impl Default for ExprDatabase {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_adaptive_radius_is_capped_by_accept_error() {
        let derivative = 10.0;
        let radius = calculate_adaptive_search_radius(derivative, 10, 0, 16, 1.0, 1e-3, 1.0);

        assert!(
            (radius - 0.01).abs() < 1e-12,
            "radius should be capped by derivative * accept_error"
        );
    }

    #[test]
    fn test_adaptive_radius_uses_strict_gate_cap_when_pool_is_full() {
        let derivative = 100.0;
        let accept_error = 0.02;
        let radius =
            calculate_adaptive_search_radius(derivative, 10, 16, 16, 1.0, accept_error, 1.0);

        let expected_cap = derivative * accept_error * STRICT_GATE_FACTOR;
        assert!(
            (radius - expected_cap).abs() < 1e-12,
            "strict-gate fullness should tighten the scan radius cap"
        );
    }

    #[test]
    fn tiny_target_radius_scales_with_target_without_an_absolute_floor() {
        let ordinary = calculate_adaptive_search_radius(2.0, 10, 0, 5000, 1.0, 1.0, 1.0);
        for target in [1e-15_f64, -1e-15, 1e-300, -1e-300] {
            let radius =
                calculate_adaptive_search_radius(2.0, 10, 0, 5000, 1.0, target.abs(), target);
            assert!((radius / target.abs() - ordinary).abs() < 1e-14);
            let tight = calculate_adaptive_search_radius(
                2.0,
                10,
                0,
                5000,
                1.0,
                target.abs() * 1e-6,
                target,
            );
            assert!(tight <= 2.0 * target.abs() * 1e-6);
        }
        assert_eq!(
            calculate_adaptive_search_radius(2.0, 10, 0, 5000, 1.0, 1.0, 0.0),
            ordinary
        );
    }

    #[test]
    #[allow(clippy::approx_constant)] // Exercise the rounded, typed target.
    fn pi_no_root_approximation_is_rejected_after_newton() {
        use crate::expr::Expression;
        let target = 3.14159265;
        let eval = |postfix| {
            let expr = Expression::parse(postfix).unwrap();
            let at = crate::eval::evaluate(&expr, target).unwrap();
            EvaluatedExpr::new(expr, at.value, at.derivative, at.num_type)
        };
        let lhs = eval("px-Cr");
        let rhs = eval("14/Ts");
        // sec(pi*(pi-x)) >= 1 near pi, while tanpi(1/4)^2 is below 1.
        assert!(rhs.value < 1.0);
        let config = SearchConfig {
            target,
            max_matches: 16,
            max_error: 0.1,
            ..Default::default()
        };
        let context = config.context();
        let delta = -(lhs.value - rhs.value) / lhs.derivative;
        let hit =
            refine_candidate(&lhs, &rhs, delta, &context).expect("Newton returns a plateau hit");
        assert!(hit.real, "loose residual test accepts the rounded plateau");
        assert!(!super::super::solved_x_within_ulps(hit.x, target));
        assert_eq!(
            super::super::match_exactness(lhs.value, rhs.value, lhs.derivative, hit.x, target),
            None
        );
        let mut db = ExprDatabase::new();
        db.insert_rhs(vec![rhs]);
        let mut pool = config.pool();
        let mut stats = SearchStats::default();
        assert!(match_one_lhs(
            &db,
            &lhs,
            &context,
            &mut pool,
            &mut stats,
            &mut crate::search::StopBound::default(),
            false
        ));
        assert_eq!(stats.newton_success, 1, "exercise post-Newton admission");
        assert!(pool.into_sorted().is_empty());
    }

    #[cfg(feature = "parallel")]
    #[test]
    fn turbo_preserves_pool_admission_with_a_wide_candidate_window() {
        use crate::eval::evaluate;
        use crate::expr::Expression;

        let config = SearchConfig {
            target: 1.035,
            max_error: 0.1,
            max_matches: 1,
            ..Default::default()
        };
        let evaluated = |postfix: &str| {
            let expr = Expression::parse(postfix).unwrap();
            let result = evaluate(&expr, config.target).unwrap();
            EvaluatedExpr::new(expr, result.value, result.derivative, result.num_type)
        };
        let mut db = ExprDatabase::new();
        // Generate constants 1 + n/1000 with two nonzero digits in n.
        db.insert_rhs(
            (1..=9)
                .flat_map(|tens| (1..=9).map(move |ones| (tens, ones)))
                .map(|(tens, ones)| evaluated(&format!("119+{tens}*{ones}+19+3^/+")))
                .collect(),
        );
        let lhs = [evaluated("x")];
        let (serial, serial_stats) = db.find_matches_with_stats(&lhs, &config);
        let (turbo, turbo_stats) = rayon::ThreadPoolBuilder::new()
            .num_threads(2)
            .build()
            .unwrap()
            .install(|| db.find_matches_turbo_with_stats_and_context(&lhs, &config.context()));
        assert_eq!(serial.len(), 1);
        assert_eq!(turbo.len(), 1);
        assert!(turbo_stats.newton_calls > serial_stats.newton_calls);
        assert_eq!(serial[0].rhs.expr, turbo[0].rhs.expr);
        assert_eq!(serial[0].x_value, turbo[0].x_value);
        assert!(turbo[0].exact);
    }
}
