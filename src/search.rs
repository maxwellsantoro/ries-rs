//! Search and matching algorithms
//!
//! Finds equations by matching LHS and RHS expressions.

use crate::expr::EvaluatedExpr;
use ordered_float::OrderedFloat;
use std::collections::BTreeMap;

/// A matched equation
#[derive(Clone)]
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
    pub complexity: u16,
}

impl Match {
    /// Format the match for display
    pub fn display(&self, _target: f64) -> String {
        let lhs_str = self.lhs.expr.to_infix();
        let rhs_str = self.rhs.expr.to_infix();

        let error_str = if self.error.abs() < 1e-14 {
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

/// Search configuration
#[derive(Clone)]
pub struct SearchConfig {
    /// Target value
    pub target: f64,
    /// Maximum number of matches to return
    pub max_matches: usize,
    /// Maximum acceptable error
    pub max_error: f64,
    /// Minimum error improvement factor to report a new match
    pub improvement_factor: f64,
}

impl Default for SearchConfig {
    fn default() -> Self {
        Self {
            target: 0.0,
            max_matches: 100,
            max_error: 1.0,
            improvement_factor: 3.0,
        }
    }
}

/// Database for storing expressions sorted by value
pub struct ExprDatabase {
    /// RHS expressions sorted by value
    rhs_by_value: BTreeMap<OrderedFloat<f64>, Vec<EvaluatedExpr>>,
}

impl ExprDatabase {
    pub fn new() -> Self {
        Self {
            rhs_by_value: BTreeMap::new(),
        }
    }

    /// Insert RHS expressions into the database
    pub fn insert_rhs(&mut self, exprs: Vec<EvaluatedExpr>) {
        for expr in exprs {
            let key = OrderedFloat(expr.value);
            self.rhs_by_value.entry(key).or_default().push(expr);
        }
    }

    /// Find matches for LHS expressions
    pub fn find_matches(
        &self,
        lhs_exprs: &[EvaluatedExpr],
        config: &SearchConfig,
    ) -> Vec<Match> {
        let mut matches = Vec::new();
        let mut best_error = config.max_error;

        // Collect more candidates than needed, then sort and truncate
        // Need to process many LHS expressions to find the best matches
        let collect_limit = config.max_matches * 100;

        // Sort LHS by complexity so simpler expressions are processed first
        let mut sorted_lhs: Vec<_> = lhs_exprs.iter().collect();
        sorted_lhs.sort_by_key(|e| e.expr.complexity());


        for lhs in sorted_lhs {
            // Skip if derivative is too small (can't solve for x)
            if lhs.derivative.abs() < 1e-100 {
                continue;
            }

            // Search for RHS expressions near this LHS value
            let search_radius = best_error * lhs.derivative.abs();
            let low = OrderedFloat(lhs.value - search_radius);
            let high = OrderedFloat(lhs.value + search_radius);

            for (_, rhs_list) in self.rhs_by_value.range(low..=high) {
                for rhs in rhs_list {
                    // Compute initial error estimate
                    let val_diff = lhs.value - rhs.value;
                    let x_delta = -val_diff / lhs.derivative;
                    let error = x_delta.abs();

                    // Skip obviously poor matches
                    if error > best_error {
                        continue;
                    }

                    // Refine with Newton-Raphson
                    if let Some(refined_x) = newton_raphson(
                        &lhs.expr,
                        rhs.value,
                        config.target,
                    ) {
                        let refined_error = refined_x - config.target;

                        // Check if this is a reasonable match
                        if refined_error.abs() < best_error {
                            let m = Match {
                                lhs: lhs.clone(),
                                rhs: rhs.clone(),
                                x_value: refined_x,
                                error: refined_error,
                                complexity: lhs.expr.complexity() + rhs.expr.complexity(),
                            };

                            // Update best error if significant improvement
                            if refined_error.abs() < best_error / config.improvement_factor {
                                best_error = refined_error.abs() * config.improvement_factor;
                            }

                            matches.push(m);

                            // Early exit if we found an exact match OR collected enough
                            if refined_error.abs() < 1e-14 {
                                // Exact match found - continue to find more but mark as done
                            }
                            if matches.len() >= collect_limit && best_error < 0.1 {
                                // Good matches found - can exit early
                                break;
                            }
                        }
                    }
                }
            }
        }

        // Sort by error first (best matches), then by complexity
        matches.sort_by(|a, b| {
            a.error.abs().partial_cmp(&b.error.abs()).unwrap()
                .then_with(|| a.complexity.cmp(&b.complexity))
        });

        matches.truncate(config.max_matches);
        matches
    }
}

impl Default for ExprDatabase {
    fn default() -> Self {
        Self::new()
    }
}

/// Newton-Raphson method to find x where lhs(x) = rhs_value
fn newton_raphson(
    lhs: &crate::expr::Expression,
    rhs_value: f64,
    initial_x: f64,
) -> Option<f64> {
    use crate::eval::evaluate;

    let mut x = initial_x;
    let max_iter = 20;
    let tolerance = 1e-15;

    for _ in 0..max_iter {
        let result = evaluate(lhs, x).ok()?;

        let f = result.value - rhs_value;
        let df = result.derivative;

        if df.abs() < 1e-100 {
            return None; // Derivative too small
        }

        let delta = f / df;
        x -= delta;

        if delta.abs() < tolerance * (1.0 + x.abs()) {
            return Some(x);
        }

        // Check for divergence
        if x.abs() > 1e100 || x.is_nan() {
            return None;
        }
    }

    // Check final result
    let result = evaluate(lhs, x).ok()?;
    if (result.value - rhs_value).abs() < 1e-10 {
        Some(x)
    } else {
        None
    }
}

/// Perform a complete search
pub fn search(
    target: f64,
    max_complexity: u16,
    max_matches: usize,
) -> Vec<Match> {
    use crate::gen::{generate_all, GenConfig};

    // Configure expression generation
    let mut gen_config = GenConfig::default();
    gen_config.max_complexity = max_complexity;

    // Generate expressions
    let generated = generate_all(&gen_config, target);

    println!(
        "Generated {} LHS and {} RHS expressions",
        generated.lhs.len(),
        generated.rhs.len()
    );

    // Build database
    let mut db = ExprDatabase::new();
    db.insert_rhs(generated.rhs);

    // Configure search
    let search_config = SearchConfig {
        target,
        max_matches,
        ..Default::default()
    };

    // Find matches
    db.find_matches(&generated.lhs, &search_config)
}

/// Perform a parallel search using Rayon
#[cfg(feature = "parallel")]
pub fn search_parallel(
    target: f64,
    max_complexity: u16,
    max_matches: usize,
) -> Vec<Match> {
    use crate::gen::{generate_all_parallel, GenConfig};
    use rayon::prelude::*;

    let mut gen_config = GenConfig::default();
    gen_config.max_complexity = max_complexity;

    // Generate expressions in parallel
    let generated = generate_all_parallel(&gen_config, target);

    println!(
        "Generated {} LHS and {} RHS expressions (parallel)",
        generated.lhs.len(),
        generated.rhs.len()
    );

    // Build database
    let mut db = ExprDatabase::new();
    db.insert_rhs(generated.rhs);

    let search_config = SearchConfig {
        target,
        max_matches,
        ..Default::default()
    };

    db.find_matches(&generated.lhs, &search_config)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_search() {
        // Search for equations matching 2.5
        let matches = search(2.5, 50, 10);

        // Should find 2x = 5
        assert!(!matches.is_empty());

        // Print matches for debugging
        for m in &matches {
            println!("{}", m.display(2.5));
        }
    }

    #[test]
    fn test_newton_raphson() {
        use crate::expr::Expression;

        // Test x^2 = 4, should find x = 2
        let expr = Expression::parse("xs").unwrap(); // x^2
        let x = newton_raphson(&expr, 4.0, 1.5).unwrap();
        assert!((x - 2.0).abs() < 1e-10);
    }

    #[test]
    fn test_2x_equals_5() {
        use crate::expr::Expression;
        use crate::eval::evaluate;
        use crate::gen::{generate_all, GenConfig};

        // Test that 2*x is properly generated and evaluated
        let expr = Expression::parse("2x*").unwrap();
        let result = evaluate(&expr, 2.5).unwrap();
        assert!(expr.contains_x(), "2x* should contain x");
        assert!((result.value - 5.0).abs() < 1e-10, "2*2.5 should be 5");

        // Now test if 2x* is generated and matches with 5
        let mut config = GenConfig::default();
        config.max_complexity = 50;
        let generated = generate_all(&config, 2.5);

        // Check if 2x* is in LHS
        let has_2x = generated.lhs.iter()
            .any(|e| e.expr.to_postfix() == "2x*");
        println!("LHS contains 2x*: {}", has_2x);

        // Check if 5 is in RHS
        let has_5 = generated.rhs.iter()
            .any(|e| e.expr.to_postfix() == "5");
        println!("RHS contains 5: {}", has_5);

        // Find expressions with value near 5
        let near_5_lhs: Vec<_> = generated.lhs.iter()
            .filter(|e| (e.value - 5.0).abs() < 0.1)
            .take(5)
            .collect();
        println!("\nLHS expressions with value ≈ 5:");
        for e in &near_5_lhs {
            println!("  {} = {} (value={:.4}, deriv={:.4})",
                e.expr.to_postfix(), e.expr.to_infix(), e.value, e.derivative);
        }

        let near_5_rhs: Vec<_> = generated.rhs.iter()
            .filter(|e| (e.value - 5.0).abs() < 0.1)
            .take(5)
            .collect();
        println!("\nRHS expressions with value ≈ 5:");
        for e in &near_5_rhs {
            println!("  {} = {} (value={:.4})",
                e.expr.to_postfix(), e.expr.to_infix(), e.value);
        }

        assert!(has_2x, "2x* should be generated as LHS");
        assert!(has_5, "5 should be generated as RHS");
    }
}
