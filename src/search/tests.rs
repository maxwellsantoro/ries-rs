use super::*;

#[test]
fn batch_limit_reports_streaming_and_skips_lhs_value_dedup() {
    let config = SearchConfig {
        target: 2.5,
        lhs_value_dedup: true,
        ..Default::default()
    };
    let (_, stats) = search_with_batch_limit(&fast_test_config(), &config, 1);
    let matcher = stats.matcher.expect("search executed");
    assert_eq!(matcher, MatcherPath::Streaming);
    assert!(!matcher.applies_lhs_value_dedup(config.lhs_value_dedup));
}

/// Create a fast test config with limited complexity and operators.
/// max_length: 8 keeps expression count tiny; complexity limits are set
/// high enough that simple 3-symbol expressions (e.g. `2x*` = 32) are
/// included under the new calibrated weights.
fn fast_test_config() -> crate::gen::GenConfig {
    crate::gen::GenConfig {
        max_lhs_complexity: 60,
        max_rhs_complexity: 60,
        max_length: 8,
        constants: vec![
            crate::symbol::Symbol::One,
            crate::symbol::Symbol::Two,
            crate::symbol::Symbol::Three,
            crate::symbol::Symbol::Four,
            crate::symbol::Symbol::Five,
            crate::symbol::Symbol::Pi,
            crate::symbol::Symbol::E,
        ],
        unary_ops: vec![
            crate::symbol::Symbol::Neg,
            crate::symbol::Symbol::Recip,
            crate::symbol::Symbol::Square,
            crate::symbol::Symbol::Sqrt,
        ],
        binary_ops: vec![
            crate::symbol::Symbol::Add,
            crate::symbol::Symbol::Sub,
            crate::symbol::Symbol::Mul,
            crate::symbol::Symbol::Div,
        ],
        rhs_constants: None,
        rhs_unary_ops: None,
        rhs_binary_ops: None,
        symbol_max_counts: std::collections::HashMap::new(),
        rhs_symbol_max_counts: None,
        min_num_type: crate::symbol::NumType::Transcendental,
        generate_lhs: true,
        generate_rhs: true,
        user_constants: Vec::new(),
        user_functions: Vec::new(),
        show_pruned_arith: false,
        symbol_table: std::sync::Arc::new(crate::symbol_table::SymbolTable::new()),
    }
}

#[test]
#[allow(unused_imports)]
fn test_simple_search() {
    use crate::gen::GenConfig;

    // Search for equations matching 2.5
    let config = fast_test_config();
    let matches = search(2.5, &config, 10);

    // Should find 2x = 5
    assert!(!matches.is_empty());

    // Print matches for debugging
    for m in &matches {
        println!("{}", m.display(2.5));
    }
}

#[test]
fn test_newton_raphson_keeps_steep_phi_root() {
    use crate::eval::evaluate;
    use crate::expr::Expression;
    // Ten-digit phi probe; refinement is checked against this value, not GOLDEN_RATIO.
    #[allow(clippy::approx_constant)]
    let target = 1.6180339887_f64;
    let lhs = Expression::parse("fx-rq").unwrap();
    let rhs = Expression::parse("2ps+E").unwrap();
    let l = evaluate(&lhs, target).unwrap();
    let r = evaluate(&rhs, target).unwrap();
    let initial = target + -(l.value - r.value) / l.derivative;
    let got =
        newton_raphson(&lhs, r.value, initial, 15).expect("steep phi match must still refine");
    assert!(
        (got - 1.6180339887008952).abs() < 1e-18,
        "refined root moved: {got}"
    );
    let ctx = crate::eval::EvalContext::default();
    assert!(
        super::newton::refined_match_is_real(&lhs, r.value, got, &ctx),
        "steep phi root must stay (relative residual is about 1e-6)"
    );

    // A steep but genuine near-root. The historical refine lands within an ulp
    // of the target; rejecting it reorders ordinary top-N.
    let target = 0.123_456_789_f64;
    let lhs = Expression::parse("x9vC").unwrap();
    let rhs = Expression::parse("21p-/").unwrap();
    let l = evaluate(&lhs, target).unwrap();
    let r = evaluate(&rhs, target).unwrap();
    let initial = target + -(l.value - r.value) / l.derivative;
    let got = newton_raphson(&lhs, r.value, initial, 15);
    let got = got.expect("x9vC vs 21p-/ should refine");
    let ctx = crate::eval::EvalContext::default();
    assert!(
        super::newton::refined_match_is_real(&lhs, r.value, got, &ctx),
        "steep near-root at 0.123456789 must stay"
    );

    // Identities whose value is exact. A residual gate that only looked at
    // absolute error would be tempted to drop the huge derivatives.
    let ctx = crate::eval::EvalContext::default();
    let exp = Expression::parse("xE").unwrap();
    let exp_rhs = 40.0_f64.exp();
    let exp_root = newton_raphson(&exp, exp_rhs, 40.0, 15).expect("exp(40)");
    assert!((exp_root - 40.0).abs() < 1e-12);
    assert!(super::newton::refined_match_is_real(
        &exp, exp_rhs, exp_root, &ctx
    ));
    let recip = Expression::parse("xr").unwrap();
    let recip_root = newton_raphson(&recip, 1e8, 1e-8, 15).expect("1/x = 1e8");
    assert!((recip_root - 1e-8).abs() < 1e-18);
    assert!(super::newton::refined_match_is_real(
        &recip, 1e8, recip_root, &ctx
    ));
    let exp_sinpi = Expression::parse("xSE").unwrap();
    let one_root = newton_raphson(&exp_sinpi, 1.0, 1e15, 15).expect("e^sinpi = 1");
    assert!(super::newton::refined_match_is_real(
        &exp_sinpi, 1.0, one_root, &ctx
    ));
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
    use crate::eval::evaluate;
    use crate::expr::Expression;
    use crate::gen::generate_all;

    // Test that 2*x is properly generated and evaluated
    let expr = Expression::parse("2x*").unwrap();
    let result = evaluate(&expr, 2.5).unwrap();
    assert!(expr.contains_x(), "2x* should contain x");
    assert!((result.value - 5.0).abs() < 1e-10, "2*2.5 should be 5");

    // Now test if 2x* is generated and matches with 5
    let config = fast_test_config();
    let generated = generate_all(&config, 2.5);

    // Check if 2x* is in LHS
    let has_2x = generated.lhs.iter().any(|e| e.expr.to_postfix() == "2x*");
    println!("LHS contains 2x*: {}", has_2x);

    // Check if 5 is in RHS
    let has_5 = generated.rhs.iter().any(|e| e.expr.to_postfix() == "5");
    println!("RHS contains 5: {}", has_5);

    // Find expressions with value near 5
    let near_5_lhs: Vec<_> = generated
        .lhs
        .iter()
        .filter(|e| (e.value - 5.0).abs() < 0.1)
        .take(5)
        .collect();
    println!("\nLHS expressions with value ≈ 5:");
    for e in &near_5_lhs {
        println!(
            "  {} = {} (value={:.4}, deriv={:.4})",
            e.expr.to_postfix(),
            e.expr.to_infix(),
            e.value,
            e.derivative
        );
    }

    let near_5_rhs: Vec<_> = generated
        .rhs
        .iter()
        .filter(|e| (e.value - 5.0).abs() < 0.1)
        .take(5)
        .collect();
    println!("\nRHS expressions with value ≈ 5:");
    for e in &near_5_rhs {
        println!(
            "  {} = {} (value={:.4})",
            e.expr.to_postfix(),
            e.expr.to_infix(),
            e.value
        );
    }

    assert!(has_2x, "2x* should be generated as LHS");
    assert!(has_5, "5 should be generated as RHS");
}

#[test]
fn test_tiered_database_sorting_with_nan() {
    use crate::expr::{EvaluatedExpr, Expression};

    // Create a database with some values including NaN
    let mut db = TieredExprDatabase::new();

    // Add normal values
    db.insert(EvaluatedExpr {
        expr: Expression::new(),
        value: 1.0,
        derivative: 0.0,
        num_type: crate::symbol::NumType::Rational,
    });

    db.insert(EvaluatedExpr {
        expr: Expression::new(),
        value: 3.0,
        derivative: 0.0,
        num_type: crate::symbol::NumType::Rational,
    });

    db.insert(EvaluatedExpr {
        expr: Expression::new(),
        value: 2.0,
        derivative: 0.0,
        num_type: crate::symbol::NumType::Rational,
    });

    // Add a NaN value - this should be sorted deterministically with total_cmp
    db.insert(EvaluatedExpr {
        expr: Expression::new(),
        value: f64::NAN,
        derivative: 0.0,
        num_type: crate::symbol::NumType::Rational,
    });

    // Finalize sorts using total_cmp which handles NaN deterministically
    db.finalize();

    // The tier should now be sorted deterministically
    // With total_cmp, NaN values are consistently ordered (less than all other values)
    let tier = db.tier(ComplexityTier::Tier0);
    let values: Vec<f64> = tier.iter().map(|e| e.value).collect();

    // total_cmp places NaN at the beginning, then sorted values
    // The exact order depends on total_cmp's implementation
    // The key point is that sorting should be deterministic (no panic)
    assert_eq!(tier.len(), 4, "All 4 expressions should be in tier 0");

    // Verify sorting is stable and doesn't panic
    // The actual order with total_cmp: NaN < -inf < ... < -0.0 < 0.0 < ... < inf
    // But since we only have positive numbers + NaN, NaN comes first
    assert!(
        values[0].is_nan() || values[0] < values[1],
        "Values should be sorted"
    );
}

// =============================================================================
// EXPENSIVE DEBUG TESTS
// These tests use high complexity limits and all operators.
// Run with `cargo test -- --ignored` to include them.
// =============================================================================

#[test]
#[ignore = "expensive debug test - run with --ignored flag"]
fn test_xx_match_directly() {
    use crate::gen::{generate_all, GenConfig};

    let mut config = GenConfig::default();
    config.max_lhs_complexity = 50;
    config.max_rhs_complexity = 50;

    let generated = generate_all(&config, 2.5);

    // Find x^x LHS
    let xx = generated
        .lhs
        .iter()
        .find(|e| e.expr.to_postfix() == "xx^")
        .expect("xx^ should exist");

    println!("xx^ LHS: value={:.6}, deriv={:.6}", xx.value, xx.derivative);

    // Find pi^2 RHS
    let pi_sq = generated
        .rhs
        .iter()
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
        if df.abs() < DEGENERATE_DERIVATIVE {
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
#[ignore = "expensive debug test - run with --ignored flag"]
fn test_search_finds_xx() {
    use crate::gen::{generate_all, GenConfig};

    let mut config = GenConfig::default();
    config.max_lhs_complexity = 50;
    config.max_rhs_complexity = 50;

    let target = 2.5;
    let generated = generate_all(&config, target);

    println!(
        "Generated {} LHS and {} RHS",
        generated.lhs.len(),
        generated.rhs.len()
    );

    // Build database
    let mut db = ExprDatabase::new();
    db.insert_rhs(generated.rhs.clone());

    // Find x^x
    let xx = generated
        .lhs
        .iter()
        .find(|e| e.expr.to_postfix() == "xx^")
        .expect("xx^ should exist");

    println!(
        "\nLooking for match for xx^: value={:.6}, deriv={:.6}",
        xx.value, xx.derivative
    );

    // Check what's in the RHS database near 9.88
    let search_radius = 1.0 * xx.derivative.abs(); // Allow error up to 1.0
    println!("Search radius: {:.2}", search_radius);
    println!(
        "Search range: [{:.2}, {:.2}]",
        xx.value - search_radius,
        xx.value + search_radius
    );

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
    let xx_match = matches.iter().find(|m| m.lhs.expr.to_postfix() == "xx^");

    if let Some(m) = xx_match {
        println!(
            "Found x^x match: {} = {} (error={:.6})",
            m.lhs.expr.to_infix(),
            m.rhs.expr.to_infix(),
            m.error
        );
    } else {
        println!("x^x match NOT found!");
    }
}

#[test]
#[ignore = "expensive debug test - run with --ignored flag"]
fn test_find_ps_in_rhs_db() {
    use crate::gen::{generate_all, GenConfig};

    let mut config = GenConfig::default();
    config.max_lhs_complexity = 50;
    config.max_rhs_complexity = 50;

    let generated = generate_all(&config, 2.5);

    // Check if ps is in RHS
    let ps = generated.rhs.iter().find(|e| e.expr.to_postfix() == "ps");

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
#[ignore = "expensive debug test - run with --ignored flag"]
#[allow(unused_imports)]
fn test_xx_match_step_by_step() {
    use crate::eval::evaluate;
    use crate::gen::{generate_all, GenConfig};

    let mut config = GenConfig::default();
    config.max_lhs_complexity = 50;
    config.max_rhs_complexity = 50;

    let target = 2.5;
    let generated = generate_all(&config, target);

    // Get xx^ and ps
    let xx = generated
        .lhs
        .iter()
        .find(|e| e.expr.to_postfix() == "xx^")
        .expect("xx^");
    let ps = generated
        .rhs
        .iter()
        .find(|e| e.expr.to_postfix() == "ps")
        .expect("ps");

    println!(
        "xx^: value={:.6}, deriv={:.6}, complexity={}",
        xx.value,
        xx.derivative,
        xx.expr.complexity()
    );
    println!(
        "ps: value={:.6}, complexity={}",
        ps.value,
        ps.expr.complexity()
    );

    // Simulate the search logic
    let best_error = 1e-12; // After finding exact matches
    let max_error = 1.0;

    // Calculate search radius
    let min_search_radius = 0.5 * xx.derivative.abs();
    let search_radius = (best_error * xx.derivative.abs()).max(min_search_radius);
    println!(
        "\nSearch radius: {:.6} (min={:.6})",
        search_radius, min_search_radius
    );

    // Check if ps is in range
    let low = xx.value - search_radius;
    let high = xx.value + search_radius;
    println!("Range: [{:.6}, {:.6}]", low, high);
    println!(
        "ps.value={:.6} in range? {}",
        ps.value,
        ps.value >= low && ps.value <= high
    );

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
    println!(
        "  error < error_threshold ({:.6} < {:.6})? {}",
        error,
        error_threshold,
        error < error_threshold
    );

    // Newton-Raphson would give refined_error ≈ 0.000661
    // Check if refined_error < max_error
    let refined_error = 0.000661_f64;
    println!("\nRefined error check:");
    println!(
        "  refined_error.abs() < max_error ({:.6} < {:.6})? {}",
        refined_error.abs(),
        max_error,
        refined_error.abs() < max_error
    );
}

#[test]
#[ignore = "expensive debug test - run with --ignored flag"]
fn test_xx_derivative_check() {
    use crate::gen::{generate_all, GenConfig};

    let mut config = GenConfig::default();
    config.max_lhs_complexity = 50;
    config.max_rhs_complexity = 50;

    let generated = generate_all(&config, 2.5);

    let xx = generated
        .lhs
        .iter()
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
#[ignore = "expensive debug test - run with --ignored flag"]
fn test_manual_xx_match() {
    use crate::gen::{generate_all, GenConfig};
    use ordered_float::OrderedFloat;

    let mut config = GenConfig::default();
    config.max_lhs_complexity = 50;
    config.max_rhs_complexity = 50;

    let target = 2.5;
    let generated = generate_all(&config, target);

    // Build database manually
    let mut rhs_by_value: std::collections::BTreeMap<OrderedFloat<f64>, Vec<_>> =
        std::collections::BTreeMap::new();
    for expr in &generated.rhs {
        let key = OrderedFloat(expr.value);
        rhs_by_value.entry(key).or_default().push(expr.clone());
    }

    // Get xx^
    let xx = generated
        .lhs
        .iter()
        .find(|e| e.expr.to_postfix() == "xx^")
        .expect("xx^");

    // Search for matches
    let best_error = 1.0;
    let min_search_radius = 0.5 * xx.derivative.abs();
    let search_radius = (best_error * xx.derivative.abs()).max(min_search_radius);

    let low = OrderedFloat(xx.value - search_radius);
    let high = OrderedFloat(xx.value + search_radius);

    println!(
        "xx^ value={:.6}, searching [{:.2}, {:.2}]",
        xx.value, low.0, high.0
    );

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
#[ignore = "expensive debug test - run with --ignored flag"]
#[allow(unused_imports)]
fn test_xx_in_find_matches_detailed() {
    use crate::expr::EvaluatedExpr;
    use crate::gen::{generate_all, GenConfig};
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
    let xx = generated
        .lhs
        .iter()
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
        println!(
            "  {} = {} (error={:.6})",
            m.lhs.expr.to_infix(),
            m.rhs.expr.to_infix(),
            m.error
        );
    }
}

#[test]
#[ignore = "expensive debug test - run with --ignored flag"]
fn test_2x_is_generated() {
    use crate::gen::{generate_all, GenConfig};

    let mut config = GenConfig::default();
    config.max_lhs_complexity = 50;
    config.max_rhs_complexity = 50;

    let generated = generate_all(&config, 2.5);

    // Find 2x*
    let twox = generated.lhs.iter().find(|e| e.expr.to_postfix() == "2x*");

    if let Some(e) = twox {
        println!(
            "2x*: value={:.10}, deriv={:.10}, complexity={}",
            e.value,
            e.derivative,
            e.expr.complexity()
        );
    } else {
        println!("2x* NOT in LHS!");
    }

    // Find 5 in RHS
    let five = generated.rhs.iter().find(|e| e.expr.to_postfix() == "5");

    if let Some(e) = five {
        println!(
            "5: value={:.10}, complexity={}",
            e.value,
            e.expr.complexity()
        );
    } else {
        println!("5 NOT in RHS!");
    }
}

#[test]
#[ignore = "expensive debug test - run with --ignored flag"]
fn test_2x_dedup() {
    use crate::gen::{generate_all, GenConfig};

    let mut config = GenConfig::default();
    config.max_lhs_complexity = 50;
    config.max_rhs_complexity = 50;

    let generated = generate_all(&config, 2.5);

    // All LHS with value exactly 5.0
    let val5: Vec<_> = generated
        .lhs
        .iter()
        .filter(|e| (e.value - 5.0).abs() < 1e-9)
        .collect();

    println!("LHS with value = 5.0:");
    for e in &val5 {
        println!(
            "  {} (deriv={:.4}, complexity={})",
            e.expr.to_postfix(),
            e.derivative,
            e.expr.complexity()
        );
    }
}

#[test]
#[ignore = "expensive debug test - run with --ignored flag"]
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
    let exact: Vec<_> = matches.iter().filter(|m| m.error.abs() < 1e-14).collect();

    println!("Total exact matches: {}", exact.len());
    println!("First 10 exact by complexity:");
    let mut sorted_exact: Vec<_> = exact.iter().collect();
    sorted_exact.sort_by_key(|m| m.complexity);
    for m in sorted_exact.iter().take(10) {
        println!(
            "  {} = {} (error={:.2e}, complexity={})",
            m.lhs.expr.to_postfix(),
            m.rhs.expr.to_postfix(),
            m.error,
            m.complexity
        );
    }
}

#[test]
#[ignore = "expensive debug test - run with --ignored flag"]
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
    let twox = generated
        .lhs
        .iter()
        .find(|e| e.expr.to_postfix() == "2x*")
        .expect("2x*");

    println!(
        "2x*: value={:.10}, deriv={:.10}",
        twox.value, twox.derivative
    );

    // Check if value filter would skip it
    println!(
        "value.abs() < 1e-4? {} (would skip)",
        twox.value.abs() < 1e-4
    );
    println!(
        "derivative.abs() < 1e-10? {} (would take degenerate path)",
        twox.derivative.abs() < 1e-10
    );

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
#[ignore = "expensive debug test - run with --ignored flag"]
fn test_2x_newton() {
    use crate::gen::{generate_all, GenConfig};

    let mut config = GenConfig::default();
    config.max_lhs_complexity = 50;
    config.max_rhs_complexity = 50;

    let generated = generate_all(&config, 2.5);

    // Find 2x*
    let twox = generated
        .lhs
        .iter()
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
#[ignore = "expensive debug test - run with --ignored flag"]
#[allow(unused_imports)]
fn test_2x_only() {
    use crate::expr::EvaluatedExpr;
    use crate::gen::{generate_all, GenConfig};

    let mut config = GenConfig::default();
    config.max_lhs_complexity = 50;
    config.max_rhs_complexity = 50;

    let generated = generate_all(&config, 2.5);

    let mut db = ExprDatabase::new();
    db.insert_rhs(generated.rhs.clone());

    // Find 2x*
    let twox = generated
        .lhs
        .iter()
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
    let exact_5 = matches.iter().find(|m| m.rhs.expr.to_postfix() == "5");

    if let Some(m) = exact_5 {
        println!(
            "Found 2x* = 5: error={:.2e}, complexity={}",
            m.error, m.complexity
        );
    } else {
        println!("2x* = 5 NOT found!");
    }

    // Show first 5 matches
    for m in matches.iter().take(5) {
        println!(
            "  {} = {} (error={:.2e})",
            m.lhs.expr.to_infix(),
            m.rhs.expr.to_infix(),
            m.error
        );
    }
}

#[test]
#[ignore = "expensive debug test - run with --ignored flag"]
fn test_one_over_x_minus_1() {
    use crate::gen::{generate_all, GenConfig};

    let mut config = GenConfig::default();
    // Level 2 defaults: LHS=43, RHS=36
    config.max_lhs_complexity = 43;
    config.max_rhs_complexity = 36;

    let generated = generate_all(&config, 2.5);

    // Find 1/(x-1) = x1-r
    println!("Looking for 1/(x-1)...");
    let lhs = generated.lhs.iter().find(|e| e.expr.to_postfix() == "x1-r");

    if let Some(e) = lhs {
        println!(
            "Found x1-r: value={:.10}, deriv={:.10}",
            e.value, e.derivative
        );
    } else {
        println!("x1-r NOT in LHS!");

        // Check for 1/(x-1) in other forms
        let with_recip: Vec<_> = generated
            .lhs
            .iter()
            .filter(|e| {
                let inf = e.expr.to_infix();
                inf.contains("x") && inf.contains("1/")
            })
            .take(10)
            .collect();

        println!("\nLHS with 1/(...x...):");
        for e in &with_recip {
            println!(
                "  {} = {} (val={:.4})",
                e.expr.to_postfix(),
                e.expr.to_infix(),
                e.value
            );
        }
    }

    // Find 1-1/3 = 2/3
    let two_thirds = 2.0 / 3.0;
    println!("\nLooking for RHS near 2/3...");
    let rhs: Vec<_> = generated
        .rhs
        .iter()
        .filter(|e| (e.value - two_thirds).abs() < 0.001)
        .take(5)
        .collect();

    println!("RHS near 2/3:");
    for e in &rhs {
        println!(
            "  {} = {} (val={:.10})",
            e.expr.to_postfix(),
            e.expr.to_infix(),
            e.value
        );
    }

    // Now check if the match 1/(x-1) = 2/3 is found
    let mut db = ExprDatabase::new();
    db.insert_rhs(generated.rhs.clone());

    let lhs_expr = generated
        .lhs
        .iter()
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
        println!(
            "  1/(x-1) = {} (error={:.2e}, complexity={})",
            m.rhs.expr.to_infix(),
            m.error,
            m.complexity
        );
    }

    // Check for exact match with 2/3
    let exact = matches.iter().find(|m| m.rhs.expr.to_postfix() == "23/");
    if let Some(m) = exact {
        println!("\nFound 1/(x-1) = 2/3: error={:.2e}", m.error);
    } else {
        println!("\n1/(x-1) = 2/3 NOT found!");
    }
}

#[test]
#[ignore = "expensive debug test - run with --ignored flag"]
#[allow(unused_imports)]
fn test_cospi_1_over_x() {
    use crate::eval::evaluate;
    use crate::gen::{generate_all, GenConfig};

    let mut config = GenConfig::default();
    config.max_lhs_complexity = 43;
    config.max_rhs_complexity = 36;

    let generated = generate_all(&config, 2.5);

    // Find cospi(1/x) = xrC
    println!("Looking for cospi(1/x)...");
    let lhs = generated.lhs.iter().find(|e| e.expr.to_postfix() == "xrC");

    if let Some(e) = lhs {
        println!(
            "Found xrC: value={:.10}, deriv={:.10}",
            e.value, e.derivative
        );
    } else {
        println!("xrC NOT in LHS!");
    }

    // Find 1/pi = pr
    println!("\nLooking for 1/pi...");
    let rhs = generated.rhs.iter().find(|e| e.expr.to_postfix() == "pr");

    if let Some(e) = rhs {
        println!("Found pr: value={:.10}", e.value);
    } else {
        println!("pr NOT in RHS!");
        // Check what's near 1/pi
        let one_over_pi = 1.0 / std::f64::consts::PI;
        let nearby: Vec<_> = generated
            .rhs
            .iter()
            .filter(|e| (e.value - one_over_pi).abs() < 0.05)
            .take(5)
            .collect();
        println!("RHS near 1/pi ({:.6}):", one_over_pi);
        for e in &nearby {
            println!(
                "  {} = {} (val={:.6})",
                e.expr.to_postfix(),
                e.expr.to_infix(),
                e.value
            );
        }
    }

    // Calculate what the match would be
    let cospi_1_over_x = (std::f64::consts::PI / 2.5).cos();
    println!("\ncospi(1/2.5) = {:.10}", cospi_1_over_x);
    println!("1/pi = {:.10}", 1.0 / std::f64::consts::PI);

    // Now search for matches
    let mut db = ExprDatabase::new();
    db.insert_rhs(generated.rhs.clone());

    let lhs_expr = generated
        .lhs
        .iter()
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
        println!(
            "  cospi(1/x) = {} (error={:.2e}, complexity={})",
            m.rhs.expr.to_infix(),
            m.error,
            m.complexity
        );
    }

    // Check for match with 1/pi
    let one_over_pi = matches.iter().find(|m| m.rhs.expr.to_postfix() == "pr");
    if let Some(m) = one_over_pi {
        println!("\nFound cospi(1/x) = 1/pi: error={:.6}", m.error);
    } else {
        println!("\ncospi(1/x) = 1/pi NOT found in matches!");
    }
}

#[test]
#[ignore = "expensive debug test - run with --ignored flag"]
#[allow(unused_imports)]
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
        println!(
            "23/ in RHS: value={:.10}, complexity={}",
            e.value,
            e.expr.complexity()
        );
    } else {
        println!("23/ NOT in RHS!");
    }

    // Check if x1-r is in LHS
    let has_x1r = generated.lhs.iter().find(|e| e.expr.to_postfix() == "x1-r");
    if let Some(e) = has_x1r {
        println!(
            "x1-r in LHS: value={:.10}, complexity={}",
            e.value,
            e.expr.complexity()
        );
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
    let nearby_23: Vec<_> = db
        .range(two_thirds - 0.0001, two_thirds + 0.0001)
        .iter()
        .collect();
    if !nearby_23.is_empty() {
        println!("Found {} expressions near 2/3 value", nearby_23.len());
        for e in &nearby_23 {
            println!("  {}", e.expr.to_postfix());
        }
    } else {
        println!("No RHS at exact value {:.10}", two_thirds);
    }

    // Now actually do the find_matches
    let lhs_expr = generated
        .lhs
        .iter()
        .find(|e| e.expr.to_postfix() == "x1-r")
        .cloned()
        .unwrap();

    println!(
        "\nLHS x1-r: value={:.10}, deriv={:.10}",
        lhs_expr.value, lhs_expr.derivative
    );

    // Check the LHS filter conditions
    println!(
        "value.abs() < 1e-4? {} (would skip)",
        lhs_expr.value.abs() < 1e-4
    );
    println!(
        "deriv.abs() < 1e-10? {} (would take degenerate path)",
        lhs_expr.derivative.abs() < 1e-10
    );

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
        println!(
            "  1/(x-1) = {} (error={:.2e}, complexity={})",
            m.rhs.expr.to_infix(),
            m.error,
            m.complexity
        );
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
#[ignore = "expensive debug test - run with --ignored flag"]
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
    let exact_matches: Vec<_> = matches.iter().filter(|m| m.error.abs() < 1e-14).collect();

    println!("Found {} exact matches:", exact_matches.len());
    for m in &exact_matches {
        println!(
            "  {} = {} (error={:.2e}, complexity={})",
            m.lhs.expr.to_infix(),
            m.rhs.expr.to_infix(),
            m.error,
            m.complexity
        );
    }

    // Check specifically for 1/(x-1) = 2/3
    let one_over = exact_matches
        .iter()
        .find(|m| m.lhs.expr.to_postfix() == "x1-r");
    if let Some(m) = one_over {
        println!(
            "\n1/(x-1) = {} error = {:.20e}",
            m.rhs.expr.to_infix(),
            m.error
        );
    }
}

#[test]
#[ignore = "expensive debug test - run with --ignored flag"]
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
        let similar: Vec<_> = generated
            .lhs
            .iter()
            .filter(|e| e.expr.to_postfix().contains("x") && e.expr.to_postfix().contains("r"))
            .take(20)
            .map(|e| (e.expr.to_postfix(), e.expr.complexity()))
            .collect();
        println!("Similar expressions: {:?}", similar);
    }

    println!("\nTotal LHS expressions: {}", generated.lhs.len());

    // List all LHS with complexity <= 30
    let low_complexity: Vec<_> = generated
        .lhs
        .iter()
        .filter(|e| e.expr.complexity() <= 30)
        .map(|e| (e.expr.to_postfix(), e.expr.complexity()))
        .collect();
    println!("LHS with complexity <= 30 ({}):", low_complexity.len());
    for (pf, c) in &low_complexity {
        println!("  {} : {}", pf, c);
    }
}

#[test]
#[ignore = "expensive debug test - run with --ignored flag"]
fn test_newton_x1sr() {
    use crate::expr::Expression;
    use crate::symbol::Symbol;

    // x1-r means: push x, push 1, subtract, reciprocal = 1/(x-1)
    let expr = Expression::from_symbols(&[Symbol::X, Symbol::One, Symbol::Sub, Symbol::Recip]);

    println!("Expression: {} = {}", expr.to_postfix(), expr.to_infix());

    // At x=2.5: 1/(2.5-1) = 1/1.5 = 0.6667
    let result = crate::eval::evaluate(&expr, 2.5).unwrap();
    println!(
        "At x=2.5: value={:.10}, deriv={:.10}",
        result.value, result.derivative
    );

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
#[ignore = "expensive debug test - run with --ignored flag"]
#[allow(unused_imports)]
fn test_full_search_with_trace() {
    use crate::gen::{generate_all, GenConfig};
    use ordered_float::OrderedFloat;

    let mut config = GenConfig::default();
    config.max_lhs_complexity = 43;
    config.max_rhs_complexity = 36;

    let generated = generate_all(&config, 2.5);

    // Find x1-r in LHS
    let x1sr = generated
        .lhs
        .iter()
        .find(|e| e.expr.to_postfix() == "x1-r")
        .unwrap();
    println!(
        "x1-r: value={:.10}, deriv={:.10}",
        x1sr.value, x1sr.derivative
    );

    // Check if 2/3 is in RHS database
    let mut db = ExprDatabase::new();
    db.insert_rhs(generated.rhs.clone());

    // Check what's in the RHS database around value 0.6667
    let two_thirds = 2.0 / 3.0;
    let low = two_thirds - 0.01;
    let high = two_thirds + 0.01;

    println!("\nRHS in range [{:.4}, {:.4}]:", low, high);
    for rhs in db.range(low, high) {
        println!(
            "  {} = {} (val={:.10})",
            rhs.expr.to_postfix(),
            rhs.expr.to_infix(),
            rhs.value
        );
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
        println!(
            "  {} = {} (error={:.2e})",
            m.lhs.expr.to_infix(),
            m.rhs.expr.to_infix(),
            m.error
        );
    }

    // Now search with ALL LHS expressions and see what happens
    let all_matches = db.find_matches(&generated.lhs, &search_config);
    println!("\nTotal matches from full search: {}", all_matches.len());

    // Check if x1-r is in the results
    let x1sr_match = all_matches
        .iter()
        .find(|m| m.lhs.expr.to_postfix() == "x1-r");
    if let Some(m) = x1sr_match {
        println!(
            "Found x1-r match: {} = {} (error={:.2e})",
            m.lhs.expr.to_infix(),
            m.rhs.expr.to_infix(),
            m.error
        );
    } else {
        println!("x1-r NOT in full search results!");
    }

    // List all exact matches
    let exact_matches: Vec<_> = all_matches
        .iter()
        .filter(|m| m.error.abs() < 1e-14)
        .collect();
    println!("\nExact matches in full search: {}", exact_matches.len());
    for m in &exact_matches {
        println!(
            "  {} = {} (complexity={})",
            m.lhs.expr.to_infix(),
            m.rhs.expr.to_infix(),
            m.complexity
        );
    }
}

#[test]
#[ignore = "expensive debug test - run with --ignored flag"]
fn test_ries_gem_formula() {
    use crate::eval::evaluate;
    use crate::expr::Expression;

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
#[ignore = "expensive debug test - run with --ignored flag"]
fn test_full_gem_formula() {
    use crate::eval::evaluate;
    use crate::expr::Expression;

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
                    let gamma1 = 14.134_725_141_734_694_f64;
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

#[test]
fn test_should_skip_degenerate_lhs_skips_constant_power() {
    use crate::eval::{evaluate_fast_with_context, EvalContext};
    use crate::expr::{EvaluatedExpr, Expression};
    use crate::symbol::NumType;

    // 1^x = 1 with derivative 0 everywhere.
    let expr = Expression::parse("1x^").expect("valid postfix");
    let ctx = EvalContext::default();
    let at_target = evaluate_fast_with_context(&expr, 2.5, &ctx).expect("eval at target");
    let lhs = EvaluatedExpr::new(
        expr,
        at_target.value,
        at_target.derivative,
        NumType::Transcendental,
    );

    assert!(
        super::should_skip_degenerate_lhs(&lhs, 2.5, &ctx),
        "1^x should be treated as degenerate"
    );
}

#[test]
fn test_should_skip_degenerate_lhs_skips_when_probe_fails() {
    use crate::eval::EvalContext;
    use crate::expr::{EvaluatedExpr, Expression};
    use crate::symbol::{NumType, Symbol};

    // sqrt(x - 8) is undefined at both target 2.5 and the probe point 2.5 + e.
    // Force the degenerate branch with a near-zero derivative recorded at generation time.
    let expr = Expression::from_symbols(&[Symbol::X, Symbol::Eight, Symbol::Sub, Symbol::Sqrt]);
    let ctx = EvalContext::default();
    let lhs = EvaluatedExpr::new(expr, 0.0, 0.0, NumType::Transcendental);

    assert!(
        super::should_skip_degenerate_lhs(&lhs, 2.5, &ctx),
        "probe failure should skip the LHS instead of falling through to value matching"
    );
}

#[test]
fn test_large_targets_keep_cospi_identity_and_still_drop_constants() {
    use crate::eval::{evaluate_fast_with_context, EvalContext};
    use crate::expr::{EvaluatedExpr, Expression};
    use crate::symbol::{NumType, Symbol};

    let ctx = EvalContext::default();
    let evaluated_at = |postfix: &str, target: f64| {
        let expr = Expression::parse(postfix).unwrap_or_else(|| panic!("parse {postfix}"));
        let at = evaluate_fast_with_context(&expr, target, &ctx)
            .unwrap_or_else(|err| panic!("eval {postfix} at {target}: {err:?}"));
        EvaluatedExpr::new(expr, at.value, at.derivative, NumType::Transcendental)
    };
    let targets = [1e16, 1e20, 2.0_f64.powi(63), 2.0_f64.powi(100)];
    for target in targets {
        let cospi = evaluated_at("xC", target);
        assert_eq!(cospi.value, 1.0, "cospi({target})");
        assert!(
            cospi.derivative.abs() < 1e-10,
            "cospi({target}) derivative {}",
            cospi.derivative
        );
        assert!(
            !super::should_skip_degenerate_lhs(&cospi, target, &ctx),
            "cospi(x) = 1 is an identity at {target}, not a constant expression"
        );

        for postfix in ["1", "1x^", "p"] {
            let constant = evaluated_at(postfix, target);
            assert!(
                super::should_skip_degenerate_lhs(&constant, target, &ctx),
                "{postfix} should stay degenerate at {target}"
            );
        }
    }

    // The historical probe is unchanged for ordinary targets, including a
    // constant and a genuinely flat-at-a-point function whose value moves.
    let constant = evaluated_at("1x^", 2.5);
    assert!(super::should_skip_degenerate_lhs(&constant, 2.5, &ctx));
    let square = evaluated_at("xs", 0.0);
    assert!(
        !super::should_skip_degenerate_lhs(&square, 0.0, &ctx),
        "x^2 is flat at 0 but not constant"
    );

    // The identity has to survive the actual search filter, not only the helper.
    let mut config = fast_test_config();
    config.constants = vec![Symbol::One];
    config.unary_ops = vec![Symbol::CosPi];
    config.binary_ops = vec![];
    config.max_lhs_complexity = 40;
    config.max_rhs_complexity = 20;
    config.max_length = 4;
    for target in targets {
        let matches = super::search(target, &config, 10);
        assert!(
            matches
                .iter()
                .any(|m| { m.lhs.expr.to_postfix() == "xC" && m.rhs.expr.to_postfix() == "1" }),
            "missing cospi(x) = 1 for {target}: {}",
            matches
                .iter()
                .map(|m| format!("{} = {}", m.lhs.expr.to_postfix(), m.rhs.expr.to_postfix()))
                .collect::<Vec<_>>()
                .join(", ")
        );
    }
}

fn integer_lattice_config() -> crate::gen::GenConfig {
    use crate::symbol::Symbol;
    let mut config = fast_test_config();
    config.max_lhs_complexity = 55;
    config.max_rhs_complexity = 40;
    config.max_length = 8;
    config.constants = vec![
        Symbol::One,
        Symbol::Two,
        Symbol::Pi,
        Symbol::E,
        Symbol::Phi,
        Symbol::Catalan,
        Symbol::Plastic,
    ];
    config.unary_ops = vec![
        Symbol::Neg,
        Symbol::Recip,
        Symbol::Sqrt,
        Symbol::Square,
        Symbol::Exp,
        Symbol::SinPi,
        Symbol::CosPi,
    ];
    config.binary_ops = vec![Symbol::Add, Symbol::Sub];
    config
}

fn is_exact_equation(matches: &[super::Match], lhs: &str, rhs: &str) -> bool {
    matches
        .iter()
        .any(|m| m.exact && m.lhs.expr.to_postfix() == lhs && m.rhs.expr.to_postfix() == rhs)
}

fn has_equation(matches: &[super::Match], lhs: &str, rhs: &str) -> bool {
    matches
        .iter()
        .any(|m| m.lhs.expr.to_postfix() == lhs && m.rhs.expr.to_postfix() == rhs)
}

fn equation_list(matches: &[super::Match]) -> String {
    matches
        .iter()
        .map(|m| {
            format!(
                "{} = {} exact={} err={:.3e}",
                m.lhs.expr.to_postfix(),
                m.rhs.expr.to_postfix(),
                m.exact,
                m.error
            )
        })
        .collect::<Vec<_>>()
        .join(", ")
}

#[test]
fn test_large_integer_flat_near_misses_are_not_exact() {
    // 2^63. cospi(sqrt(x)) is about 0.997 with a tiny derivative, and 2+1/x
    // equals 2 only because 1/x underflows. Neither is an exact root.
    let target = 2.0_f64.powi(63);
    let config = integer_lattice_config();
    for (label, matches) in [
        ("batch", super::search(target, &config, 12)),
        (
            "streaming",
            super::search_streaming(target, &config, 12, false, None).0,
        ),
    ] {
        assert!(
            !is_exact_equation(&matches, "xqC", "1"),
            "{label} stamped cospi(sqrt(x)) = 1 exact: {}",
            equation_list(&matches)
        );
        assert!(
            !is_exact_equation(&matches, "2xr+", "2") && !is_exact_equation(&matches, "xr2+", "2"),
            "{label} stamped 2+1/x = 2 exact: {}",
            equation_list(&matches)
        );
        assert!(
            is_exact_equation(&matches, "xC", "1"),
            "{label} dropped the real identity cospi(x) = 1: {}",
            equation_list(&matches)
        );
    }
}

#[test]
fn test_newton_does_not_call_a_sub_ulp_step_exact() {
    use crate::expr::Expression;

    let two_52 = 2.0_f64.powi(52);
    let ctx = crate::eval::EvalContext::default();
    let exp_sinpi = Expression::parse("xSE").expect("e^sinpi(x)");
    let stuck = newton_raphson(&exp_sinpi, 2.0, two_52, 15);
    assert_eq!(stuck, Some(two_52));
    assert!(
        !super::newton::refined_match_is_real(&exp_sinpi, 2.0, two_52, &ctx),
        "e^sinpi(x) = 2 is not a root at 2^52 (value is 1)"
    );
    let root = newton_raphson(&exp_sinpi, 1.0, two_52, 15).expect("e^sinpi(x) = 1");
    assert!(
        (root - two_52).abs() < 1.0,
        "true root should stay on 2^52, got {root}"
    );
    assert!(super::newton::refined_match_is_real(
        &exp_sinpi, 1.0, root, &ctx
    ));

    // The same early-accept used to stamp these on main. sinpi(x^2) is 0,
    // cospi(x^2) is 1.
    let sin_square = Expression::parse("xsS").expect("sinpi(x^2)");
    assert!(!super::newton::refined_match_is_real(
        &sin_square,
        1.0,
        two_52,
        &ctx
    ));
    let cos_square = Expression::parse("xsC").expect("cospi(x^2)");
    assert!(newton_raphson(&cos_square, 2.0, two_52, 15).is_none());

    let one_e15 = 1.0e15;
    let catalan = 0.915_965_594_177_219;
    assert!(
        !super::newton::refined_match_is_real(&exp_sinpi, catalan, one_e15, &ctx),
        "e^sinpi(x) = catalan leaves a residual of about 0.084 at 1e15"
    );

    // A step of about 0.32 passes the 1e-15*(1+|x|) test at |x| = 1e15 and
    // lands one ulp away. The residual there is still O(1).
    for target in [one_e15, -one_e15] {
        let moved = newton_raphson(&exp_sinpi, 2.0, target, 15).expect("step test returns");
        assert!(
            moved != target,
            "expected the step to leave {target}, got {moved}"
        );
        assert!(
            !super::newton::refined_match_is_real(&exp_sinpi, 2.0, moved, &ctx),
            "e^sinpi(x) = 2 at {moved} is not a root of target {target}"
        );
    }
}

#[test]
fn test_residual_is_exact_rejects_the_old_flat_window() {
    use crate::eval::evaluate;
    use crate::expr::Expression;
    use crate::symbol::Symbol;

    // atan2(pi, 2) ≈ 1.0039. That sits inside the old ±0.01 flat window and
    // outside a few ulps of 1. Restoring the window makes this assertion fail.
    let atan = evaluate(&Expression::parse("p2A").unwrap(), 1.0).unwrap();
    assert!(
        (atan.value - 1.0).abs() < 0.01 && (atan.value - 1.0).abs() > 1e-4,
        "atan2(pi, 2) = {}",
        atan.value
    );
    assert!(super::residual_is_exact(1.0, 1.0));
    assert!(super::residual_is_exact(1.0, 1.0 + 4.0 * f64::EPSILON));
    assert!(
        !super::residual_is_exact(1.0, atan.value),
        "ulp window accepted a residual of {}",
        (atan.value - 1.0).abs()
    );
    // cospi(sqrt(2^63)) ≈ 0.997, the near-miss that used to be stamped exact.
    assert!(!super::residual_is_exact(0.997170624192306, 1.0));

    // Even integer: cospi(x) is exactly 1, derivative 0, and target+e is a
    // real perturbation so the LHS is kept. The flat matcher is what decides
    // whether atan2(pi, 2) is an exact partner.
    let mut config = fast_test_config();
    config.max_lhs_complexity = 40;
    config.max_rhs_complexity = 40;
    config.constants = vec![Symbol::One, Symbol::Two, Symbol::Pi];
    config.unary_ops = vec![Symbol::CosPi];
    config.binary_ops = vec![Symbol::Atan2];
    for (label, matches) in [
        ("batch", super::search(64.0, &config, 8)),
        (
            "streaming",
            super::search_streaming(64.0, &config, 8, false, None).0,
        ),
    ] {
        let list = equation_list(&matches);
        assert!(
            is_exact_equation(&matches, "xC", "1"),
            "{label} lost cospi(x) = 1: {list}"
        );
        assert!(
            !is_exact_equation(&matches, "xC", "p2A"),
            "{label} stamped cospi(x) = atan2(pi, 2) exact inside ±0.01: {list}"
        );
    }
}

#[test]
fn test_squared_trig_false_exacts_stay_out() {
    use crate::symbol::Symbol;
    // cospi(x^2) at 2^52 is really 1 with derivative 0, so LHS dedup keeps the
    // simpler cospi(x). sinpi(x^2) is 0. Neither may show up as = 1 or = 2
    // the way main's Newton step used to.
    let mut config = fast_test_config();
    config.max_lhs_complexity = 50;
    config.max_rhs_complexity = 20;
    config.constants = vec![Symbol::One, Symbol::Two];
    config.unary_ops = vec![Symbol::Square, Symbol::CosPi, Symbol::SinPi];
    config.binary_ops = vec![];
    let target = 2.0_f64.powi(52);
    for matches in [
        super::search(target, &config, 20),
        super::search_streaming(target, &config, 20, false, None).0,
    ] {
        let list = equation_list(&matches);
        assert!(
            is_exact_equation(&matches, "xC", "1"),
            "cospi(x) = 1 missing: {list}"
        );
        assert!(!is_exact_equation(&matches, "xsC", "2"), "{list}");
        assert!(!is_exact_equation(&matches, "xsS", "1"), "{list}");
        assert!(!is_exact_equation(&matches, "xsS", "2"), "{list}");
    }
}

#[test]
fn test_false_exact_identities_stay_out_of_large_integer_results() {
    let config = integer_lattice_config();

    let two_52 = 2.0_f64.powi(52);
    let at_two_52 = super::search(two_52, &config, 8);
    assert!(
        !is_exact_equation(&at_two_52, "xSE", "2"),
        "e^sinpi(x) = 2 was exact at 2^52: {}",
        equation_list(&at_two_52)
    );
    assert!(
        !is_exact_equation(&at_two_52, "xsS", "1"),
        "sinpi(x^2) = 1 was exact at 2^52: {}",
        equation_list(&at_two_52)
    );
    assert!(
        !is_exact_equation(&at_two_52, "xsC", "2"),
        "cospi(x^2) = 2 was exact at 2^52: {}",
        equation_list(&at_two_52)
    );
    assert!(
        is_exact_equation(&at_two_52, "xC", "1"),
        "cospi(x) = 1 missing at 2^52: {}",
        equation_list(&at_two_52)
    );
    assert!(
        is_exact_equation(&at_two_52, "xSE", "1"),
        "e^sinpi(x) = 1 should stay exact at 2^52: {}",
        equation_list(&at_two_52)
    );

    // 2^53 - 1 is the last odd integer. cospi is -1 there, not +1; the bug
    // was the LHS being dropped because target+e is the next integer.
    let two_53_minus = 2.0_f64.powi(53) - 1.0;
    let at_odd = super::search(two_53_minus, &config, 8);
    assert!(
        at_odd.iter().any(|m| {
            m.exact && m.lhs.expr.to_postfix() == "xC" && (m.rhs.value + 1.0).abs() == 0.0
        }),
        "cospi(x) = -1 missing at 2^53-1: {}",
        equation_list(&at_odd)
    );
    assert!(
        !is_exact_equation(&at_odd, "xC", "1"),
        "cospi(x) = 1 is false at odd 2^53-1: {}",
        equation_list(&at_odd)
    );

    let at_1e15 = super::search(1.0e15, &config, 8);
    assert!(
        at_1e15
            .iter()
            .take(8)
            .all(|m| m.rhs.expr.to_postfix() != "G"),
        "catalan ranked in the top 8 at 1e15: {}",
        equation_list(&at_1e15)
    );
    assert!(
        !is_exact_equation(&at_1e15, "xSE", "G"),
        "e^sinpi(x) = catalan was exact at 1e15: {}",
        equation_list(&at_1e15)
    );
    assert!(
        at_1e15
            .iter()
            .all(|m| !(m.lhs.expr.to_postfix() == "xSE" && m.rhs.expr.to_postfix() == "2")),
        "e^sinpi(x) = 2 stayed in the top 8 at 1e15 after leaving the target float: {}",
        equation_list(&at_1e15)
    );
}

#[test]
fn test_small_rhs_loose_roots_are_not_exact() {
    use crate::symbol::Symbol;

    // 146140d marked these exact because the loose cap was
    // `|f-rhs| <= 1e-5 * max(|rhs|, 1)` and exactness followed `|x-target|`.
    let target = 1.774_999_999_999_999_7e-11;

    let mut config = fast_test_config();
    config.max_lhs_complexity = 45;
    config.max_rhs_complexity = 45;
    config.max_length = 6;
    config.constants = vec![Symbol::E, Symbol::Six, Symbol::Nine];
    config.unary_ops = vec![Symbol::Recip, Symbol::SinPi];
    config.binary_ops = vec![];

    for (label, matches) in [
        ("batch", super::search(target, &config, 16)),
        (
            "streaming",
            super::search_streaming(target, &config, 16, false, None).0,
        ),
    ] {
        let list = equation_list(&matches);
        assert!(
            has_equation(&matches, "xrS", "er"),
            "{label} dropped sinpi(1/x) = 1/e (relative gap about 4.9e-6 still passes the loose cap): {list}"
        );
        assert!(
            !is_exact_equation(&matches, "xrS", "er"),
            "{label} stamped sinpi(1/x) = 1/e exact from the loose path: {list}"
        );
        assert!(
            !has_equation(&matches, "xrS", "9r"),
            "{label} kept sinpi(1/x) = 1/9 (relative gap about 6.8e-5): {list}"
        );
        assert!(
            !has_equation(&matches, "xrS", "6r"),
            "{label} kept sinpi(1/x) = 1/6 (relative gap about 4.8e-5): {list}"
        );
    }

    // Constant 2 with reciprocal, square, exp, and sinpi. `(1/e^(2^2))^2` is
    // exp(-8). Sides are about 3.26e-4 and 3.35e-4.
    let mut small = fast_test_config();
    small.max_lhs_complexity = 55;
    small.max_rhs_complexity = 55;
    small.max_length = 8;
    small.constants = vec![Symbol::Two];
    small.unary_ops = vec![Symbol::Recip, Symbol::Square, Symbol::Exp, Symbol::SinPi];
    small.binary_ops = vec![];
    for (label, matches) in [
        ("batch", super::search(target, &small, 8)),
        (
            "streaming",
            super::search_streaming(target, &small, 8, false, None).0,
        ),
    ] {
        let list = equation_list(&matches);
        assert!(
            !has_equation(&matches, "xrS", "2sErs"),
            "{label} ranked sinpi(1/x) = exp(-8) (about 2.9% relative): {list}"
        );
        assert!(
            !is_exact_equation(&matches, "xrS", "2sErs"),
            "{label} stamped sinpi(1/x) = exp(-8) exact: {list}"
        );
        assert!(
            !matches.is_empty(),
            "{label} generator produced no rows to compare against exp(-8)"
        );
    }
}

fn restricted_config(
    constants: Vec<crate::symbol::Symbol>,
    unary_ops: Vec<crate::symbol::Symbol>,
    binary_ops: Vec<crate::symbol::Symbol>,
    complexity: u32,
    max_length: usize,
) -> crate::gen::GenConfig {
    let mut config = fast_test_config();
    config.max_lhs_complexity = complexity;
    config.max_rhs_complexity = complexity;
    config.max_length = max_length;
    config.constants = constants;
    config.unary_ops = unary_ops;
    config.binary_ops = binary_ops;
    config
}

fn batch_and_streaming(
    target: f64,
    config: &crate::gen::GenConfig,
    n: usize,
) -> Vec<(&'static str, Vec<super::Match>)> {
    vec![
        ("batch", super::search(target, config, n)),
        (
            "streaming",
            super::search_streaming(target, config, n, false, None).0,
        ),
    ]
}

#[test]
fn test_exact_uses_sides_at_the_typed_target() {
    use crate::eval::evaluate;
    use crate::expr::Expression;
    use crate::symbol::Symbol;

    // Steep near a tiny target: Newton lands a few ulps away, where sinpi(1/x)
    // is exactly 0 or 1, but the sides at the typed target do not agree.
    let tiny = 1.774_999_999_999_999_7e-11;
    let sinpi = evaluate(&Expression::parse("xrS").unwrap(), tiny).unwrap();
    let ln_one = evaluate(&Expression::parse("1l").unwrap(), tiny).unwrap();
    assert!(
        sinpi.value.abs() > 1e-2 && ln_one.value == 0.0,
        "sinpi(1/x) at the target is {}, ln(1) is {}",
        sinpi.value,
        ln_one.value
    );
    assert!(
        !super::sides_agree_within_ulps(sinpi.value, ln_one.value),
        "sides at the typed target must not count as exact"
    );

    // sinpi(1) is also 0 and ties ln(1) on complexity. RHS dedupe keeps one of
    // them, so the right-hand side is generated with ln only. The left-hand
    // side still has sinpi.
    let mut ln_one_cfg = restricted_config(
        vec![Symbol::One],
        vec![Symbol::Recip, Symbol::SinPi, Symbol::Ln],
        vec![],
        45,
        4,
    );
    ln_one_cfg.rhs_unary_ops = Some(vec![Symbol::Ln]);
    let companions = restricted_config(
        vec![Symbol::One],
        vec![Symbol::Recip, Symbol::SinPi, Symbol::Exp],
        vec![Symbol::Add, Symbol::Sub],
        55,
        6,
    );
    let cases = [
        (&ln_one_cfg, &[("xrS", "1l")][..]),
        (
            &companions,
            &[("xrSE", "1"), ("1xrS+", "1"), ("1xrS-", "1")][..],
        ),
    ];
    for (config, equations) in cases {
        for (label, matches) in batch_and_streaming(tiny, config, 16) {
            let list = equation_list(&matches);
            for &(lhs, rhs) in equations {
                assert!(
                    has_equation(&matches, lhs, rhs),
                    "{label} lost {lhs} = {rhs}, so the exactness check would be vacuous: {list}"
                );
                assert!(
                    !is_exact_equation(&matches, lhs, rhs),
                    "{label} stamped {lhs} = {rhs} exact from a nearby root: {list}"
                );
                let row = matches
                    .iter()
                    .find(|m| m.lhs.expr.to_postfix() == lhs && m.rhs.expr.to_postfix() == rhs)
                    .expect("row present");
                assert!(
                    row.error.abs() < crate::thresholds::EXACT_MATCH_TOLERANCE,
                    "{label} {lhs} = {rhs} left the 1e-14 window (err {}), so this is a different match",
                    row.error
                );
            }
        }
    }

    let smaller = 1e-15;
    let sinpi_small = evaluate(&Expression::parse("xrS").unwrap(), smaller).unwrap();
    assert!(
        !super::sides_agree_within_ulps(sinpi_small.value, 1.0),
        "sinpi(1/x) at 1e-15 is {}, not 1",
        sinpi_small.value
    );
    let sinpi_one = restricted_config(
        vec![Symbol::One],
        vec![Symbol::Recip, Symbol::SinPi],
        vec![],
        40,
        4,
    );
    for (label, matches) in batch_and_streaming(smaller, &sinpi_one, 8) {
        let list = equation_list(&matches);
        assert!(
            has_equation(&matches, "xrS", "1"),
            "{label} lost sinpi(1/x) = 1: {list}"
        );
        assert!(
            !is_exact_equation(&matches, "xrS", "1"),
            "{label} stamped sinpi(1/x) = 1 exact at 1e-15: {list}"
        );
    }

    // Flat root: cbrt(1e6) and (1+9)^2 agree to a couple of ulps of 100, but
    // Newton stops ~8e-10 away. The old 1e-14 window dropped the row.
    let million = 1.0e6;
    let cbrt = evaluate(&Expression::parse("3xv").unwrap(), million).unwrap();
    let hundred = evaluate(&Expression::parse("19+s").unwrap(), million).unwrap();
    assert!(
        super::sides_agree_within_ulps(cbrt.value, hundred.value),
        "cbrt(1e6) = {}, (1+9)^2 = {}",
        cbrt.value,
        hundred.value
    );
    let roots = restricted_config(
        vec![Symbol::One, Symbol::Three, Symbol::Nine],
        vec![Symbol::Square],
        vec![Symbol::Add, Symbol::Root],
        50,
        6,
    );
    for (label, matches) in batch_and_streaming(million, &roots, 16) {
        let list = equation_list(&matches);
        assert!(
            is_exact_equation(&matches, "3xv", "19+s"),
            "{label} dropped x^(1/3) = (1+9)^2: {list}"
        );
        let row = matches
            .iter()
            .find(|m| m.lhs.expr.to_postfix() == "3xv" && m.rhs.expr.to_postfix() == "19+s")
            .expect("cube root row");
        assert!(
            row.error.abs() > crate::thresholds::EXACT_MATCH_TOLERANCE,
            "{label} cube-root x-error {} is inside 1e-14; the window would not have dropped it",
            row.error
        );
        assert!(
            super::solved_x_within_ulps(row.x_value, million),
            "{label} cube-root x {} is outside 8 ulps of 1e6",
            row.x_value
        );
    }

    // 64's cube root is about 2 ulps of 64 (2.84e-14), outside the old 1e-14
    // window and inside 8 ulps. A 1-ulp cap or the absolute window drops it.
    let sixty_four = 64.0;
    let cbrt_64 = restricted_config(
        vec![Symbol::Three, Symbol::Four],
        vec![],
        vec![Symbol::Root],
        40,
        4,
    );
    for (label, matches) in batch_and_streaming(sixty_four, &cbrt_64, 8) {
        let list = equation_list(&matches);
        assert!(
            is_exact_equation(&matches, "3xv", "4"),
            "{label} dropped x^(1/3) = 4: {list}"
        );
        let row = matches
            .iter()
            .find(|m| m.lhs.expr.to_postfix() == "3xv" && m.rhs.expr.to_postfix() == "4")
            .expect("cube root of 64");
        assert!(
            row.error.abs() > crate::thresholds::EXACT_MATCH_TOLERANCE,
            "{label} 64 cube-root x-error {} is inside 1e-14",
            row.error
        );
        assert!(
            super::solved_x_within_ulps(row.x_value, sixty_four),
            "{label} 64 cube-root x {} is outside 8 ulps",
            row.x_value
        );
    }
}

#[test]
fn test_small_target_underflow_is_skipped() {
    use crate::eval::{evaluate_fast_with_context, EvalContext};
    use crate::expr::{EvaluatedExpr, Expression};
    use crate::symbol::{NumType, Symbol};

    let ctx = EvalContext::default();
    let evaluated_at = |postfix: &str, target: f64| {
        let expr = Expression::parse(postfix).unwrap_or_else(|| panic!("parse {postfix}"));
        let at = evaluate_fast_with_context(&expr, target, &ctx)
            .unwrap_or_else(|err| panic!("eval {postfix} at {target}: {err:?}"));
        EvaluatedExpr::new(expr, at.value, at.derivative, NumType::Transcendental)
    };

    let tiny = 1.774_999_999_999_999_7e-11;
    // x^2 has vanished into the constant. At 1e-8 the derivative of 2+x^2 is
    // 2e-8, above the flat-path threshold, so the skip has to run first.
    // 1.1e-8 is past the old fixed cutoff and still bitwise. 2e-8 moves 2+x^2
    // by one ulp of 2, and 6e-8 is still inside the 8-ulp exact window.
    // exp(x^2) leaves that window before 6e-8, so it is not in the last set.
    for target in [tiny, 1e-8, -1e-8, 1.1e-8, -1.1e-8, 2e-8] {
        for postfix in ["2xs+", "pxs+", "xsE", "xsC", "4xs+", "5xs+"] {
            let lhs = evaluated_at(postfix, target);
            assert!(
                super::should_skip_degenerate_lhs(&lhs, target, &ctx),
                "{postfix} at {target} still has an x term in floating point (value {})",
                lhs.value
            );
        }
    }
    for postfix in ["2xs+", "pxs+", "xsC", "4xs+", "5xs+"] {
        let lhs = evaluated_at(postfix, 6e-8);
        assert!(
            super::should_skip_degenerate_lhs(&lhs, 6e-8, &ctx),
            "{postfix} at 6e-8 still has an x term in floating point (value {})",
            lhs.value
        );
    }

    // 2+(2e-8)^2 is 2+1ulp. Bitwise inequality is not "a real function of x"
    // while that ulp still passes the exact-side rule.
    let moved = evaluated_at("2xs+", 2e-8);
    assert_ne!(moved.value, 2.0, "2 + (2e-8)^2 should move by one ulp");
    assert!(
        super::sides_agree_within_ulps(moved.value, 2.0),
        "2 + (2e-8)^2 = {} must still be inside the exact tolerance of 2",
        moved.value
    );

    // Just past the 8-ulp window the addend is a real change and stays.
    let past = evaluated_at("2xs+", 7e-8);
    assert!(
        !super::sides_agree_within_ulps(past.value, 2.0),
        "2 + (7e-8)^2 = {} should have left the 8-ulp window",
        past.value
    );
    assert!(
        !super::should_skip_degenerate_lhs(&past, 7e-8, &ctx),
        "2 + x^2 at 7e-8 has left the flush window"
    );

    // Target 0 is the probe point itself. x^2 is flat there and must stay.
    assert!(!super::should_skip_degenerate_lhs(
        &evaluated_at("xs", 0.0),
        0.0,
        &ctx
    ));
    assert!(!super::should_skip_degenerate_lhs(
        &evaluated_at("2xs+", 0.0),
        0.0,
        &ctx
    ));

    // 1/x does not evaluate at 0 or at a subnormal, so the tiny-target skip
    // must not eat it. sinpi(1/x) is the steep root the exactness test sees.
    assert!(!super::should_skip_degenerate_lhs(
        &evaluated_at("xrS", tiny),
        tiny,
        &ctx
    ));

    // Same vanished-x pattern when 0 is outside the domain, or when ±1 is
    // still a root (cospi(6) = 1) and only the 0.5 probe moves. True
    // identities hold at that probe and stay.
    // Same flush once the value is merely within a few ulps of the limit,
    // including cospi(plastic"/x) and 2+x at 1e-15 (a couple of ulps of 2).
    for postfix in ["6x*C", "xf^E", "1xf^-", "xx^C", "xCWr", "PxvC", "2x+"] {
        let target = if postfix == "2x+" { 1e-15 } else { tiny };
        assert!(
            super::should_skip_degenerate_lhs(&evaluated_at(postfix, target), target, &ctx),
            "{postfix} still depends on x at {target}"
        );
    }
    for postfix in ["xxsL", "xxqL", "xrxrL"] {
        assert!(
            !super::should_skip_degenerate_lhs(&evaluated_at(postfix, tiny), tiny, &ctx),
            "{postfix} is an identity at {tiny} and must stay"
        );
    }
    assert!(!super::should_skip_degenerate_lhs(
        &evaluated_at("xxr*", 1.1e-8),
        1.1e-8,
        &ctx
    ));
    // Equal to the limit at 0 for a real reason, not because x flushed.
    assert!(!super::should_skip_degenerate_lhs(
        &evaluated_at("xsC", 2.0_f64.sqrt()),
        2.0_f64.sqrt(),
        &ctx
    ));
    assert!(!super::should_skip_degenerate_lhs(
        &evaluated_at("xrC", 0.1),
        0.1,
        &ctx
    ));

    let config = restricted_config(
        vec![
            Symbol::One,
            Symbol::Two,
            Symbol::Three,
            Symbol::Four,
            Symbol::Five,
            Symbol::Pi,
        ],
        vec![Symbol::Square, Symbol::Exp, Symbol::CosPi, Symbol::Recip],
        vec![Symbol::Add, Symbol::Sub, Symbol::Mul],
        45,
        5,
    );
    let vanished = [
        "2xs+", "xs2+", "pxs+", "xsp+", "xsE", "xsC", "pxs-", "3xs+", "3xs-", "4xs+", "5xs+",
    ];
    for target in [tiny, 1e-8, 1.1e-8, -1.1e-8, 2e-8, 6e-8] {
        for (label, matches) in batch_and_streaming(target, &config, 8) {
            let list = equation_list(&matches);
            for lhs in vanished {
                assert!(
                    !matches.iter().any(|m| m.lhs.expr.to_postfix() == lhs),
                    "{label} reported {lhs} at {target}: {list}"
                );
            }
            if target.abs() == 1.1e-8 {
                assert!(
                    is_exact_equation(&matches, "xxr*", "1"),
                    "{label} dropped x*(1/x) = 1 at {target}: {list}"
                );
            }
        }
    }
}

#[test]
fn test_exact_requires_solved_x_near_the_target() {
    use crate::symbol::Symbol;

    // Sides of e^x = 1 agree at 1e-15, and Newton walks to 0. That x-error is
    // ~1e-15, far past 8 ulps of the target. The 1e6 cube root (~7 ulps) is
    // covered by test_exact_uses_sides_at_the_typed_target.
    let small = 1e-15;
    assert!(!super::match_is_exact(1.0, 1.0, 0.0, small));
    assert!(super::solved_x_within_ulps(
        1.0e6 + 8.149_072_527_885_437e-10,
        1.0e6
    ));
    assert!(super::solved_x_within_ulps(
        64.0 + 2.842_170_943_040_401e-14,
        64.0
    ));
    assert!(!super::solved_x_within_ulps(
        1.0e6 + 9.0 * (1.0e6_f64.next_up() - 1.0e6),
        1.0e6
    ));

    let near_zero = restricted_config(
        vec![
            Symbol::One,
            Symbol::Two,
            Symbol::Three,
            Symbol::Four,
            Symbol::Pi,
        ],
        vec![Symbol::Exp, Symbol::Recip],
        vec![Symbol::Add, Symbol::Sub, Symbol::Pow],
        50,
        5,
    );
    let walked_off = [
        ("xE", "1"),
        ("1x-", "1"),
        ("2x^", "1"),
        ("2x+", "2"),
        ("px^", "1"),
        ("2x-", "2"),
        ("3x^", "1"),
        ("4x^", "1"),
    ];
    for target in [small, -small] {
        for (label, matches) in batch_and_streaming(target, &near_zero, 16) {
            let list = equation_list(&matches);
            for (lhs, rhs) in walked_off {
                assert!(
                    !is_exact_equation(&matches, lhs, rhs),
                    "{label} stamped {lhs} = {rhs} exact at {target}: {list}"
                );
            }
        }
    }

    // x*(1/x) = 1 at 1e-15 is a real identity. A config that actually
    // generates it must keep the row exact (the 1.1e-8 search covers the
    // same identity; this is the smaller target).
    let identity = restricted_config(
        vec![Symbol::One],
        vec![Symbol::Recip],
        vec![Symbol::Mul],
        50,
        5,
    );
    for target in [small, -small] {
        for (label, matches) in batch_and_streaming(target, &identity, 8) {
            assert!(
                is_exact_equation(&matches, "xxr*", "1"),
                "{label} dropped x*(1/x) = 1 at {target}: {}",
                equation_list(&matches)
            );
        }
    }

    // Level-2 rows whose sides barely move and whose solved x is ~1e-11 off.
    let tiny = 1.774_999_999_999_999_7e-11;
    assert!(!super::match_is_exact(1.0, 1.0, 4.2e-12, tiny));
    assert!(!super::solved_x_within_ulps(0.0, tiny));
    let flat_near_root = restricted_config(
        vec![Symbol::One, Symbol::Pi, Symbol::E, Symbol::Plastic],
        vec![Symbol::CosPi, Symbol::Square, Symbol::Exp, Symbol::Root],
        vec![Symbol::Add],
        60,
        6,
    );
    for (label, matches) in batch_and_streaming(tiny, &flat_near_root, 16) {
        let list = equation_list(&matches);
        assert!(
            !is_exact_equation(&matches, "PxvC", "1"),
            "{label} stamped cospi(plastic\"/x) = 1 exact: {list}"
        );
        assert!(
            !is_exact_equation(&matches, "psEx+", "psE"),
            "{label} stamped e^(pi^2)+x = e^(pi^2) exact: {list}"
        );
    }

    // cospi(x) = 1 just off 2: the sides agree and Newton walks back to 2.
    // The slope is too large for the flush skip, so the row stays, not exact.
    let just_off = 2.00000001;
    let cospi = restricted_config(vec![Symbol::One], vec![Symbol::CosPi], vec![], 40, 3);
    for (label, matches) in batch_and_streaming(2.0, &cospi, 4) {
        assert!(
            is_exact_equation(&matches, "xC", "1"),
            "{label} dropped cospi(x) = 1 at 2: {}",
            equation_list(&matches)
        );
    }
    for (label, matches) in batch_and_streaming(just_off, &cospi, 4) {
        let list = equation_list(&matches);
        assert!(
            has_equation(&matches, "xC", "1"),
            "{label} lost cospi(x) = 1 off 2, so the exactness check would be vacuous: {list}"
        );
        assert!(
            !is_exact_equation(&matches, "xC", "1"),
            "{label} stamped cospi(x) = 1 exact off an even integer: {list}"
        );
        let row = matches
            .iter()
            .find(|m| m.lhs.expr.to_postfix() == "xC")
            .expect("cospi row");
        assert!(
            row.error.abs() > 8.0 * (just_off.next_up() - just_off).abs(),
            "{label} cospi error {} is inside 8 ulps of the target",
            row.error
        );
    }
}

#[test]
fn test_search_adaptive_rejects_non_finite_target() {
    let config = fast_test_config();
    let search_config = SearchConfig {
        target: f64::NAN,
        max_matches: 10,
        ..Default::default()
    };

    let (matches, _) = super::search_adaptive(&config, &search_config, 0);
    assert!(
        matches.is_empty(),
        "non-finite targets should return no matches"
    );
}

#[test]
fn test_adaptive_target_count_exponent_does_not_overflow() {
    assert_eq!(super::adaptive_target_count(u32::MAX), usize::MAX);
}
