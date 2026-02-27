//! Metrics and scoring for match categorization
//!
//! Computes scores across multiple dimensions to categorize matches:
//! - Exact: error below machine epsilon
//! - Best: lowest error approximations
//! - Elegant: simplest/cleanest expressions
//! - Interesting: novel/unexpected combinations
//! - Stable: robust matches (good Newton conditioning)

use crate::search::Match;
use crate::symbol::{Seft, Symbol};
use crate::thresholds::{DEGENERATE_TEST_THRESHOLD, EXACT_MATCH_TOLERANCE};
use std::collections::HashMap;

/// Metrics computed for a match
#[derive(Clone, Debug)]
pub struct MatchMetrics {
    /// Absolute error from target
    pub error: f64,
    /// Whether this is an exact match (error < 1e-14)
    pub is_exact: bool,
    /// Total complexity score
    pub complexity: u32,
    /// "Ugliness" penalty (deep nesting, many ops)
    pub ugliness: f64,
    /// Novelty score (rarer operators/constants)
    pub novelty: f64,
    /// Stability score (Newton conditioning)
    pub stability: f64,
    /// Operator diversity score
    pub diversity: f64,
}

impl MatchMetrics {
    /// Compute metrics for a match
    pub fn from_match(m: &Match, freq_map: Option<&OperatorFrequency>) -> Self {
        let error = m.error.abs();
        let is_exact = error < EXACT_MATCH_TOLERANCE;
        let complexity = m.complexity;

        // Ugliness: penalize deep nesting and operator count
        let ugliness = compute_ugliness(m);

        // Novelty: based on operator rarity
        let novelty = compute_novelty(m, freq_map);

        // Stability: based on derivative magnitude at solution
        let stability = compute_stability(m);

        // Diversity: bonus for mixed operator families
        let diversity = compute_diversity(m);

        Self {
            error,
            is_exact,
            complexity,
            ugliness,
            novelty,
            stability,
            diversity,
        }
    }

    /// Elegant score: lower is better
    /// Optimizes for simplicity and cleanliness
    pub fn elegant_score(&self) -> f64 {
        self.complexity as f64 + 0.1 * self.ugliness
    }

    /// Interesting score: higher is better
    /// Optimizes for novelty while maintaining reasonable error
    pub fn interesting_score(&self, error_cap: f64) -> f64 {
        if self.error > error_cap {
            return f64::NEG_INFINITY;
        }

        // Normalize error to [0, 1] range within cap.
        // When error_cap == EXACT_MATCH_TOLERANCE (1e-14) the denominator is 0;
        // treat it as a near-exact match (error_norm = 0).
        let error_norm = if self.error < EXACT_MATCH_TOLERANCE {
            0.0
        } else {
            let denom = error_cap.log10() + 14.0;
            if denom.abs() < f64::EPSILON {
                0.0
            } else {
                (self.error.log10() + 14.0) / denom
            }
        };

        // Normalize complexity to rough [0, 1] range (100 = max typical)
        let complexity_norm = (self.complexity as f64) / 100.0;

        // Score formula: novelty is king, but penalize high error and complexity
        self.novelty + 0.3 * self.diversity - 0.7 * error_norm - 0.2 * complexity_norm
    }

    /// Stable score: higher is better
    pub fn stable_score(&self) -> f64 {
        self.stability
    }
}

/// Operator frequency map for computing rarity
#[derive(Default)]
pub struct OperatorFrequency {
    /// Count of each symbol across all matches
    symbol_counts: HashMap<Symbol, usize>,
    /// Total symbol occurrences
    total: usize,
    /// Bigram counts (consecutive symbols)
    bigram_counts: HashMap<(Symbol, Symbol), usize>,
    /// Total bigrams
    total_bigrams: usize,
}

impl OperatorFrequency {
    /// Create a new frequency map
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a match to the frequency counts
    pub fn add(&mut self, m: &Match) {
        let lhs_syms = m.lhs.expr.symbols();
        let rhs_syms = m.rhs.expr.symbols();

        // Count symbols
        for &sym in lhs_syms.iter().chain(rhs_syms.iter()) {
            *self.symbol_counts.entry(sym).or_insert(0) += 1;
            self.total += 1;
        }

        // Count bigrams
        for window in lhs_syms.windows(2) {
            let bigram = (window[0], window[1]);
            *self.bigram_counts.entry(bigram).or_insert(0) += 1;
            self.total_bigrams += 1;
        }
        for window in rhs_syms.windows(2) {
            let bigram = (window[0], window[1]);
            *self.bigram_counts.entry(bigram).or_insert(0) += 1;
            self.total_bigrams += 1;
        }
    }

    /// Get rarity score for a symbol (higher = rarer)
    pub fn symbol_rarity(&self, sym: Symbol) -> f64 {
        if self.total == 0 {
            return 1.0;
        }
        let count = self.symbol_counts.get(&sym).copied().unwrap_or(0);
        if count == 0 {
            return 2.0; // Very rare (not seen)
        }
        let freq = count as f64 / self.total as f64;
        // Inverse log frequency as rarity
        (-freq.log10()).max(0.0)
    }

    /// Get rarity score for a bigram
    pub fn bigram_rarity(&self, a: Symbol, b: Symbol) -> f64 {
        if self.total_bigrams == 0 {
            return 1.0;
        }
        let count = self.bigram_counts.get(&(a, b)).copied().unwrap_or(0);
        if count == 0 {
            return 2.0;
        }
        let freq = count as f64 / self.total_bigrams as f64;
        (-freq.log10()).max(0.0)
    }
}

/// Compute ugliness score
fn compute_ugliness(m: &Match) -> f64 {
    let mut score = 0.0;

    // Penalize total operator count
    let op_count = count_operators(&m.lhs) + count_operators(&m.rhs);
    score += op_count as f64 * 0.5;

    // Penalize nesting depth (approximated by expression length)
    let total_len = m.lhs.expr.len() + m.rhs.expr.len();
    if total_len > 8 {
        score += (total_len - 8) as f64 * 0.3;
    }

    // Penalize transcendental operators (they're "expensive")
    for sym in m.lhs.expr.symbols().iter().chain(m.rhs.expr.symbols()) {
        if matches!(
            sym,
            Symbol::Ln
                | Symbol::Exp
                | Symbol::SinPi
                | Symbol::CosPi
                | Symbol::TanPi
                | Symbol::LambertW
                | Symbol::Log
                | Symbol::Atan2
        ) {
            score += 1.0;
        }
    }

    score
}

/// Count operators in an expression
fn count_operators(expr: &crate::expr::EvaluatedExpr) -> usize {
    expr.expr
        .symbols()
        .iter()
        .filter(|s| s.seft() != Seft::A)
        .count()
}

/// Compute novelty score based on operator rarity
fn compute_novelty(m: &Match, freq_map: Option<&OperatorFrequency>) -> f64 {
    let mut score = 0.0;

    // Base novelty from using uncommon operators
    for sym in m.lhs.expr.symbols().iter().chain(m.rhs.expr.symbols()) {
        if let Some(freq) = freq_map {
            score += freq.symbol_rarity(*sym);
        } else {
            // Default rarity based on operator type
            score += default_rarity(*sym);
        }
    }

    // Bonus for bigram rarity
    if let Some(freq) = freq_map {
        let lhs_syms = m.lhs.expr.symbols();
        for window in lhs_syms.windows(2) {
            score += freq.bigram_rarity(window[0], window[1]) * 0.5;
        }
    }

    // Normalize by expression length
    let len = (m.lhs.expr.len() + m.rhs.expr.len()).max(1);
    score / len as f64
}

/// Default rarity for operators (when no frequency map available)
fn default_rarity(sym: Symbol) -> f64 {
    match sym {
        // Common constants
        Symbol::One | Symbol::Two | Symbol::X => 0.1,
        Symbol::Three | Symbol::Four | Symbol::Five => 0.2,
        Symbol::Pi | Symbol::E => 0.3,
        Symbol::Six | Symbol::Seven | Symbol::Eight | Symbol::Nine => 0.4,
        Symbol::Phi => 0.6,
        // New constants - medium-high rarity (less common)
        Symbol::Gamma => 0.7,
        Symbol::Plastic => 0.7,
        Symbol::Apery => 0.8,
        Symbol::Catalan => 0.7,

        // Common operators
        Symbol::Add | Symbol::Sub | Symbol::Mul | Symbol::Div => 0.2,
        Symbol::Pow | Symbol::Sqrt | Symbol::Square => 0.3,

        // Less common operators
        Symbol::Recip | Symbol::Neg => 0.4,
        Symbol::Ln | Symbol::Exp => 0.5,

        // Uncommon operators (higher novelty)
        Symbol::SinPi | Symbol::CosPi => 0.7,
        Symbol::TanPi => 0.8,
        Symbol::Root | Symbol::Log => 0.7,
        Symbol::LambertW | Symbol::Atan2 => 1.0,

        // User constants - medium rarity
        Symbol::UserConstant0
        | Symbol::UserConstant1
        | Symbol::UserConstant2
        | Symbol::UserConstant3
        | Symbol::UserConstant4
        | Symbol::UserConstant5
        | Symbol::UserConstant6
        | Symbol::UserConstant7
        | Symbol::UserConstant8
        | Symbol::UserConstant9
        | Symbol::UserConstant10
        | Symbol::UserConstant11
        | Symbol::UserConstant12
        | Symbol::UserConstant13
        | Symbol::UserConstant14
        | Symbol::UserConstant15 => 0.5,

        // User functions - medium-high rarity (custom operations)
        Symbol::UserFunction0
        | Symbol::UserFunction1
        | Symbol::UserFunction2
        | Symbol::UserFunction3
        | Symbol::UserFunction4
        | Symbol::UserFunction5
        | Symbol::UserFunction6
        | Symbol::UserFunction7
        | Symbol::UserFunction8
        | Symbol::UserFunction9
        | Symbol::UserFunction10
        | Symbol::UserFunction11
        | Symbol::UserFunction12
        | Symbol::UserFunction13
        | Symbol::UserFunction14
        | Symbol::UserFunction15 => 0.6,
    }
}

/// Compute stability score based on Newton conditioning
fn compute_stability(m: &Match) -> f64 {
    let deriv = m.lhs.derivative.abs();

    // Ideal: derivative magnitude near 1 (order-1 updates)
    // Bad: too small (sensitive) or too large (ill-conditioned)
    if deriv < DEGENERATE_TEST_THRESHOLD {
        return 0.0; // Very unstable (degenerate)
    }

    let log_deriv = deriv.log10();

    // Sweet spot: log10(deriv) near 0
    // Penalize extremes
    let distance_from_ideal = log_deriv.abs();

    // Score: higher is better, max at 1.0
    (1.0 - distance_from_ideal / 5.0).max(0.0)
}

/// Compute diversity score (bonus for mixed operator families)
fn compute_diversity(m: &Match) -> f64 {
    let mut has_algebraic = false;
    let mut has_transcendental = false;
    let mut has_trigonometric = false;

    for sym in m.lhs.expr.symbols().iter().chain(m.rhs.expr.symbols()) {
        match sym {
            Symbol::Add
            | Symbol::Sub
            | Symbol::Mul
            | Symbol::Div
            | Symbol::Pow
            | Symbol::Sqrt
            | Symbol::Square
            | Symbol::Root
            | Symbol::Neg
            | Symbol::Recip => has_algebraic = true,

            Symbol::Ln | Symbol::Exp | Symbol::LambertW => has_transcendental = true,

            Symbol::SinPi | Symbol::CosPi | Symbol::TanPi | Symbol::Atan2 => {
                has_trigonometric = true;
            }

            _ => {}
        }
    }

    let mut score = 0.0;
    let count = [has_algebraic, has_transcendental, has_trigonometric]
        .iter()
        .filter(|&&b| b)
        .count();

    if count >= 2 {
        score += 0.5;
    }
    if count >= 3 {
        score += 0.5;
    }

    score
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::expr::{EvaluatedExpr, Expression};
    use crate::symbol::NumType;

    fn make_match(lhs: &str, rhs: &str, error: f64, deriv: f64) -> Match {
        let lhs_expr = Expression::parse(lhs).unwrap();
        let rhs_expr = Expression::parse(rhs).unwrap();
        Match {
            lhs: EvaluatedExpr::new(lhs_expr.clone(), 0.0, deriv, NumType::Integer),
            rhs: EvaluatedExpr::new(rhs_expr.clone(), 0.0, 0.0, NumType::Integer),
            x_value: 2.5,
            error,
            complexity: lhs_expr.complexity() + rhs_expr.complexity(),
        }
    }

    #[test]
    fn test_metrics_exact() {
        let m = make_match("2x*", "5", 0.0, 2.0);
        let metrics = MatchMetrics::from_match(&m, None);

        assert!(metrics.is_exact);
        assert!(metrics.stability > 0.5); // Good conditioning
    }

    #[test]
    fn test_elegant_score() {
        let simple = make_match("2x*", "5", 0.0, 2.0);
        let complex = make_match("xx^ps+", "3qE", 0.001, 1.0);

        let simple_metrics = MatchMetrics::from_match(&simple, None);
        let complex_metrics = MatchMetrics::from_match(&complex, None);

        // Simpler expression should have lower elegant score
        assert!(simple_metrics.elegant_score() < complex_metrics.elegant_score());
    }

    #[test]
    fn test_stability_extremes() {
        let stable = make_match("x", "25/", 0.0, 1.0);
        let unstable = make_match("x", "25/", 0.0, 1e-12);

        let stable_metrics = MatchMetrics::from_match(&stable, None);
        let unstable_metrics = MatchMetrics::from_match(&unstable, None);

        assert!(stable_metrics.stability > unstable_metrics.stability);
    }

    /// Issue: when error_cap == EXACT_MATCH_TOLERANCE (1e-14), the denominator
    /// `error_cap.log10() + 14.0` is exactly 0.0, producing NaN via 0/0.
    #[test]
    fn test_interesting_score_finite_at_exact_tolerance_boundary() {
        // error == error_cap == 1e-14: falls into the division branch (not < EXACT_MATCH_TOLERANCE)
        // and denominator = 1e-14.log10() + 14.0 = -14 + 14 = 0 → 0/0 = NaN before fix.
        let m = make_match("2x*", "5", EXACT_MATCH_TOLERANCE, 2.0);
        let metrics = MatchMetrics::from_match(&m, None);
        let interesting = metrics.interesting_score(EXACT_MATCH_TOLERANCE);
        assert!(
            interesting.is_finite(),
            "interesting_score must be finite, got {interesting}"
        );
    }
}
