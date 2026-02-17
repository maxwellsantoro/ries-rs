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

/// Wrapper for Match that implements ordering for the heap
/// We want a min-heap by (complexity, error) so we reverse the ordering
#[derive(Clone)]
struct PoolEntry {
    m: Match,
    rank_key: (u32, i64), // (complexity, error_bits) - lower is better
}

impl PoolEntry {
    fn new(m: Match) -> Self {
        // Convert error to sortable integer (negative exponent first)
        let error_bits = m.error.abs().to_bits() as i64;
        Self {
            rank_key: (m.complexity, error_bits),
            m,
        }
    }
}

impl PartialEq for PoolEntry {
    fn eq(&self, other: &Self) -> bool {
        self.rank_key == other.rank_key
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
        // Keep worst (highest complexity/error) at the top for eviction
        self.rank_key.cmp(&other.rank_key)
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
}

impl TopKPool {
    /// Create a new pool with given capacity
    pub fn new(capacity: usize, initial_max_error: f64) -> Self {
        Self {
            capacity,
            heap: BinaryHeap::with_capacity(capacity + 1),
            seen_eqn: HashSet::new(),
            seen_lhs: HashSet::new(),
            best_error: initial_max_error,
            accept_error: initial_max_error,
            stats: PoolStats::default(),
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
        let entry = PoolEntry::new(m);
        self.seen_eqn.insert(eqn_key);
        self.seen_lhs.insert(LhsKey::from_match(&entry.m));
        self.heap.push(entry);
        self.stats.insertions += 1;

        // Update thresholds
        if is_exact {
            // Exact match: tighten best_error aggressively but keep a floor
            self.best_error = EXACT_MATCH_TOLERANCE.max(self.best_error * BEST_ERROR_TIGHTEN_FACTOR);
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

    /// Get all matches, sorted by (complexity, error)
    pub fn into_sorted(self) -> Vec<Match> {
        let mut matches: Vec<Match> = self.heap.into_iter().map(|e| e.m).collect();
        matches.sort_by(|a, b| {
            a.complexity
                .cmp(&b.complexity)
                .then_with(|| {
                    a.error
                        .abs()
                        .partial_cmp(&b.error.abs())
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
        });
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
}
