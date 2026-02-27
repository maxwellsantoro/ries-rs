//! Bounded priority pool for streaming match collection
//!
//! Implements a bounded pool that keeps the best matches across
//! multiple dimensions (error, complexity) with deduplication.
//!
//! # Key Design
//!
//! Keys use `Expression` directly rather than `String` for zero-allocation
//! deduplication. Since `Expression` uses `SmallVec<[Symbol; 21]>`, expressions
//! within the length limit stay inline on the stack, avoiding heap allocation
//! during hashing and comparison.

use crate::expr::Expression;
use crate::search::Match;
use crate::thresholds::{
    ACCEPT_ERROR_TIGHTEN_FACTOR, BEST_ERROR_TIGHTEN_FACTOR, EXACT_MATCH_TOLERANCE,
    NEWTON_TOLERANCE, STRICT_GATE_CAPACITY_FRACTION, STRICT_GATE_FACTOR,
};
use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashSet};

/// Match ranking mode for pool eviction and final ordering.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum RankingMode {
    /// Sort by exactness -> error -> complexity (current default behavior)
    #[default]
    Complexity,
    /// Sort by exactness -> error -> legacy signed parity score -> complexity
    Parity,
}

/// Keys for full equation deduplication (LHS + RHS pair)
///
/// Uses a pair of expressions directly for zero-allocation hashing.
/// The tuple (lhs, rhs) uniquely identifies an equation.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct EqnKey {
    /// LHS expression (contains x)
    lhs: Expression,
    /// RHS expression (constants only)
    rhs: Expression,
}

impl EqnKey {
    /// Create a key from a match
    #[inline]
    pub fn from_match(m: &Match) -> Self {
        Self {
            lhs: m.lhs.expr.clone(),
            rhs: m.rhs.expr.clone(),
        }
    }
}

/// Keys for LHS-only deduplication
///
/// Used to prevent adding too many variants of the same LHS expression.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct LhsKey {
    /// LHS expression
    lhs: Expression,
}

impl LhsKey {
    /// Create a key from a match
    #[inline]
    pub fn from_match(m: &Match) -> Self {
        Self {
            lhs: m.lhs.expr.clone(),
        }
    }
}

/// Signature for operator/constant pattern (used for "interesting" dedupe)
///
/// Uses a boxed slice for efficient storage and hashing.
/// Signatures are created during pool insertion (not the hot path).
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct SignatureKey {
    /// Operator pattern signature as bytes
    key: Box<[u8]>,
}

impl SignatureKey {
    pub fn from_match(m: &Match) -> Self {
        // Build a signature from operator types and constants used
        let expected_len = m.lhs.expr.len() + m.rhs.expr.len() + 1;
        let mut ops = Vec::with_capacity(expected_len);

        for sym in m.lhs.expr.symbols() {
            ops.push(*sym as u8);
        }
        ops.push(b'=');
        for sym in m.rhs.expr.symbols() {
            ops.push(*sym as u8);
        }

        Self {
            key: ops.into_boxed_slice(),
        }
    }
}

/// Compute legacy (original RIES style) signed parity score for an expression.
pub fn legacy_parity_score_expr(expr: &Expression) -> i32 {
    expr.symbols().iter().fold(0_i32, |acc, sym| {
        acc.saturating_add(sym.legacy_parity_weight())
    })
}

/// Compute legacy (original RIES style) signed parity score for a match.
pub fn legacy_parity_score_match(m: &Match) -> i32 {
    legacy_parity_score_expr(&m.lhs.expr).saturating_add(legacy_parity_score_expr(&m.rhs.expr))
}

#[inline]
fn compare_expr(a: &Expression, b: &Expression) -> Ordering {
    a.symbols()
        .iter()
        .map(|s| *s as u8)
        .cmp(b.symbols().iter().map(|s| *s as u8))
}

/// Compare two matches according to the selected ranking mode.
pub fn compare_matches(a: &Match, b: &Match, ranking_mode: RankingMode) -> Ordering {
    let a_exactness = if a.error.abs() < EXACT_MATCH_TOLERANCE {
        0_u8
    } else {
        1_u8
    };
    let b_exactness = if b.error.abs() < EXACT_MATCH_TOLERANCE {
        0_u8
    } else {
        1_u8
    };

    let mut ord = a_exactness.cmp(&b_exactness).then_with(|| {
        a.error
            .abs()
            .partial_cmp(&b.error.abs())
            .unwrap_or(Ordering::Equal)
    });

    if ord != Ordering::Equal {
        return ord;
    }

    ord = match ranking_mode {
        RankingMode::Complexity => a.complexity.cmp(&b.complexity),
        RankingMode::Parity => legacy_parity_score_match(a)
            .cmp(&legacy_parity_score_match(b))
            .then_with(|| a.complexity.cmp(&b.complexity)),
    };

    if ord != Ordering::Equal {
        return ord;
    }

    compare_expr(&a.lhs.expr, &b.lhs.expr).then_with(|| compare_expr(&a.rhs.expr, &b.rhs.expr))
}

/// Wrapper for Match that implements ordering for the heap
/// We keep the worst-ranked entry at the heap top for eviction.
#[derive(Clone)]
struct PoolEntry {
    m: Match,
    rank_key: (u8, i64, i32, u32), // (exactness, error_bits, mode_tie, complexity)
}

impl PoolEntry {
    fn new(m: Match, ranking_mode: RankingMode) -> Self {
        let is_exact = m.error.abs() < EXACT_MATCH_TOLERANCE;
        let exactness_rank = if is_exact { 0 } else { 1 };
        // Convert error to sortable integer, handling special values.
        // For IEEE 754 doubles, positive values' bit patterns preserve ordering
        // when interpreted as unsigned, but we need to handle NaN/Infinity specially.
        let error_abs = m.error.abs();
        let error_bits = if error_abs.is_nan() {
            // NaN should sort as worst (largest) error
            i64::MAX
        } else if error_abs.is_infinite() {
            // Infinity should also sort as worst (just below NaN)
            i64::MAX - 1
        } else {
            // For normal positive floats, the bit pattern preserves ordering
            // when cast to i64 (since all positive floats have bit patterns < i64::MAX)
            error_abs.to_bits() as i64
        };
        let mode_tie = match ranking_mode {
            RankingMode::Complexity => m.complexity as i32,
            RankingMode::Parity => legacy_parity_score_match(&m),
        };
        Self {
            rank_key: (exactness_rank, error_bits, mode_tie, m.complexity),
            m,
        }
    }
}

impl PartialEq for PoolEntry {
    fn eq(&self, other: &Self) -> bool {
        self.rank_key == other.rank_key
            && self.m.lhs.expr == other.m.lhs.expr
            && self.m.rhs.expr == other.m.rhs.expr
    }
}

impl Eq for PoolEntry {}

impl PartialOrd for PoolEntry {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for PoolEntry {
    fn cmp(&self, other: &Self) -> Ordering {
        // Keep worst (least exact, largest error, largest complexity) at top for eviction.
        self.rank_key
            .cmp(&other.rank_key)
            .then_with(|| compare_expr(&self.m.lhs.expr, &other.m.lhs.expr))
            .then_with(|| compare_expr(&self.m.rhs.expr, &other.m.rhs.expr))
    }
}

/// Statistics from pool operations
#[derive(Clone, Debug, Default)]
pub struct PoolStats {
    /// Number of successful insertions
    pub insertions: usize,
    /// Number rejected due to error threshold
    pub rejections_error: usize,
    /// Number rejected due to deduplication
    pub rejections_dedupe: usize,
    /// Number evicted to maintain capacity
    pub evictions: usize,
}

/// Bounded pool for collecting matches
pub struct TopKPool {
    /// Max capacity
    capacity: usize,
    /// Priority queue (worst at top for eviction)
    heap: BinaryHeap<PoolEntry>,
    /// Seen equation keys for dedupe
    seen_eqn: HashSet<EqnKey>,
    /// Seen LHS keys for soft dedupe
    seen_lhs: HashSet<LhsKey>,
    /// Best error seen so far (for threshold tightening)
    pub best_error: f64,
    /// Accept error threshold (tightens slowly for diversity)
    pub accept_error: f64,
    /// Statistics
    pub stats: PoolStats,
    /// Show diagnostic output for database adds (-DG)
    show_db_adds: bool,
    /// Ranking mode for eviction and output ordering
    ranking_mode: RankingMode,
}

impl TopKPool {
    /// Create a new pool with given capacity
    #[allow(dead_code)]
    pub fn new(capacity: usize, initial_max_error: f64) -> Self {
        Self {
            capacity,
            heap: BinaryHeap::with_capacity(capacity + 1),
            seen_eqn: HashSet::new(),
            seen_lhs: HashSet::new(),
            best_error: initial_max_error,
            accept_error: initial_max_error,
            stats: PoolStats::default(),
            show_db_adds: false,
            ranking_mode: RankingMode::Complexity,
        }
    }

    /// Create a new pool with diagnostic options
    pub fn new_with_diagnostics(
        capacity: usize,
        initial_max_error: f64,
        show_db_adds: bool,
        ranking_mode: RankingMode,
    ) -> Self {
        Self {
            capacity,
            heap: BinaryHeap::with_capacity(capacity + 1),
            seen_eqn: HashSet::new(),
            seen_lhs: HashSet::new(),
            best_error: initial_max_error,
            accept_error: initial_max_error,
            stats: PoolStats::default(),
            show_db_adds,
            ranking_mode,
        }
    }

    /// Try to insert a match into the pool
    /// Returns true if inserted, false if rejected
    pub fn try_insert(&mut self, m: Match) -> bool {
        let error = m.error.abs();
        let is_exact = error < EXACT_MATCH_TOLERANCE;

        // Check error threshold (must be better than accept_error)
        if !is_exact && error > self.accept_error {
            self.stats.rejections_error += 1;
            return false;
        }

        // Check equation-level dedupe
        let eqn_key = EqnKey::from_match(&m);
        if self.seen_eqn.contains(&eqn_key) {
            self.stats.rejections_dedupe += 1;
            return false;
        }

        // Insert
        let entry = PoolEntry::new(m, self.ranking_mode);
        self.seen_eqn.insert(eqn_key);
        self.seen_lhs.insert(LhsKey::from_match(&entry.m));

        // Diagnostic output for -DG (before moving entry into heap)
        if self.show_db_adds {
            eprintln!(
                "  [db add] lhs={:?} rhs={:?} error={:.6e} complexity={}",
                entry.m.lhs.expr.to_postfix(),
                entry.m.rhs.expr.to_postfix(),
                entry.m.error,
                entry.m.complexity
            );
        }

        self.heap.push(entry);
        self.stats.insertions += 1;

        // Update thresholds
        if is_exact {
            // Exact match: tighten best_error aggressively but keep a floor
            self.best_error =
                EXACT_MATCH_TOLERANCE.max(self.best_error * BEST_ERROR_TIGHTEN_FACTOR);
        } else if error < self.best_error {
            // Better approximation: tighten best_error
            self.best_error = error * BEST_ERROR_TIGHTEN_FACTOR - NEWTON_TOLERANCE;
            self.best_error = self.best_error.max(EXACT_MATCH_TOLERANCE);
        }

        // Slowly tighten accept_error for diversity
        if error < self.accept_error * ACCEPT_ERROR_TIGHTEN_FACTOR {
            self.accept_error *= ACCEPT_ERROR_TIGHTEN_FACTOR;
        }

        // Evict worst if over capacity
        if self.heap.len() > self.capacity {
            if let Some(evicted) = self.heap.pop() {
                // Remove from seen sets
                self.seen_eqn.remove(&EqnKey::from_match(&evicted.m));
                // Note: we don't remove from seen_lhs to prevent re-adding variants
                self.stats.evictions += 1;
            }
        }

        true
    }

    /// Check if a match would be accepted (for early pruning)
    pub fn would_accept(&self, error: f64, is_exact: bool) -> bool {
        if is_exact {
            return true;
        }
        error <= self.accept_error
    }

    /// Check if a match would be accepted, with stricter gate when pool is near capacity
    /// This is used as a pre-Newton filter to avoid expensive refinement calls
    pub fn would_accept_strict(&self, coarse_error: f64, is_potentially_exact: bool) -> bool {
        // Always accept potential exact matches
        if is_potentially_exact {
            return true;
        }

        // Basic threshold check
        if coarse_error > self.accept_error {
            return false;
        }

        // Stricter check when pool is near capacity:
        // If we're at capacity fraction and have good matches, be more aggressive
        if self.heap.len() as f64 >= self.capacity as f64 * STRICT_GATE_CAPACITY_FRACTION {
            // Only accept if error is below strict gate threshold
            // This avoids Newton calls for marginal candidates
            if coarse_error > self.accept_error * STRICT_GATE_FACTOR {
                return false;
            }
        }

        true
    }

    /// Get all matches, sorted by ranking mode.
    pub fn into_sorted(self) -> Vec<Match> {
        let ranking_mode = self.ranking_mode;
        let mut matches: Vec<Match> = self.heap.into_iter().map(|e| e.m).collect();
        matches.sort_by(|a, b| compare_matches(a, b, ranking_mode));
        matches
    }

    /// Get current pool size
    pub fn len(&self) -> usize {
        self.heap.len()
    }

    /// Check if pool is empty
    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool {
        self.heap.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::expr::{EvaluatedExpr, Expression};
    use crate::symbol::NumType;

    fn make_match(lhs: &str, rhs: &str, error: f64, complexity: u32) -> Match {
        let lhs_expr = Expression::parse(lhs).unwrap();
        let rhs_expr = Expression::parse(rhs).unwrap();
        Match {
            lhs: EvaluatedExpr::new(lhs_expr, 0.0, 1.0, NumType::Integer),
            rhs: EvaluatedExpr::new(rhs_expr, 0.0, 0.0, NumType::Integer),
            x_value: 2.5,
            error,
            complexity,
        }
    }

    #[test]
    fn test_pool_basic() {
        let mut pool = TopKPool::new(5, 1.0);

        // Insert some matches
        assert!(pool.try_insert(make_match("2x*", "5", 0.0, 27)));
        assert!(pool.try_insert(make_match("x1+", "35/", 0.01, 34)));

        assert_eq!(pool.len(), 2);
    }

    #[test]
    fn test_pool_eviction() {
        let mut pool = TopKPool::new(2, 1.0);

        // Insert 3 matches into pool of capacity 2
        // Worst (highest complexity): xs with complexity 50
        // Best: 2x* with complexity 27 and exact match
        // Medium: x1+ with complexity 34
        pool.try_insert(make_match("xs", "64/", 0.1, 50));
        pool.try_insert(make_match("2x*", "5", 0.0, 27));
        pool.try_insert(make_match("x1+", "35/", 0.01, 34));

        // Should have evicted the worst one (highest complexity)
        assert_eq!(pool.len(), 2);

        let sorted = pool.into_sorted();
        // The two best matches should remain (by complexity)
        // 2x* (27) and x1+ (34) should remain, xs (50) should be evicted
        let remaining: Vec<_> = sorted.iter().map(|m| m.lhs.expr.to_postfix()).collect();
        assert!(
            remaining.contains(&"2x*".to_string()),
            "Expected 2x* to remain, got: {:?}",
            remaining
        );
        assert!(
            remaining.contains(&"x1+".to_string()),
            "Expected x1+ to remain, got: {:?}",
            remaining
        );
    }

    #[test]
    fn test_pool_dedupe() {
        let mut pool = TopKPool::new(10, 1.0);

        // Try to insert same equation twice
        assert!(pool.try_insert(make_match("2x*", "5", 0.0, 27)));
        assert!(!pool.try_insert(make_match("2x*", "5", 0.0, 27)));

        assert_eq!(pool.len(), 1);
    }

    #[test]
    fn test_parity_score_prefers_operator_dense_form() {
        let low_operator = make_match("2x*", "5", 1e-6, 10);
        let high_operator = make_match("x1+", "3", 1e-6, 20);

        let low_score = legacy_parity_score_match(&low_operator);
        let high_score = legacy_parity_score_match(&high_operator);
        assert!(
            high_score < low_score,
            "expected operator-dense form to have lower legacy parity score ({} vs {})",
            high_score,
            low_score
        );
    }

    #[test]
    fn test_parity_ranking_changes_ordering() {
        let low_operator = make_match("2x*", "5", 1e-6, 10);
        let high_operator = make_match("x1+", "3", 1e-6, 20);

        // Complexity mode: simpler complexity first.
        let mut complexity_pool =
            TopKPool::new_with_diagnostics(10, 1.0, false, RankingMode::Complexity);
        complexity_pool.try_insert(low_operator.clone());
        complexity_pool.try_insert(high_operator.clone());
        let complexity_sorted = complexity_pool.into_sorted();
        assert_eq!(complexity_sorted[0].lhs.expr.to_postfix(), "2x*");

        // Parity mode: legacy parity score first.
        let mut parity_pool = TopKPool::new_with_diagnostics(10, 1.0, false, RankingMode::Parity);
        parity_pool.try_insert(low_operator);
        parity_pool.try_insert(high_operator);
        let parity_sorted = parity_pool.into_sorted();
        assert_eq!(parity_sorted[0].lhs.expr.to_postfix(), "x1+");
    }

    #[test]
    fn test_pool_handles_nan_and_infinity_errors() {
        let mut pool = TopKPool::new(10, f64::INFINITY);

        // Normal error should be accepted
        let normal = make_match("x", "1", 0.01, 25);
        assert!(pool.try_insert(normal));

        // Infinity error should sort as worst but still be accepted
        let infinite = make_match("x1+", "2", f64::INFINITY, 30);
        assert!(pool.try_insert(infinite));

        // NaN error should also be handled (sorts as worst)
        let nan_match = make_match("x2*", "3", f64::NAN, 35);
        assert!(pool.try_insert(nan_match));

        // All three should be in the pool
        assert_eq!(pool.len(), 3);

        // When sorted, the normal match should come first (lowest error)
        let sorted = pool.into_sorted();
        assert_eq!(sorted[0].lhs.expr.to_postfix(), "x");
    }

    #[test]
    fn test_pool_entry_distinct_with_same_rank_key() {
        // Two matches with identical rank_key but different expressions
        // should both be insertable (they're distinct equations)
        let mut pool = TopKPool::new(10, 1.0);

        // Both have error 0.0 and same complexity, but different LHS
        let m1 = make_match("x", "1", 0.0, 25);
        let m2 = make_match("x1-", "1", 0.0, 25);

        assert!(pool.try_insert(m1));
        assert!(pool.try_insert(m2));

        // Both should be in the pool since they're different equations
        assert_eq!(pool.len(), 2);
    }
}
