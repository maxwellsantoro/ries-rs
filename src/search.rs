//! Search and matching algorithms
//!
//! Finds equations by matching LHS and RHS expressions.

use crate::expr::EvaluatedExpr;
use crate::pool::TopKPool;
use crate::profile::UserConstant;
use std::time::{Duration, Instant};

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
        println!("    Time:            {:>10.3}ms", self.gen_time.as_secs_f64() * 1000.0);
        println!("    LHS expressions: {:>10}", self.lhs_count);
        println!("    RHS expressions: {:>10}", self.rhs_count);
        println!();
        println!("  Search:");
        println!("    Time:            {:>10.3}ms", self.search_time.as_secs_f64() * 1000.0);
        println!("    LHS tested:      {:>10}", self.lhs_tested);
        println!("    Candidates:      {:>10}", self.candidates_tested);
        println!("    Newton calls:    {:>10}", self.newton_calls);
        println!("    Newton success:  {:>10} ({:.1}%)",
            self.newton_success,
            if self.newton_calls > 0 { 100.0 * self.newton_success as f64 / self.newton_calls as f64 } else { 0.0 });
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
    /// Format the match for display (used in tests)
    #[cfg(test)]
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
    /// Stop search when exact match is found
    pub stop_at_exact: bool,
    /// Stop search when error goes below this threshold
    pub stop_below: Option<f64>,
    /// Threshold for pruning LHS expressions with near-zero values
    /// (prevents flooding with trivial matches like cospi(2.5)=0)
    pub zero_value_threshold: f64,
    /// Maximum Newton-Raphson iterations for root refinement
    pub newton_iterations: usize,
    /// User-defined constants for evaluation
    pub user_constants: Vec<UserConstant>,
    /// User-defined functions for evaluation
    pub user_functions: Vec<crate::udf::UserFunction>,
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
        }
    }
}

/// Database for storing expressions sorted by value
/// Uses a flat sorted vector for cache-friendly range scans
pub struct ExprDatabase {
    /// RHS expressions sorted by value (flat vector for cache locality)
    rhs_sorted: Vec<EvaluatedExpr>,
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
        exprs.sort_by(|a, b| a.value.partial_cmp(&b.value).unwrap_or(std::cmp::Ordering::Equal));
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
    pub fn find_matches(
        &self,
        lhs_exprs: &[EvaluatedExpr],
        config: &SearchConfig,
    ) -> Vec<Match> {
        let (matches, _stats) = self.find_matches_with_stats(lhs_exprs, config);
        matches
    }

    /// Find matches with statistics collection
    pub fn find_matches_with_stats(
        &self,
        lhs_exprs: &[EvaluatedExpr],
        config: &SearchConfig,
    ) -> (Vec<Match>, SearchStats) {
        let mut stats = SearchStats::new();
        let search_start = Instant::now();

        // Use target-scaled initial threshold like original RIES
        let initial_max_error = config.max_error.max(config.target.abs() * 0.01).max(1e-12);

        // Create bounded pool with configured capacity
        let mut pool = TopKPool::new(config.max_matches, initial_max_error);

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
                continue;
            }

            // Skip degenerate expressions: contain x but derivative is 0
            // These are trivial identities like 1^x=1, x/x=1, log_x(x)=1
            if lhs.derivative.abs() < 1e-10 {
                // To distinguish true repeated roots from degenerate expressions,
                // evaluate at a different x value. Degenerate expressions have
                // derivative 0 everywhere; true repeated roots only at specific x.
                // Use an irrational offset to avoid hitting special values
                let test_x = config.target + std::f64::consts::E;
                if let Ok(test_result) = crate::eval::evaluate_with_constants(&lhs.expr, test_x, &config.user_constants) {
                    // Check both: derivative still ~0, AND value unchanged
                    // This catches x*(1/x)=1 type expressions
                    let value_unchanged = (test_result.value - lhs.value).abs() < 1e-10;
                    let deriv_still_zero = test_result.derivative.abs() < 1e-10;
                    if deriv_still_zero || value_unchanged {
                        // Degenerate expression - skip
                        continue;
                    }
                }
                // Derivative is non-zero at test_x, so this might be a true repeated root
                // Check if LHS(target) ≈ some RHS
                let val_error = 0.01;
                let low = lhs.value - val_error;
                let high = lhs.value + val_error;

                stats.lhs_tested += 1;
                for rhs in self.range(low, high) {
                    stats.candidates_tested += 1;
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
                stats.candidates_tested += 1;

                // Compute initial error estimate (coarse filter)
                let val_diff = lhs.value - rhs.value;
                let x_delta = -val_diff / lhs.derivative;
                let coarse_error = x_delta.abs();

                // Skip if coarse estimate won't pass threshold
                // Use strict gate to avoid expensive Newton calls for marginal candidates
                let is_potentially_exact = coarse_error < 1e-10;
                if !pool.would_accept_strict(coarse_error, is_potentially_exact) {
                    continue;
                }

                // Refine with Newton-Raphson
                stats.newton_calls += 1;
                if let Some(refined_x) = newton_raphson_with_constants(
                    &lhs.expr,
                    rhs.value,
                    config.target,
                    config.newton_iterations,
                    &config.user_constants,
                    &config.user_functions,
                ) {
                    stats.newton_success += 1;
                    let refined_error = refined_x - config.target;
                    let is_exact = refined_error.abs() < 1e-14;

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

/// Newton-Raphson method to find x where lhs(x) = rhs_value
/// Uses configurable max iterations - balances speed vs. convergence
///
/// Uses thread-local workspace for zero heap allocations in hot loop.
fn newton_raphson(
    lhs: &crate::expr::Expression,
    rhs_value: f64,
    initial_x: f64,
    max_iterations: usize,
) -> Option<f64> {
    newton_raphson_with_constants(lhs, rhs_value, initial_x, max_iterations, &[], &[])
}

/// Newton-Raphson with user constants support
fn newton_raphson_with_constants(
    lhs: &crate::expr::Expression,
    rhs_value: f64,
    initial_x: f64,
    max_iterations: usize,
    user_constants: &[crate::profile::UserConstant],
    user_functions: &[crate::udf::UserFunction],
) -> Option<f64> {
    use crate::eval::evaluate_fast_with_constants_and_functions;

    let mut x = initial_x;
    let tolerance = 1e-15;

    for _ in 0..max_iterations {
        let result = evaluate_fast_with_constants_and_functions(lhs, x, user_constants, user_functions).ok()?;
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
    let result = evaluate_fast_with_constants_and_functions(lhs, x, user_constants, user_functions).ok()?;
    if (result.value - rhs_value).abs() < 1e-10 {
        Some(x)
    } else {
        None
    }
}

/// Perform a complete search
pub fn search(
    target: f64,
    gen_config: &crate::gen::GenConfig,
    max_matches: usize,
) -> Vec<Match> {
    let (matches, _stats) = search_with_stats(target, gen_config, max_matches);
    matches
}

/// Perform a complete search with statistics collection
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
    use crate::gen::generate_all;

    let gen_start = Instant::now();

    // Generate expressions
    let generated = generate_all(gen_config, target);
    let gen_time = gen_start.elapsed();

    // Build database
    let mut db = ExprDatabase::new();
    db.insert_rhs(generated.rhs);

    // Configure search with user constants from gen_config
    let search_config = SearchConfig {
        target,
        max_matches,
        stop_at_exact,
        stop_below,
        user_constants: gen_config.user_constants.clone(),
        ..Default::default()
    };

    // Find matches with stats
    let (matches, mut stats) = db.find_matches_with_stats(&generated.lhs, &search_config);

    // Add generation stats
    stats.gen_time = gen_time;
    stats.lhs_count = generated.lhs.len();
    stats.rhs_count = db.rhs_count();

    (matches, stats)
}

/// Perform a parallel search using Rayon
#[cfg(feature = "parallel")]
pub fn search_parallel(
    target: f64,
    gen_config: &crate::gen::GenConfig,
    max_matches: usize,
) -> Vec<Match> {
    let (matches, _stats) = search_parallel_with_stats(target, gen_config, max_matches);
    matches
}

/// Perform a parallel search with statistics collection
#[cfg(feature = "parallel")]
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
    use crate::gen::generate_all_parallel;

    let gen_start = Instant::now();

    // Generate expressions in parallel
    let generated = generate_all_parallel(gen_config, target);
    let gen_time = gen_start.elapsed();

    // Build database
    let mut db = ExprDatabase::new();
    db.insert_rhs(generated.rhs);

    // Configure search with user constants from gen_config
    let search_config = SearchConfig {
        target,
        max_matches,
        stop_at_exact,
        stop_below,
        user_constants: gen_config.user_constants.clone(),
        ..Default::default()
    };

    // Find matches with stats
    let (matches, mut stats) = db.find_matches_with_stats(&generated.lhs, &search_config);

    // Add generation stats
    stats.gen_time = gen_time;
    stats.lhs_count = generated.lhs.len();
    stats.rhs_count = db.rhs_count();

    (matches, stats)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_search() {
        use crate::gen::GenConfig;

        // Search for equations matching 2.5
        let mut config = GenConfig::default();
        config.max_lhs_complexity = 50;
        config.max_rhs_complexity = 50;
        let matches = search(2.5, &config, 10);

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
        let x = newton_raphson(&expr, 4.0, 1.5, 15).unwrap();
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
        config.max_lhs_complexity = 50;
        config.max_rhs_complexity = 50;
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

#[test]
fn test_xx_match_directly() {
    use crate::gen::{generate_all, GenConfig};
    
    let mut config = GenConfig::default();
    config.max_lhs_complexity = 50;
    config.max_rhs_complexity = 50;
    
    let generated = generate_all(&config, 2.5);
    
    // Find x^x LHS
    let xx = generated.lhs.iter()
        .find(|e| e.expr.to_postfix() == "xx^")
        .expect("xx^ should exist");
    
    println!("xx^ LHS: value={:.6}, deriv={:.6}", xx.value, xx.derivative);
    
    // Find pi^2 RHS
    let pi_sq = generated.rhs.iter()
        .find(|e| e.expr.to_postfix() == "ps")
        .expect("ps should exist");
    
    println!("ps RHS: value={:.6}", pi_sq.value);
    
    // Check the matching manually
    let val_diff = xx.value - pi_sq.value;
    let x_delta = -val_diff / xx.derivative;
    let error = x_delta.abs();
    
    println!("\nMatching:");
    println!("  val_diff = {:.6}", val_diff);
    println!("  x_delta = {:.6}", x_delta);
    println!("  error = {:.6}", error);
    
    // Try Newton-Raphson
    use crate::eval::evaluate;
    let mut x = 2.5_f64;
    for i in 0..10 {
        let result = evaluate(&xx.expr, x).unwrap();
        let f = result.value - pi_sq.value;
        let df = result.derivative;
        println!("  NR iter {}: x={:.10}, f={:.10}, df={:.6}", i, x, f, df);
        if df.abs() < 1e-100 {
            println!("  Derivative too small!");
            break;
        }
        let delta = f / df;
        x -= delta;
        if delta.abs() < 1e-15 * (1.0 + x.abs()) {
            println!("  Converged!");
            break;
        }
    }
    println!("\nFinal x = {:.15}, error from 2.5 = {:.10}", x, x - 2.5);
}

#[test]
fn test_search_finds_xx() {
    use crate::gen::{generate_all, GenConfig};
    
    let mut config = GenConfig::default();
    config.max_lhs_complexity = 50;
    config.max_rhs_complexity = 50;
    
    let target = 2.5;
    let generated = generate_all(&config, target);
    
    println!("Generated {} LHS and {} RHS", generated.lhs.len(), generated.rhs.len());
    
    // Build database
    let mut db = ExprDatabase::new();
    db.insert_rhs(generated.rhs.clone());
    
    // Find x^x
    let xx = generated.lhs.iter()
        .find(|e| e.expr.to_postfix() == "xx^")
        .expect("xx^ should exist");
    
    println!("\nLooking for match for xx^: value={:.6}, deriv={:.6}", xx.value, xx.derivative);
    
    // Check what's in the RHS database near 9.88
    let search_radius = 1.0 * xx.derivative.abs(); // Allow error up to 1.0
    println!("Search radius: {:.2}", search_radius);
    println!("Search range: [{:.2}, {:.2}]", xx.value - search_radius, xx.value + search_radius);
    
    let low = xx.value - search_radius;
    let high = xx.value + search_radius;

    let in_range: Vec<_> = db.range(low, high).iter().take(10).collect();

    println!("\nRHS in range:");
    for e in &in_range {
        println!("  {} = {:.6}", e.expr.to_postfix(), e.value);
    }
    
    // Now do the full search
    let search_config = SearchConfig {
        target,
        max_matches: 100,
        max_error: 1.0,
        ..Default::default()
    };

    let matches = db.find_matches(&generated.lhs, &search_config);

    println!("\nFound {} matches", matches.len());
    
    // Check for x^x match
    let xx_match = matches.iter()
        .find(|m| m.lhs.expr.to_postfix() == "xx^");
    
    if let Some(m) = xx_match {
        println!("Found x^x match: {} = {} (error={:.6})", 
            m.lhs.expr.to_infix(), m.rhs.expr.to_infix(), m.error);
    } else {
        println!("x^x match NOT found!");
    }
}

#[test]
fn test_find_ps_in_rhs_db() {
    use crate::gen::{generate_all, GenConfig};
    
    let mut config = GenConfig::default();
    config.max_lhs_complexity = 50;
    config.max_rhs_complexity = 50;
    
    let generated = generate_all(&config, 2.5);
    
    // Check if ps is in RHS
    let ps = generated.rhs.iter()
        .find(|e| e.expr.to_postfix() == "ps");
    
    if let Some(e) = ps {
        println!("ps in RHS: value={:.10}", e.value);
    } else {
        println!("ps NOT in generated RHS!");
        return;
    }
    
    // Build database
    let mut db = ExprDatabase::new();
    db.insert_rhs(generated.rhs);
    
    // Check if ps value is in database
    let pi_sq = std::f64::consts::PI.powi(2);

    let nearby: Vec<_> = db.range(pi_sq - 0.001, pi_sq + 0.001).iter().collect();

    if nearby.is_empty() {
        println!("No expressions at exact pi^2 value in database");
    } else {
        println!("Found {} expressions near pi^2 value", nearby.len());
        for e in &nearby {
            println!("  {} at {:.10}", e.expr.to_postfix(), e.value);
        }
    }
}

#[test]
fn test_xx_match_step_by_step() {
    use crate::gen::{generate_all, GenConfig};
    use crate::eval::evaluate;
    
    let mut config = GenConfig::default();
    config.max_lhs_complexity = 50;
    config.max_rhs_complexity = 50;
    
    let target = 2.5;
    let generated = generate_all(&config, target);
    
    // Get xx^ and ps
    let xx = generated.lhs.iter()
        .find(|e| e.expr.to_postfix() == "xx^")
        .expect("xx^");
    let ps = generated.rhs.iter()
        .find(|e| e.expr.to_postfix() == "ps")
        .expect("ps");
    
    println!("xx^: value={:.6}, deriv={:.6}, complexity={}", 
        xx.value, xx.derivative, xx.expr.complexity());
    println!("ps: value={:.6}, complexity={}", 
        ps.value, ps.expr.complexity());
    
    // Simulate the search logic
    let best_error = 1e-12; // After finding exact matches
    let max_error = 1.0;
    
    // Calculate search radius  
    let min_search_radius = 0.5 * xx.derivative.abs();
    let search_radius = (best_error * xx.derivative.abs()).max(min_search_radius);
    println!("\nSearch radius: {:.6} (min={:.6})", search_radius, min_search_radius);
    
    // Check if ps is in range
    let low = xx.value - search_radius;
    let high = xx.value + search_radius;
    println!("Range: [{:.6}, {:.6}]", low, high);
    println!("ps.value={:.6} in range? {}", ps.value, ps.value >= low && ps.value <= high);
    
    // Compute error
    let val_diff = xx.value - ps.value;
    let x_delta = -val_diff / xx.derivative;
    let error = x_delta.abs();
    println!("\nError calculation:");
    println!("  val_diff = {:.6}", val_diff);
    println!("  x_delta = {:.6}", x_delta);
    println!("  error = {:.6}", error);
    
    // Check error threshold
    let error_threshold = best_error.max(max_error);
    println!("  error < error_threshold ({:.6} < {:.6})? {}", 
        error, error_threshold, error < error_threshold);
    
    // Newton-Raphson would give refined_error ≈ 0.000661
    // Check if refined_error < max_error
    let refined_error = 0.000661_f64;
    println!("\nRefined error check:");
    println!("  refined_error.abs() < max_error ({:.6} < {:.6})? {}",
        refined_error.abs(), max_error, refined_error.abs() < max_error);
}

#[test]
fn test_xx_derivative_check() {
    use crate::gen::{generate_all, GenConfig};
    
    let mut config = GenConfig::default();
    config.max_lhs_complexity = 50;
    config.max_rhs_complexity = 50;
    
    let generated = generate_all(&config, 2.5);
    
    let xx = generated.lhs.iter()
        .find(|e| e.expr.to_postfix() == "xx^")
        .expect("xx^");
    
    println!("xx^ derivative: {:.10}", xx.derivative);
    println!("derivative.abs() < 1e-10? {}", xx.derivative.abs() < 1e-10);
    
    // If derivative is small, it goes into the special path
    // Otherwise normal search path
    if xx.derivative.abs() < 1e-10 {
        println!("Would take DEGENERATE path");
    } else {
        println!("Would take NORMAL search path");
    }
}

#[test]
fn test_manual_xx_match() {
    use crate::gen::{generate_all, GenConfig};
    use ordered_float::OrderedFloat;
    
    let mut config = GenConfig::default();
    config.max_lhs_complexity = 50;
    config.max_rhs_complexity = 50;
    
    let target = 2.5;
    let generated = generate_all(&config, target);
    
    // Build database manually
    let mut rhs_by_value: std::collections::BTreeMap<OrderedFloat<f64>, Vec<_>> = std::collections::BTreeMap::new();
    for expr in &generated.rhs {
        let key = OrderedFloat(expr.value);
        rhs_by_value.entry(key).or_default().push(expr.clone());
    }
    
    // Get xx^
    let xx = generated.lhs.iter()
        .find(|e| e.expr.to_postfix() == "xx^")
        .expect("xx^");
    
    // Search for matches
    let best_error = 1.0;
    let min_search_radius = 0.5 * xx.derivative.abs();
    let search_radius = (best_error * xx.derivative.abs()).max(min_search_radius);
    
    let low = OrderedFloat(xx.value - search_radius);
    let high = OrderedFloat(xx.value + search_radius);
    
    println!("xx^ value={:.6}, searching [{:.2}, {:.2}]", xx.value, low.0, high.0);
    
    let mut found_ps = false;
    for (key, rhs_list) in rhs_by_value.range(low..=high) {
        for rhs in rhs_list {
            if rhs.expr.to_postfix() == "ps" {
                println!("Found ps at value {:.6}!", key.0);
                found_ps = true;
                
                // Check newton raphson
                if let Some(x) = newton_raphson(&xx.expr, rhs.value, target, 15) {
                    let error = x - target;
                    println!("Newton-Raphson: x={:.10}, error={:.10}", x, error);
                }
            }
        }
    }
    
    if !found_ps {
        println!("ps NOT found in range!");
    }
}

#[test]
fn test_xx_in_find_matches_detailed() {
    use crate::gen::{generate_all, GenConfig};
    use crate::expr::EvaluatedExpr;
    use ordered_float::OrderedFloat;
    
    let mut config = GenConfig::default();
    config.max_lhs_complexity = 50;
    config.max_rhs_complexity = 50;
    
    let target = 2.5;
    let generated = generate_all(&config, target);
    
    // Build database
    let mut db = ExprDatabase::new();
    db.insert_rhs(generated.rhs.clone());
    
    // Get xx^
    let xx = generated.lhs.iter()
        .find(|e| e.expr.to_postfix() == "xx^")
        .cloned()
        .expect("xx^");
    
    println!("Testing find_matches with ONLY xx^ as LHS");
    
    let search_config = SearchConfig {
        target,
        max_matches: 100,
        max_error: 1.0,
        ..Default::default()
    };

    // Call find_matches with only xx^
    let single_lhs = vec![xx.clone()];
    let matches = db.find_matches(&single_lhs, &search_config);
    
    println!("Found {} matches for xx^", matches.len());
    for m in &matches {
        println!("  {} = {} (error={:.6})", 
            m.lhs.expr.to_infix(), m.rhs.expr.to_infix(), m.error);
    }
}

#[test]
fn test_2x_is_generated() {
    use crate::gen::{generate_all, GenConfig};
    
    let mut config = GenConfig::default();
    config.max_lhs_complexity = 50;
    config.max_rhs_complexity = 50;
    
    let generated = generate_all(&config, 2.5);
    
    // Find 2x*
    let twox = generated.lhs.iter()
        .find(|e| e.expr.to_postfix() == "2x*");
    
    if let Some(e) = twox {
        println!("2x*: value={:.10}, deriv={:.10}, complexity={}", 
            e.value, e.derivative, e.expr.complexity());
    } else {
        println!("2x* NOT in LHS!");
    }
    
    // Find 5 in RHS
    let five = generated.rhs.iter()
        .find(|e| e.expr.to_postfix() == "5");
    
    if let Some(e) = five {
        println!("5: value={:.10}, complexity={}", e.value, e.expr.complexity());
    } else {
        println!("5 NOT in RHS!");
    }
}

#[test]
fn test_2x_dedup() {
    use crate::gen::{generate_all, GenConfig};
    
    let mut config = GenConfig::default();
    config.max_lhs_complexity = 50;
    config.max_rhs_complexity = 50;
    
    let generated = generate_all(&config, 2.5);
    
    // All LHS with value exactly 5.0
    let val5: Vec<_> = generated.lhs.iter()
        .filter(|e| (e.value - 5.0).abs() < 1e-9)
        .collect();
    
    println!("LHS with value = 5.0:");
    for e in &val5 {
        println!("  {} (deriv={:.4}, complexity={})", 
            e.expr.to_postfix(), e.derivative, e.expr.complexity());
    }
}

#[test]
fn test_exact_matches() {
    use crate::gen::{generate_all, GenConfig};
    
    let mut config = GenConfig::default();
    config.max_lhs_complexity = 50;
    config.max_rhs_complexity = 50;
    
    let generated = generate_all(&config, 2.5);
    
    let mut db = ExprDatabase::new();
    db.insert_rhs(generated.rhs.clone());
    
    let search_config = SearchConfig {
        target: 2.5,
        max_matches: 1000,
        max_error: 1.0,
        ..Default::default()
    };
    
    // Get all matches (before filtering)
    let matches = db.find_matches(&generated.lhs, &search_config);
    
    // Count exact matches (error < 1e-14)
    let exact: Vec<_> = matches.iter()
        .filter(|m| m.error.abs() < 1e-14)
        .collect();
    
    println!("Total exact matches: {}", exact.len());
    println!("First 10 exact by complexity:");
    let mut sorted_exact: Vec<_> = exact.iter().collect();
    sorted_exact.sort_by_key(|m| m.complexity);
    for m in sorted_exact.iter().take(10) {
        println!("  {} = {} (error={:.2e}, complexity={})", 
            m.lhs.expr.to_postfix(), m.rhs.expr.to_postfix(), 
            m.error, m.complexity);
    }
}

#[test]
fn test_2x_search_trace() {
    use crate::gen::{generate_all, GenConfig};
    use ordered_float::OrderedFloat;
    
    let mut config = GenConfig::default();
    config.max_lhs_complexity = 50;
    config.max_rhs_complexity = 50;
    
    let generated = generate_all(&config, 2.5);
    
    let mut db = ExprDatabase::new();
    db.insert_rhs(generated.rhs.clone());
    
    // Find 2x*
    let twox = generated.lhs.iter()
        .find(|e| e.expr.to_postfix() == "2x*")
        .expect("2x*");
    
    println!("2x*: value={:.10}, deriv={:.10}", twox.value, twox.derivative);
    
    // Check if value filter would skip it
    println!("value.abs() < 1e-4? {} (would skip)", twox.value.abs() < 1e-4);
    println!("derivative.abs() < 1e-10? {} (would take degenerate path)", twox.derivative.abs() < 1e-10);
    
    // Check search range
    let best_error = 1.0;
    let min_search_radius = 0.5 * twox.derivative.abs();
    let search_radius = (best_error * twox.derivative.abs()).max(min_search_radius);
    println!("Search radius: {:.4}", search_radius);
    
    let low = OrderedFloat(twox.value - search_radius);
    let high = OrderedFloat(twox.value + search_radius);
    println!("Range: [{:.4}, {:.4}]", low.0, high.0);
    
    // Find 5 in range
    if 5.0 >= low.0 && 5.0 <= high.0 {
        println!("5 is in search range!");
    } else {
        println!("5 is NOT in search range!");
    }

    // Check if 5 is in database
    let exprs_at_5: Vec<_> = db.range(4.999, 5.001).iter().collect();
    if !exprs_at_5.is_empty() {
        println!("Found {} expressions at value ~5.0:", exprs_at_5.len());
        for e in &exprs_at_5 {
            println!("  {}", e.expr.to_postfix());
        }
    } else {
        println!("No expressions at value 5.0 in database!");
    }
}

#[test]
fn test_2x_newton() {
    use crate::gen::{generate_all, GenConfig};
    
    let mut config = GenConfig::default();
    config.max_lhs_complexity = 50;
    config.max_rhs_complexity = 50;
    
    let generated = generate_all(&config, 2.5);
    
    // Find 2x*
    let twox = generated.lhs.iter()
        .find(|e| e.expr.to_postfix() == "2x*")
        .expect("2x*");
    
    // Try Newton-Raphson
    if let Some(x) = newton_raphson(&twox.expr, 5.0, 2.5, 15) {
        let error = x - 2.5;
        println!("Newton-Raphson converged: x={:.15}, error={:.2e}", x, error);
        println!("error.abs() < 1e-14? {}", error.abs() < 1e-14);
        println!("error.abs() < 1.0 (max_error)? {}", error.abs() < 1.0);
    } else {
        println!("Newton-Raphson FAILED!");
    }
}

#[test]
fn test_2x_only() {
    use crate::gen::{generate_all, GenConfig};
    use crate::expr::EvaluatedExpr;
    
    let mut config = GenConfig::default();
    config.max_lhs_complexity = 50;
    config.max_rhs_complexity = 50;
    
    let generated = generate_all(&config, 2.5);
    
    let mut db = ExprDatabase::new();
    db.insert_rhs(generated.rhs.clone());
    
    // Find 2x*
    let twox = generated.lhs.iter()
        .find(|e| e.expr.to_postfix() == "2x*")
        .cloned()
        .expect("2x*");
    
    let search_config = SearchConfig {
        target: 2.5,
        max_matches: 100,
        max_error: 1.0,
        ..Default::default()
    };

    // Search with only 2x*
    let single_lhs = vec![twox];
    let matches = db.find_matches(&single_lhs, &search_config);
    
    println!("Found {} matches for 2x*", matches.len());
    
    // Check for exact match with 5
    let exact_5 = matches.iter()
        .find(|m| m.rhs.expr.to_postfix() == "5");
    
    if let Some(m) = exact_5 {
        println!("Found 2x* = 5: error={:.2e}, complexity={}", m.error, m.complexity);
    } else {
        println!("2x* = 5 NOT found!");
    }
    
    // Show first 5 matches
    for m in matches.iter().take(5) {
        println!("  {} = {} (error={:.2e})",
            m.lhs.expr.to_infix(), m.rhs.expr.to_infix(), m.error);
    }
}

#[test]
fn test_one_over_x_minus_1() {
    use crate::gen::{generate_all, GenConfig};

    let mut config = GenConfig::default();
    // Level 2 defaults: LHS=43, RHS=36
    config.max_lhs_complexity = 43;
    config.max_rhs_complexity = 36;

    let generated = generate_all(&config, 2.5);

    // Find 1/(x-1) = x1-r
    println!("Looking for 1/(x-1)...");
    let lhs = generated.lhs.iter()
        .find(|e| e.expr.to_postfix() == "x1-r");

    if let Some(e) = lhs {
        println!("Found x1-r: value={:.10}, deriv={:.10}", e.value, e.derivative);
    } else {
        println!("x1-r NOT in LHS!");

        // Check for 1/(x-1) in other forms
        let with_recip: Vec<_> = generated.lhs.iter()
            .filter(|e| {
                let inf = e.expr.to_infix();
                inf.contains("x") && inf.contains("1/")
            })
            .take(10)
            .collect();

        println!("\nLHS with 1/(...x...):");
        for e in &with_recip {
            println!("  {} = {} (val={:.4})", e.expr.to_postfix(), e.expr.to_infix(), e.value);
        }
    }

    // Find 1-1/3 = 2/3
    let two_thirds = 2.0 / 3.0;
    println!("\nLooking for RHS near 2/3...");
    let rhs: Vec<_> = generated.rhs.iter()
        .filter(|e| (e.value - two_thirds).abs() < 0.001)
        .take(5)
        .collect();

    println!("RHS near 2/3:");
    for e in &rhs {
        println!("  {} = {} (val={:.10})", e.expr.to_postfix(), e.expr.to_infix(), e.value);
    }

    // Now check if the match 1/(x-1) = 2/3 is found
    let mut db = ExprDatabase::new();
    db.insert_rhs(generated.rhs.clone());

    let lhs_expr = generated.lhs.iter()
        .find(|e| e.expr.to_postfix() == "x1-r")
        .cloned()
        .unwrap();

    let search_config = SearchConfig {
        target: 2.5,
        max_matches: 100,
        max_error: 1.0,
        ..Default::default()
    };

    let single_lhs = vec![lhs_expr];
    let matches = db.find_matches(&single_lhs, &search_config);

    println!("\nMatches for 1/(x-1):");
    for m in matches.iter().take(10) {
        println!("  1/(x-1) = {} (error={:.2e}, complexity={})",
            m.rhs.expr.to_infix(), m.error, m.complexity);
    }

    // Check for exact match with 2/3
    let exact = matches.iter()
        .find(|m| m.rhs.expr.to_postfix() == "23/");
    if let Some(m) = exact {
        println!("\nFound 1/(x-1) = 2/3: error={:.2e}", m.error);
    } else {
        println!("\n1/(x-1) = 2/3 NOT found!");
    }
}

#[test]
fn test_cospi_1_over_x() {
    use crate::gen::{generate_all, GenConfig};
    use crate::eval::evaluate;

    let mut config = GenConfig::default();
    config.max_lhs_complexity = 43;
    config.max_rhs_complexity = 36;

    let generated = generate_all(&config, 2.5);

    // Find cospi(1/x) = xrC
    println!("Looking for cospi(1/x)...");
    let lhs = generated.lhs.iter()
        .find(|e| e.expr.to_postfix() == "xrC");

    if let Some(e) = lhs {
        println!("Found xrC: value={:.10}, deriv={:.10}", e.value, e.derivative);
    } else {
        println!("xrC NOT in LHS!");
    }

    // Find 1/pi = pr
    println!("\nLooking for 1/pi...");
    let rhs = generated.rhs.iter()
        .find(|e| e.expr.to_postfix() == "pr");

    if let Some(e) = rhs {
        println!("Found pr: value={:.10}", e.value);
    } else {
        println!("pr NOT in RHS!");
        // Check what's near 1/pi
        let one_over_pi = 1.0 / std::f64::consts::PI;
        let nearby: Vec<_> = generated.rhs.iter()
            .filter(|e| (e.value - one_over_pi).abs() < 0.05)
            .take(5)
            .collect();
        println!("RHS near 1/pi ({:.6}):", one_over_pi);
        for e in &nearby {
            println!("  {} = {} (val={:.6})", e.expr.to_postfix(), e.expr.to_infix(), e.value);
        }
    }

    // Calculate what the match would be
    let cospi_1_over_x = (std::f64::consts::PI / 2.5).cos();
    println!("\ncospi(1/2.5) = {:.10}", cospi_1_over_x);
    println!("1/pi = {:.10}", 1.0 / std::f64::consts::PI);

    // Now search for matches
    let mut db = ExprDatabase::new();
    db.insert_rhs(generated.rhs.clone());

    let lhs_expr = generated.lhs.iter()
        .find(|e| e.expr.to_postfix() == "xrC")
        .cloned()
        .unwrap();

    let search_config = SearchConfig {
        target: 2.5,
        max_matches: 100,
        max_error: 1.0,
        ..Default::default()
    };

    let single_lhs = vec![lhs_expr];
    let matches = db.find_matches(&single_lhs, &search_config);

    println!("\nMatches for cospi(1/x):");
    for m in matches.iter().take(10) {
        println!("  cospi(1/x) = {} (error={:.2e}, complexity={})",
            m.rhs.expr.to_infix(), m.error, m.complexity);
    }

    // Check for match with 1/pi
    let one_over_pi = matches.iter()
        .find(|m| m.rhs.expr.to_postfix() == "pr");
    if let Some(m) = one_over_pi {
        println!("\nFound cospi(1/x) = 1/pi: error={:.6}", m.error);
    } else {
        println!("\ncospi(1/x) = 1/pi NOT found in matches!");
    }
}

#[test]
fn test_debug_1_over_x_minus_1() {
    use crate::gen::{generate_all, GenConfig};
    use ordered_float::OrderedFloat;

    let mut config = GenConfig::default();
    config.max_lhs_complexity = 43;
    config.max_rhs_complexity = 36;

    let generated = generate_all(&config, 2.5);

    // Check if 23/ is in RHS
    println!("RHS count: {}", generated.rhs.len());
    let has_23 = generated.rhs.iter().find(|e| e.expr.to_postfix() == "23/");
    if let Some(e) = has_23 {
        println!("23/ in RHS: value={:.10}, complexity={}", e.value, e.expr.complexity());
    } else {
        println!("23/ NOT in RHS!");
    }

    // Check if x1-r is in LHS
    let has_x1r = generated.lhs.iter().find(|e| e.expr.to_postfix() == "x1-r");
    if let Some(e) = has_x1r {
        println!("x1-r in LHS: value={:.10}, complexity={}", e.value, e.expr.complexity());
    } else {
        println!("x1-r NOT in LHS!");
    }

    // Build database and check range search
    let mut db = ExprDatabase::new();
    db.insert_rhs(generated.rhs.clone());

    let two_thirds = 2.0 / 3.0;
    let search_radius = 0.5;
    let low = two_thirds - search_radius;
    let high = two_thirds + search_radius;

    println!("\nSearching RHS in range [{:.4}, {:.4}]:", low, high);
    let in_range: Vec<_> = db.range(low, high).iter().take(100).collect();
    println!("Found {} RHS in range", in_range.len());

    // Look specifically for 23/
    let nearby_23: Vec<_> = db.range(two_thirds - 0.0001, two_thirds + 0.0001).iter().collect();
    if !nearby_23.is_empty() {
        println!("Found {} expressions near 2/3 value", nearby_23.len());
        for e in &nearby_23 {
            println!("  {}", e.expr.to_postfix());
        }
    } else {
        println!("No RHS at exact value {:.10}", two_thirds);
    }

    // Now actually do the find_matches
    let lhs_expr = generated.lhs.iter()
        .find(|e| e.expr.to_postfix() == "x1-r")
        .cloned()
        .unwrap();

    println!("\nLHS x1-r: value={:.10}, deriv={:.10}", lhs_expr.value, lhs_expr.derivative);

    // Check the LHS filter conditions
    println!("value.abs() < 1e-4? {} (would skip)", lhs_expr.value.abs() < 1e-4);
    println!("deriv.abs() < 1e-10? {} (would take degenerate path)", lhs_expr.derivative.abs() < 1e-10);

    let search_config = SearchConfig {
        target: 2.5,
        max_matches: 100,
        max_error: 1.0,
        ..Default::default()
    };

    let single_lhs = vec![lhs_expr.clone()];
    let matches = db.find_matches(&single_lhs, &search_config);

    println!("\nFound {} matches for x1-r", matches.len());
    for m in matches.iter().take(10) {
        println!("  1/(x-1) = {} (error={:.2e}, complexity={})",
            m.rhs.expr.to_infix(), m.error, m.complexity);
    }

    // Check for 23/
    let has_23 = matches.iter().find(|m| m.rhs.expr.to_postfix() == "23/");
    if let Some(m) = has_23 {
        println!("\nFound 1/(x-1) = 2/3: error={:.2e}", m.error);
    } else {
        println!("\n1/(x-1) = 2/3 NOT in matches!");
    }
}

#[test]
fn test_exact_match_threshold() {
    use crate::gen::{generate_all, GenConfig};
    
    let mut config = GenConfig::default();
    config.max_lhs_complexity = 43;
    config.max_rhs_complexity = 36;
    
    let generated = generate_all(&config, 2.5);
    
    // Find all exact matches (error < 1e-14)
    let mut db = ExprDatabase::new();
    db.insert_rhs(generated.rhs.clone());
    
    let search_config = SearchConfig {
        target: 2.5,
        max_matches: 100,
        max_error: 1.0,
        ..Default::default()
    };

    let matches = db.find_matches(&generated.lhs, &search_config);

    // Count and list exact matches
    let exact_matches: Vec<_> = matches.iter()
        .filter(|m| m.error.abs() < 1e-14)
        .collect();
    
    println!("Found {} exact matches:", exact_matches.len());
    for m in &exact_matches {
        println!("  {} = {} (error={:.2e}, complexity={})", 
            m.lhs.expr.to_infix(), m.rhs.expr.to_infix(), 
            m.error, m.complexity);
    }
    
    // Check specifically for 1/(x-1) = 2/3
    let one_over = exact_matches.iter()
        .find(|m| m.lhs.expr.to_postfix() == "x1-r");
    if let Some(m) = one_over {
        println!("\n1/(x-1) = {} error = {:.20e}", m.rhs.expr.to_infix(), m.error);
    }
}

#[test]
fn test_x1sr_in_generated() {
    use crate::gen::{generate_all, GenConfig};
    
    let mut config = GenConfig::default();
    config.max_lhs_complexity = 43;
    config.max_rhs_complexity = 36;
    
    let generated = generate_all(&config, 2.5);
    
    // Find x1-r in LHS
    let x1sr = generated.lhs.iter().find(|e| e.expr.to_postfix() == "x1-r");
    if let Some(e) = x1sr {
        println!("Found x1-r: complexity={}", e.expr.complexity());
        println!("  value = {:.10}", e.value);
        println!("  derivative = {:.10}", e.derivative);
    } else {
        println!("x1-r NOT in generated.lhs!");
        
        // Check for similar expressions
        let similar: Vec<_> = generated.lhs.iter()
            .filter(|e| e.expr.to_postfix().contains("x") && e.expr.to_postfix().contains("r"))
            .take(20)
            .map(|e| (e.expr.to_postfix(), e.expr.complexity()))
            .collect();
        println!("Similar expressions: {:?}", similar);
    }
    
    println!("\nTotal LHS expressions: {}", generated.lhs.len());
    
    // List all LHS with complexity <= 30
    let low_complexity: Vec<_> = generated.lhs.iter()
        .filter(|e| e.expr.complexity() <= 30)
        .map(|e| (e.expr.to_postfix(), e.expr.complexity()))
        .collect();
    println!("LHS with complexity <= 30 ({}):", low_complexity.len());
    for (pf, c) in &low_complexity {
        println!("  {} : {}", pf, c);
    }
}

#[test]
fn test_newton_x1sr() {
    use crate::expr::Expression;
    use crate::symbol::Symbol;

    // x1-r means: push x, push 1, subtract, reciprocal = 1/(x-1)
    let expr = Expression::from_symbols(&[
        Symbol::X,
        Symbol::One,
        Symbol::Sub,
        Symbol::Recip,
    ]);

    println!("Expression: {} = {}", expr.to_postfix(), expr.to_infix());

    // At x=2.5: 1/(2.5-1) = 1/1.5 = 0.6667
    let result = crate::eval::evaluate(&expr, 2.5).unwrap();
    println!("At x=2.5: value={:.10}, deriv={:.10}", result.value, result.derivative);

    // Newton-Raphson to solve 1/(x-1) = 2/3
    // Solution should be x = 2.5 exactly
    let rhs_value = 2.0 / 3.0;
    println!("\nSolving 1/(x-1) = {:.10}", rhs_value);

    let refined = newton_raphson(&expr, rhs_value, 2.5, 15);
    match refined {
        Some(x) => println!("Newton-Raphson: x = {:.15}, error = {:.2e}", x, x - 2.5),
        None => println!("Newton-Raphson FAILED!"),
    }
}

#[test]
fn test_full_search_with_trace() {
    use crate::gen::{generate_all, GenConfig};
    use ordered_float::OrderedFloat;
    
    let mut config = GenConfig::default();
    config.max_lhs_complexity = 43;
    config.max_rhs_complexity = 36;
    
    let generated = generate_all(&config, 2.5);
    
    // Find x1-r in LHS
    let x1sr = generated.lhs.iter().find(|e| e.expr.to_postfix() == "x1-r").unwrap();
    println!("x1-r: value={:.10}, deriv={:.10}", x1sr.value, x1sr.derivative);
    
    // Check if 2/3 is in RHS database
    let mut db = ExprDatabase::new();
    db.insert_rhs(generated.rhs.clone());

    // Check what's in the RHS database around value 0.6667
    let two_thirds = 2.0 / 3.0;
    let low = two_thirds - 0.01;
    let high = two_thirds + 0.01;

    println!("\nRHS in range [{:.4}, {:.4}]:", low, high);
    for rhs in db.range(low, high) {
        println!("  {} = {} (val={:.10})", rhs.expr.to_postfix(), rhs.expr.to_infix(), rhs.value);
    }
    
    // Check if the search would find this
    let search_config = SearchConfig {
        target: 2.5,
        max_matches: 100,
        max_error: 1.0,
        ..Default::default()
    };

    // Search with just x1-r
    let single_lhs = vec![x1sr.clone()];
    let matches = db.find_matches(&single_lhs, &search_config);
    println!("\nMatches for just x1-r: {}", matches.len());
    for m in &matches {
        println!("  {} = {} (error={:.2e})", m.lhs.expr.to_infix(), m.rhs.expr.to_infix(), m.error);
    }
    
    // Now search with ALL LHS expressions and see what happens
    let all_matches = db.find_matches(&generated.lhs, &search_config);
    println!("\nTotal matches from full search: {}", all_matches.len());
    
    // Check if x1-r is in the results
    let x1sr_match = all_matches.iter().find(|m| m.lhs.expr.to_postfix() == "x1-r");
    if let Some(m) = x1sr_match {
        println!("Found x1-r match: {} = {} (error={:.2e})", 
            m.lhs.expr.to_infix(), m.rhs.expr.to_infix(), m.error);
    } else {
        println!("x1-r NOT in full search results!");
    }
    
    // List all exact matches
    let exact_matches: Vec<_> = all_matches.iter()
        .filter(|m| m.error.abs() < 1e-14)
        .collect();
    println!("\nExact matches in full search: {}", exact_matches.len());
    for m in &exact_matches {
        println!("  {} = {} (complexity={})", 
            m.lhs.expr.to_infix(), m.rhs.expr.to_infix(), m.complexity);
    }
}

#[test]
fn test_ries_gem_formula() {
    use crate::expr::Expression;
    use crate::eval::evaluate;
    
    // Try to build: 24 * sqrt(atan2(1, 2)) - 6/e
    // In postfix: 1 2 A q 24 * 6 e / -
    // But we need to check our constants...
    
    // First, can we even do atan2(1, 2)?
    let atan_expr = Expression::parse("12A").unwrap();
    let result = evaluate(&atan_expr, 0.0).unwrap();
    println!("atan2(1, 2) = {:.15}", result.value);
    println!("Expected:    {:.15}", 0.4636476090008061);
    
    // What about sqrt of that?
    let sqrt_atan = Expression::parse("12Aq").unwrap();
    let result2 = evaluate(&sqrt_atan, 0.0).unwrap();
    println!("sqrt(atan2(1,2)) = {:.15}", result2.value);
    
    // Now 24 * that - but we don't have 24 as a constant
    // We'd need to compose it: 24 = 8*3 or 4*6 or 3*8
    // Let's try 8*3 = "83*"
    // So: 12Aq 83* * 6 e / -
    // But we don't have 'e' as a constant separate from E (exp)
    // Actually 'e' is the constant, 'E' is exp()
    
    println!("\nConstants available:");
    println!("1-9, p(pi), e, f(phi), x");
}

#[test]
fn test_full_gem_formula() {
    use crate::expr::Expression;
    use crate::eval::evaluate;
    
    // Formula: 24 * sqrt(atan2(1, 2)) - 6/e
    // We need: 24 = 8*3, and 6 = 6
    // Postfix: 12Aq 83** 6er -
    // Wait, let me think...
    // 1 2 A -> atan2(1,2)
    // q -> sqrt(atan2(1,2))
    // 8 3 * -> 24
    // * -> 24 * sqrt(atan2(1,2))
    // 6 e / -> 6/e  (but '/' is a/b, and 'r' is 1/a)
    // Actually for 6/e we need: 6 e /
    // - -> subtract
    
    // Full: 12Aq83**6e/-
    let expr_str = "12Aq83**6e/-";
    println!("Trying to parse: {}", expr_str);
    
    match Expression::parse(expr_str) {
        Some(expr) => {
            println!("Parsed OK: {} symbols", expr.len());
            println!("Complexity: {}", expr.complexity());
            println!("Postfix: {}", expr.to_postfix());
            println!("Infix: {}", expr.to_infix());
            
            match evaluate(&expr, 0.0) {
                Ok(result) => {
                    let gamma1 = 14.134725141734693790457251983562_f64;
                    println!("\nValue: {:.15}", result.value);
                    println!("γ₁:    {:.15}", gamma1);
                    println!("Error: {:.2e}", result.value - gamma1);
                }
                Err(e) => println!("Eval error: {:?}", e),
            }
        }
        None => println!("Parse failed!"),
    }
}
