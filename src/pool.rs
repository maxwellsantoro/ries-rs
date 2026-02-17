//! Bounded priority pool for streaming match collection
//!
//! Implements a bounded pool that keeps the best matches across
//! multiple dimensions (error, complexity) with deduplication.

use crate::search::Match;
use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashSet};

/// Keys for deduplication
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct EqnKey {
    /// Normalized full equation (LHS + RHS)
    pub key: String,
}

impl EqnKey {
    pub fn from_match(m: &Match) -> Self {
        // Combine LHS and RHS postfix for full equation key
        let key = format!("{}={}", m.lhs.expr.to_postfix(), m.rhs.expr.to_postfix());
        Self { key }
    }
}

#[derive(Clone, PartialEq, Eq, Hash)]
pub struct LhsKey {
    /// Normalized LHS only
    pub key: String,
}

impl LhsKey {
    pub fn from_match(m: &Match) -> Self {
        Self {
            key: m.lhs.expr.to_postfix(),
        }
    }
}

/// Signature for operator/constant pattern (used for "interesting" dedupe)
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct SignatureKey {
    /// Operator pattern signature
    pub key: String,
}

impl SignatureKey {
    pub fn from_match(m: &Match) -> Self {
        use crate::symbol::Seft;

        // Build a signature from operator types and constants used
        let mut ops: Vec<char> = Vec::new();
        for sym in m.lhs.expr.symbols() {
            match sym.seft() {
                Seft::A => ops.push((*sym as u8) as char),
                Seft::B | Seft::C => ops.push((*sym as u8) as char),
            }
        }
        ops.push('=');
        for sym in m.rhs.expr.symbols() {
            match sym.seft() {
                Seft::A => ops.push((*sym as u8) as char),
                Seft::B | Seft::C => ops.push((*sym as u8) as char),
            }
        }

        Self {
            key: ops.into_iter().collect(),
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
        let is_exact = error < 1e-14;

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
            self.best_error = 1e-14_f64.max(self.best_error * 0.999);
        } else if error < self.best_error {
            // Better approximation: tighten best_error
            self.best_error = error * 0.999 - 1e-15;
            self.best_error = self.best_error.max(1e-14);
        }

        // Slowly tighten accept_error for diversity
        if error < self.accept_error * 0.9999 {
            self.accept_error *= 0.9999;
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
        // If we're at 80%+ capacity and have good matches, be more aggressive
        if self.heap.len() >= self.capacity * 4 / 5 {
            // Only accept if error is at least 2x better than accept threshold
            // This avoids Newton calls for marginal candidates
            if coarse_error > self.accept_error * 0.5 {
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
                .then_with(|| a.error.abs().partial_cmp(&b.error.abs()).unwrap())
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
