//! Stability analysis for impostor detection
//!
//! Runs search at multiple precision/tolerance levels and classifies
//! candidates by their stability across runs.

use crate::search::Match;
use std::collections::HashMap;

/// Stability classification for a match
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StabilityClass {
    /// Appears at all tolerance levels - very likely the true formula
    Stable,
    /// Appears at most levels but not all
    ModeratelyStable,
    /// Only appears at loose tolerance - likely an impostor
    Fragile,
    /// Appears at tight tolerance but not loose (rare, indicates numeric issues)
    Anomalous,
}

impl StabilityClass {
    /// Get display name
    pub fn name(&self) -> &'static str {
        match self {
            StabilityClass::Stable => "stable",
            StabilityClass::ModeratelyStable => "moderate",
            StabilityClass::Fragile => "fragile",
            StabilityClass::Anomalous => "anomalous",
        }
    }

    /// Get description
    pub fn description(&self) -> &'static str {
        match self {
            StabilityClass::Stable => "Persists at all precision levels",
            StabilityClass::ModeratelyStable => "Persists at most precision levels",
            StabilityClass::Fragile => "Only appears at low precision (impostor)",
            StabilityClass::Anomalous => "Anomalous stability pattern",
        }
    }
}

/// Result of stability analysis
#[derive(Debug, Clone)]
pub struct StabilityResult {
    /// The match being analyzed
    pub match_: Match,
    /// Stability classification
    pub class: StabilityClass,
    /// Number of levels where this match appeared
    pub appearance_count: usize,
    /// Total number of levels checked
    pub total_levels: usize,
    /// Stability score (0.0 - 1.0, higher is more stable)
    pub score: f64,
}

/// Key for matching expressions across runs
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct ExprKey {
    lhs: String,
    rhs: String,
}

impl ExprKey {
    fn from_match(m: &Match) -> Self {
        Self {
            lhs: m.lhs.expr.to_postfix(),
            rhs: m.rhs.expr.to_postfix(),
        }
    }
}

/// Stability ladder configuration
#[derive(Debug, Clone)]
pub struct StabilityConfig {
    /// Error tolerance multipliers for each level
    /// Level 0 is loosest, higher levels are tighter
    pub tolerance_factors: Vec<f64>,
    /// Minimum appearance ratio for "stable" classification
    pub stable_threshold: f64,
    /// Minimum appearance ratio for "moderately stable" classification
    pub moderate_threshold: f64,
}

impl Default for StabilityConfig {
    fn default() -> Self {
        Self {
            // Run at 100%, 10%, 1%, 0.1%, 0.01% of base error tolerance
            tolerance_factors: vec![1.0, 0.1, 0.01, 0.001, 0.0001],
            stable_threshold: 0.8,   // Appear at 80%+ of levels
            moderate_threshold: 0.5, // Appear at 50-80% of levels
        }
    }
}

impl StabilityConfig {
    /// Create a quick stability config (fewer levels)
    pub fn quick() -> Self {
        Self {
            tolerance_factors: vec![1.0, 0.01, 0.0001],
            stable_threshold: 0.67,
            moderate_threshold: 0.34,
        }
    }

    /// Create a thorough stability config (more levels)
    pub fn thorough() -> Self {
        Self {
            tolerance_factors: vec![1.0, 0.5, 0.1, 0.05, 0.01, 0.005, 0.001, 0.0001],
            stable_threshold: 0.75,
            moderate_threshold: 0.5,
        }
    }
}

/// Analyze stability of matches across multiple runs
pub struct StabilityAnalyzer {
    config: StabilityConfig,
    /// Matches from each level, keyed by expression
    levels: Vec<HashMap<ExprKey, Match>>,
}

impl StabilityAnalyzer {
    /// Create a new stability analyzer
    pub fn new(config: StabilityConfig) -> Self {
        Self {
            config,
            levels: Vec::new(),
        }
    }

    /// Add matches from a run at a specific tolerance level
    pub fn add_level(&mut self, matches: Vec<Match>) {
        let mut level_map = HashMap::new();
        for m in matches {
            let key = ExprKey::from_match(&m);
            level_map.insert(key, m);
        }
        self.levels.push(level_map);
    }

    /// Get the number of levels analyzed
    pub fn level_count(&self) -> usize {
        self.levels.len()
    }

    /// Analyze all matches and return stability results
    pub fn analyze(&self) -> Vec<StabilityResult> {
        let total_levels = self.levels.len();
        if total_levels == 0 {
            return Vec::new();
        }

        // Track all unique expressions and their appearance count
        let mut appearance_counts: HashMap<ExprKey, usize> = HashMap::new();
        let mut best_matches: HashMap<ExprKey, Match> = HashMap::new();

        for level in &self.levels {
            for (key, m) in level {
                *appearance_counts.entry(key.clone()).or_insert(0) += 1;
                // Keep the match from the tightest tolerance (last occurrence)
                best_matches.insert(key.clone(), m.clone());
            }
        }

        // Convert to results
        let mut results: Vec<StabilityResult> = appearance_counts
            .into_iter()
            .map(|(key, count)| {
                let match_ = best_matches.remove(&key).unwrap();
                let ratio = count as f64 / total_levels as f64;
                let class = if ratio >= self.config.stable_threshold {
                    StabilityClass::Stable
                } else if ratio >= self.config.moderate_threshold {
                    StabilityClass::ModeratelyStable
                } else if count == 1 {
                    // Only appeared once
                    if self.is_from_loose_level(&key) {
                        StabilityClass::Fragile
                    } else {
                        StabilityClass::Anomalous
                    }
                } else {
                    StabilityClass::Fragile
                };

                StabilityResult {
                    match_,
                    class,
                    appearance_count: count,
                    total_levels,
                    score: ratio,
                }
            })
            .collect();

        // Sort by stability score (descending), then by error (ascending)
        results.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| {
                    a.match_
                        .error
                        .abs()
                        .partial_cmp(&b.match_.error.abs())
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
        });

        results
    }

    /// Check if a match only appeared in the loosest level
    fn is_from_loose_level(&self, key: &ExprKey) -> bool {
        if self.levels.is_empty() {
            return false;
        }
        // Only in first level
        self.levels[0].contains_key(key) && !self.levels.iter().skip(1).any(|l| l.contains_key(key))
    }
}

/// Format stability results for display
pub fn format_stability_report(results: &[StabilityResult], max_display: usize) -> String {
    let mut output = String::new();

    // Group by stability class
    let stable: Vec<_> = results
        .iter()
        .filter(|r| r.class == StabilityClass::Stable)
        .take(max_display)
        .collect();
    let moderate: Vec<_> = results
        .iter()
        .filter(|r| r.class == StabilityClass::ModeratelyStable)
        .take(max_display)
        .collect();
    let fragile: Vec<_> = results
        .iter()
        .filter(|r| r.class == StabilityClass::Fragile)
        .take(max_display)
        .collect();

    if !stable.is_empty() {
        output.push_str("\n  -- Stable formulas (high confidence) --\n\n");
        for r in &stable {
            output.push_str(&format!(
                "  {:<24} = {:<24}  [{}/{} levels] {{{}}}\n",
                r.match_.lhs.expr.to_infix(),
                r.match_.rhs.expr.to_infix(),
                r.appearance_count,
                r.total_levels,
                r.match_.complexity
            ));
        }
    }

    if !moderate.is_empty() {
        output.push_str("\n  -- Moderately stable (medium confidence) --\n\n");
        for r in &moderate {
            output.push_str(&format!(
                "  {:<24} = {:<24}  [{}/{} levels] {{{}}}\n",
                r.match_.lhs.expr.to_infix(),
                r.match_.rhs.expr.to_infix(),
                r.appearance_count,
                r.total_levels,
                r.match_.complexity
            ));
        }
    }

    if !fragile.is_empty() {
        output.push_str("\n  -- Fragile (likely impostors) --\n\n");
        for r in &fragile {
            output.push_str(&format!(
                "  {:<24} = {:<24}  [{}/{} levels] {{{}}}\n",
                r.match_.lhs.expr.to_infix(),
                r.match_.rhs.expr.to_infix(),
                r.appearance_count,
                r.total_levels,
                r.match_.complexity
            ));
        }
    }

    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::expr::{EvaluatedExpr, Expression};
    use crate::symbol::NumType;

    fn make_test_match(lhs: &str, rhs: &str, error: f64) -> Match {
        let lhs_expr = Expression::parse(lhs).unwrap();
        let rhs_expr = Expression::parse(rhs).unwrap();
        Match {
            lhs: EvaluatedExpr::new(lhs_expr.clone(), 0.0, 1.0, NumType::Integer),
            rhs: EvaluatedExpr::new(rhs_expr.clone(), 0.0, 0.0, NumType::Integer),
            x_value: 2.5,
            error,
            complexity: lhs_expr.complexity() + rhs_expr.complexity(),
        }
    }

    #[test]
    fn test_stability_classification() {
        let mut analyzer = StabilityAnalyzer::new(StabilityConfig::default());

        // Level 0: Both matches appear
        analyzer.add_level(vec![
            make_test_match("x", "5", 0.01),
            make_test_match("2x*", "5", 0.001),
        ]);

        // Level 1: Only first match appears
        analyzer.add_level(vec![make_test_match("x", "5", 0.001)]);

        // Level 2: Only first match appears
        analyzer.add_level(vec![make_test_match("x", "5", 0.0001)]);

        let results = analyzer.analyze();
        assert_eq!(results.len(), 2);

        // First match is stable (appears at 3/3 levels)
        let stable = results
            .iter()
            .find(|r| r.match_.lhs.expr.to_postfix() == "x");
        assert!(stable.is_some());
        assert_eq!(stable.unwrap().class, StabilityClass::Stable);
        assert_eq!(stable.unwrap().appearance_count, 3);

        // Second match is fragile (appears at 1/3 levels)
        let fragile = results
            .iter()
            .find(|r| r.match_.lhs.expr.to_postfix() == "2x*");
        assert!(fragile.is_some());
        assert_eq!(fragile.unwrap().class, StabilityClass::Fragile);
        assert_eq!(fragile.unwrap().appearance_count, 1);
    }

    #[test]
    fn test_empty_analyzer() {
        let analyzer = StabilityAnalyzer::new(StabilityConfig::default());
        assert_eq!(analyzer.analyze().len(), 0);
    }
}
