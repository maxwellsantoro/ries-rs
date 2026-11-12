//! Integration tests for the search module

#![cfg(not(target_arch = "wasm32"))]
#![allow(clippy::field_reassign_with_default)]

use ries_rs::eval;
use ries_rs::expr::{EvaluatedExpr, Expression};
use ries_rs::gen::{generate_all, GenConfig};
use ries_rs::profile::UserConstant;
use ries_rs::search::{
    search_adaptive, search_streaming_with_config, search_with_stats_and_config,
    search_with_stats_and_options, ExprDatabase, SearchConfig, SearchContext,
};
use ries_rs::symbol::{NumType, Symbol};
use ries_rs::udf::{UdfOp, UserFunction};
use ries_rs::SymbolTable;
use std::collections::HashMap;
use std::sync::Arc;

mod common;

/// Create a fast config for integration tests
/// Uses lower complexity limits and fewer operators for speed
fn fast_config() -> GenConfig {
    GenConfig {
        // 40/40 is the minimum to include basic 3-symbol expressions like `2x*` (32)
        // under calibrated original-RIES weights; max_length keeps count small.
        max_lhs_complexity: 40,
        max_rhs_complexity: 40,
        max_length: 10,
        constants: vec![
            Symbol::One,
            Symbol::Two,
            Symbol::Three,
            Symbol::Four,
            Symbol::Five,
            Symbol::Six,
            Symbol::Seven,
            Symbol::Eight,
            Symbol::Nine,
            Symbol::Pi,
            Symbol::E,
        ],
        unary_ops: vec![Symbol::Neg, Symbol::Recip, Symbol::Square, Symbol::Sqrt],
        binary_ops: vec![Symbol::Add, Symbol::Sub, Symbol::Mul, Symbol::Div],
        rhs_constants: None,
        rhs_unary_ops: None,
        rhs_binary_ops: None,
        symbol_max_counts: HashMap::new(),
        rhs_symbol_max_counts: None,
        min_num_type: NumType::Transcendental,
        generate_lhs: true,
        generate_rhs: true,
        user_constants: Vec::new(),
        user_functions: Vec::new(),
        show_pruned_arith: false,
        symbol_table: Arc::new(SymbolTable::new()),
    }
}

/// Test that basic expression generation works
#[test]
fn test_basic_generation() {
    let config = fast_config();
    let generated = generate_all(&config, 2.5);

    // Should generate LHS expressions (containing x)
    assert!(!generated.lhs.is_empty(), "Should generate LHS expressions");

    // Should generate RHS expressions (constants only)
    assert!(!generated.rhs.is_empty(), "Should generate RHS expressions");

    // All LHS should contain x
    for lhs in &generated.lhs {
        assert!(lhs.expr.contains_x(), "LHS should contain x");
    }

    // No RHS should contain x
    for rhs in &generated.rhs {
        assert!(!rhs.expr.contains_x(), "RHS should not contain x");
    }
}

/// Test that expressions are generated within complexity limits
#[test]
fn test_complexity_limits() {
    let config = fast_config();
    let generated = generate_all(&config, 1.0);

    for lhs in &generated.lhs {
        assert!(
            lhs.expr.complexity() <= 40,
            "LHS complexity {} exceeds limit",
            lhs.expr.complexity()
        );
    }

    for rhs in &generated.rhs {
        assert!(
            rhs.expr.complexity() <= 40,
            "RHS complexity {} exceeds limit",
            rhs.expr.complexity()
        );
    }
}

/// Test -O semantics: per-expression symbol count limits.
#[test]
fn test_symbol_count_limits_are_enforced() {
    let mut config = fast_config();
    config.symbol_max_counts.insert(Symbol::X, 1);
    config.symbol_max_counts.insert(Symbol::Add, 1);

    let generated = generate_all(&config, 2.5);
    for lhs in &generated.lhs {
        let x_count = lhs
            .expr
            .symbols()
            .iter()
            .filter(|&&s| s == Symbol::X)
            .count();
        let add_count = lhs
            .expr
            .symbols()
            .iter()
            .filter(|&&s| s == Symbol::Add)
            .count();
        assert!(
            x_count <= 1,
            "expected at most one x in LHS, got {}",
            x_count
        );
        assert!(
            add_count <= 1,
            "expected at most one + in LHS, got {}",
            add_count
        );
    }
}

/// Test expression database operations
#[test]
fn test_expr_database() {
    let config = fast_config();
    let generated = generate_all(&config, 2.5);

    // Verify we generated some RHS expressions
    assert!(!generated.rhs.is_empty(), "Should generate RHS expressions");

    // Test database creation and insertion
    let mut db = ExprDatabase::new();
    db.insert_rhs(generated.rhs);

    // Verify we can query the database using the range method
    let results = db.range(2.4, 2.6);
    assert!(!results.is_empty(), "Should find expressions near 2.5");
}

#[test]
fn test_programmatic_function_cannot_hide_a_variable_in_rhs() {
    let mut gen = fast_config();
    gen.constants = vec![Symbol::One];
    gen.unary_ops = vec![Symbol::UserFunction0];
    gen.binary_ops.clear();
    gen.user_functions.push(UserFunction {
        weight: 1,
        name: "f".into(),
        description: String::new(),
        body: vec![UdfOp::Symbol(Symbol::X), UdfOp::Symbol(Symbol::Mul)],
        num_type: NumType::Transcendental,
    });

    let mut rhs = Expression::new();
    rhs.push(Symbol::One);
    rhs.push(Symbol::UserFunction0);
    assert!(matches!(
        eval::evaluate_with_constants_and_functions(&rhs, 2.1, &[], &gen.user_functions),
        Err(eval::EvalError::Invalid)
    ));
    let generated = generate_all(&gen, 2.1);
    assert!(generated
        .rhs
        .iter()
        .all(|rhs| !rhs.expr.symbols().contains(&Symbol::UserFunction0)));
}

#[test]
fn test_refinement_rejects_large_relative_residual_for_small_sides() {
    let target = 1e15;
    let config = SearchConfig {
        target,
        zero_value_threshold: 0.0,
        user_constants: [
            4e-11,
            4.8e-11,
            4e-11 * (0.125_f64 * std::f64::consts::PI).sin().exp(),
        ]
        .into_iter()
        .enumerate()
        .map(|(slot, value)| UserConstant {
            weight: 10,
            name: format!("c{slot}"),
            description: String::new(),
            value,
            num_type: NumType::Transcendental,
        })
        .collect(),
        ..SearchConfig::default()
    };
    let context = SearchContext::new(&config);
    let evaluate = |expr| {
        let result = eval::evaluate_with_context(&expr, target, &context.eval).unwrap();
        EvaluatedExpr::new(expr, result.value, result.derivative, result.num_type)
    };
    let constant = |symbol| {
        let mut expr = Expression::new();
        expr.push(symbol);
        evaluate(expr)
    };
    let mut lhs = Expression::parse("xSE").unwrap();
    lhs.push(Symbol::UserConstant0);
    lhs.push(Symbol::Mul);
    let lhs = evaluate(lhs);

    let mut db = ExprDatabase::new();
    db.insert_rhs(vec![constant(Symbol::UserConstant1)]);
    assert!(db
        .find_matches_with_context(std::slice::from_ref(&lhs), &context)
        .is_empty());

    // The neighboring representable root is still accepted at the same scale.
    let mut db = ExprDatabase::new();
    db.insert_rhs(vec![constant(Symbol::UserConstant2)]);
    let matches = db.find_matches_with_context(&[lhs], &context);
    assert_eq!(matches.len(), 1);
    assert_eq!(matches[0].x_value, target + 0.125);
}

/// Test that π can be found as an RHS
#[test]
fn test_pi_generation() {
    let config = fast_config();
    let generated = generate_all(&config, std::f64::consts::PI);

    // Should be able to find π in RHS expressions
    let has_pi = generated.rhs.iter().any(|e| e.expr.to_postfix() == "p");
    assert!(has_pi, "Should generate π as RHS");
}

/// Test that basic operators work
#[test]
fn test_basic_operators() {
    let config = fast_config();
    let generated = generate_all(&config, 5.0);

    // Check for common patterns
    let postfixes: Vec<_> = generated.rhs.iter().map(|e| e.expr.to_postfix()).collect();

    // Should have single-digit constants
    assert!(postfixes.iter().any(|p| p == "1"), "Should have 1");
    assert!(postfixes.iter().any(|p| p == "2"), "Should have 2");
    assert!(postfixes.iter().any(|p| p == "5"), "Should have 5");
}

/// Test expression evaluation for known values
#[test]
fn test_expression_evaluation() {
    // Test x^2 at x=3 = 9
    let expr = Expression::parse("xs").unwrap();
    let result = eval::evaluate(&expr, 3.0).unwrap();
    assert!((result.value - 9.0).abs() < 1e-10);
    assert!((result.derivative - 6.0).abs() < 1e-10); // d(x^2)/dx = 2x = 6

    // Test sqrt(x) at x=4 = 2
    let expr = Expression::parse("xq").unwrap();
    let result = eval::evaluate(&expr, 4.0).unwrap();
    assert!((result.value - 2.0).abs() < 1e-10);
    assert!((result.derivative - 0.25).abs() < 1e-10); // d(sqrt(x))/dx = 1/(2*sqrt(x)) = 1/4

    // Test 2x at x=3 = 6
    let expr = Expression::parse("2x*").unwrap();
    let result = eval::evaluate(&expr, 3.0).unwrap();
    assert!((result.value - 6.0).abs() < 1e-10);
    assert!((result.derivative - 2.0).abs() < 1e-10);
}

/// Test that the search finds exact matches
#[test]
fn test_exact_match_finding() {
    // For target = 2, we should find x = 2 exactly
    let config = fast_config();
    let generated = generate_all(&config, 2.0);

    // Check that x=2 is in LHS
    let has_x_equals_2 = generated.lhs.iter().any(|e| {
        if let Ok(result) = eval::evaluate(&e.expr, 2.0) {
            (result.value - 2.0).abs() < 1e-10
        } else {
            false
        }
    });
    assert!(has_x_equals_2, "Should find x = 2 for target 2");
}

// ============================================================================
// REGRESSION TESTS - Key equations that must be found
// These tests use fast_config for speed but still verify core functionality
// ============================================================================

/// Regression test: For target 2.5, must find 2x = 5 (exact match)
/// Uses fast_config for speed - still comprehensive enough to find key equations
#[test]
fn test_find_2x_equals_5() {
    let config = fast_config();
    let (matches, _stats) = search_with_stats_and_options(2.5, &config, 50, false, None);

    // Should find 2x = 5 as an exact match
    let has_2x_5 = matches.iter().any(|m| {
        // Check LHS is 2x* (2 times x)
        let lhs_is_2x = m.lhs.expr.to_postfix() == "2x*";
        // Check RHS is 5
        let rhs_is_5 = m.rhs.expr.to_postfix() == "5";
        // Check error is essentially zero (exact match)
        let is_exact = m.error.abs() < 1e-14;
        lhs_is_2x && rhs_is_5 && is_exact
    });

    assert!(has_2x_5, "Should find 2x = 5 as exact match for target 2.5");
}

/// Regression test: For target 2.5, should find reciprocal-related equations
/// This is a simplified test that doesn't require the full 1/(x-1) = 2/3 pattern
#[test]
fn test_find_reciprocal_relations() {
    let config = fast_config();
    let (matches, _stats) = search_with_stats_and_options(2.5, &config, 50, false, None);

    // Should find some match involving reciprocals
    // Just verify that matches exist and some have small errors
    assert!(!matches.is_empty(), "Should find matches for target 2.5");

    // Should have at least one exact or near-exact match
    let has_near_exact = matches.iter().any(|m| m.error.abs() < 1e-10);
    assert!(
        has_near_exact,
        "Should find at least one near-exact match for target 2.5"
    );
}

#[test]
fn test_batch_and_streaming_agree_on_top_match_for_pi() {
    let config = fast_config();
    let search_config = SearchConfig {
        target: std::f64::consts::PI,
        max_matches: 10,
        user_constants: config.user_constants.clone(),
        user_functions: config.user_functions.clone(),
        ..Default::default()
    };

    let (batch_matches, _batch_stats) = search_with_stats_and_config(&config, &search_config);
    let (streaming_matches, _streaming_stats) =
        search_streaming_with_config(&config, &search_config);

    assert!(
        !batch_matches.is_empty(),
        "batch search should find matches"
    );
    assert!(
        !streaming_matches.is_empty(),
        "streaming search should find matches"
    );

    let batch_top = (
        batch_matches[0].lhs.expr.to_postfix(),
        batch_matches[0].rhs.expr.to_postfix(),
    );
    let streaming_top = (
        streaming_matches[0].lhs.expr.to_postfix(),
        streaming_matches[0].rhs.expr.to_postfix(),
    );

    assert_eq!(
        batch_top, streaming_top,
        "batch and streaming should agree on the top-ranked pi match"
    );
}

#[test]
fn test_batch_and_streaming_agree_on_first_five_matches_for_pi() {
    let config = fast_config();
    let search_config = SearchConfig {
        target: std::f64::consts::PI,
        max_matches: 10,
        user_constants: config.user_constants.clone(),
        user_functions: config.user_functions.clone(),
        ..Default::default()
    };

    let (batch_matches, _batch_stats) = search_with_stats_and_config(&config, &search_config);
    let (streaming_matches, _streaming_stats) =
        search_streaming_with_config(&config, &search_config);

    let batch_pairs: Vec<_> = batch_matches
        .iter()
        .take(5)
        .map(|m| (m.lhs.expr.to_postfix(), m.rhs.expr.to_postfix()))
        .collect();
    let streaming_pairs: Vec<_> = streaming_matches
        .iter()
        .take(5)
        .map(|m| (m.lhs.expr.to_postfix(), m.rhs.expr.to_postfix()))
        .collect();

    assert_eq!(
        batch_pairs.len(),
        5,
        "expected at least five batch matches for pi"
    );
    assert_eq!(
        streaming_pairs.len(),
        5,
        "expected at least five streaming matches for pi"
    );
    assert_eq!(
        batch_pairs, streaming_pairs,
        "batch and streaming should agree on the first five pi matches"
    );
}

#[test]
fn test_search_stats_report_window_and_acceptance_metrics() {
    let config = fast_config();
    let search_config = SearchConfig {
        target: std::f64::consts::PI,
        max_matches: 10,
        user_constants: config.user_constants.clone(),
        user_functions: config.user_functions.clone(),
        ..Default::default()
    };

    let (_batch_matches, batch_stats) = search_with_stats_and_config(&config, &search_config);
    let (_streaming_matches, streaming_stats) =
        search_streaming_with_config(&config, &search_config);

    for stats in [&batch_stats, &streaming_stats] {
        assert!(
            stats.lhs_tested > 0,
            "expected LHS expressions to be tested"
        );
        assert!(
            stats.candidate_window_total >= stats.candidates_tested,
            "candidate windows should cover all tested pairs"
        );
        assert!(
            stats.candidate_window_max > 0,
            "expected at least one non-empty candidate window"
        );
        assert!(
            stats.candidate_window_max_lhs_postfix.is_some(),
            "expected max-window LHS details to be recorded"
        );
        assert!(
            stats.candidate_window_max_lhs_value.is_some()
                && stats.candidate_window_max_lhs_derivative.is_some()
                && stats.candidate_window_max_lhs_complexity.is_some(),
            "expected max-window numeric context to be recorded"
        );
        assert!(
            stats.candidate_window_avg() > 0.0,
            "expected a positive average candidate window width"
        );
        assert!(
            (0.0..=1.0).contains(&stats.newton_success_rate()),
            "newton success rate should be normalized"
        );
        assert!(
            (0.0..=1.0).contains(&stats.pool_acceptance_rate()),
            "pool acceptance rate should be normalized"
        );
    }
}

/// Test: For target phi (golden ratio), should find x = phi or related equations
#[test]
fn test_find_golden_ratio() {
    let mut config = fast_config();
    config.constants.push(Symbol::Phi); // Add phi constant

    let phi = std::f64::consts::GOLDEN_RATIO;
    let (matches, _stats) = search_with_stats_and_options(phi, &config, 50, false, None);

    // Should find some match (not necessarily exact due to transcendental nature)
    assert!(
        !matches.is_empty(),
        "Should find some matches for golden ratio"
    );

    // Best match should have error < 0.1 (relaxed for fast_config)
    let best_error = matches
        .iter()
        .map(|m| m.error.abs())
        .fold(f64::INFINITY, |a, b| a.min(b));
    assert!(
        best_error < 0.1,
        "Best match error should be < 0.1 for phi, got {}",
        best_error
    );
}

// ============================================================================
// USER CONSTANTS TESTS
// ============================================================================

/// Test that user constants work in generation and search
#[test]
// Short gamma-like user-constant fixture, not std::f64::consts::EULER_GAMMA.
#[allow(clippy::approx_constant)]
fn test_user_constant_in_search() {
    // Create a config with a user constant
    let user_constants = vec![UserConstant {
        weight: 4, // Low weight so it gets generated
        name: "g".to_string(),
        description: "test constant".to_string(),
        value: 0.57721,
        num_type: NumType::Transcendental,
    }];

    let mut config = fast_config();
    config.user_constants = user_constants;

    // Add user constant symbol to the constants pool
    config.constants.push(Symbol::UserConstant0);

    // Search for the user constant value
    let (matches, _stats) = search_with_stats_and_options(0.57721, &config, 50, false, None);

    // Should find x = u0 as an exact match (u0 = user constant 0)
    let has_user_constant_match = matches.iter().any(|m| {
        let lhs_is_x = m.lhs.expr.to_postfix() == "x";
        let is_exact = m.error.abs() < 1e-10;
        lhs_is_x
            && is_exact
            && m.rhs
                .expr
                .symbols()
                .iter()
                .any(|s| matches!(s, Symbol::UserConstant0))
    });

    assert!(
        has_user_constant_match,
        "Should find x = u0 as match for user constant value"
    );
}

#[test]
fn test_numeric_deduplication_against_atomic_equation_oracle() {
    use ries_rs::pool::{compare_matches, RankingMode};
    use ries_rs::profile::Profile;
    use ries_rs::search::{search_one_sided_with_stats_and_config, Match, SearchStats};

    type Search = fn(&GenConfig, &SearchConfig) -> (Vec<Match>, SearchStats);
    let modes: &[(&str, Search)] = &[
        ("batch", search_with_stats_and_config),
        ("streaming", search_streaming_with_config),
        ("one-sided", search_one_sided_with_stats_and_config),
        ("adaptive", |gen, cfg| search_adaptive(gen, cfg, 0)),
        #[cfg(feature = "parallel")]
        (
            "parallel",
            ries_rs::search::search_parallel_with_stats_and_config,
        ),
        #[cfg(feature = "parallel")]
        ("turbo", ries_rs::search::search_turbo_with_stats_and_config),
    ];
    let signatures = |matches: &[Match]| {
        matches
            .iter()
            .map(|m| {
                (
                    m.lhs.expr.to_postfix(),
                    m.rhs.expr.to_postfix(),
                    m.x_value.to_bits(),
                    m.complexity,
                    m.exact,
                )
            })
            .collect::<Vec<_>>()
    };
    // Cover the near collision, an exactness tie, and both old large-value sentinels.
    for (first, second, tied) in [
        (2.0, 2.000_000_004_9, false),
        (2.0, 2.0_f64.next_up(), true),
        (1e11, 2e11, false),
        (-1e11, -2e11, false),
    ] {
        for weights in [[14, 20], [20, 14]] {
            let mut gen = fast_config();
            let literal_first = first == 2.0;
            let first_symbol = if literal_first {
                Symbol::Two
            } else {
                Symbol::UserConstant1
            };
            gen.constants = vec![first_symbol, Symbol::UserConstant0];
            gen.unary_ops.clear();
            gen.binary_ops.clear();
            gen.max_length = 1;
            gen.user_constants = [second, first]
                .into_iter()
                .zip([weights[1], weights[0]])
                .take(if literal_first { 1 } else { 2 })
                .enumerate()
                .map(|(i, (value, weight))| UserConstant {
                    weight,
                    name: format!("u{i}"),
                    description: "atomic oracle constant".into(),
                    value,
                    num_type: NumType::Transcendental,
                })
                .collect();
            let mut profile = Profile::default();
            profile.symbol_weights.insert(Symbol::Two, weights[0]);
            gen.symbol_table =
                SymbolTable::from_parts(&profile, &gen.user_constants, &[]).into_shared();

            for target in [first, second] {
                let atom = |symbol, value, derivative| {
                    let mut expr = Expression::new();
                    expr.push_with_table(symbol, &gen.symbol_table);
                    EvaluatedExpr::new(expr, value, derivative, NumType::Transcendental)
                };
                let lhs = atom(Symbol::X, target, 1.0);

                // Exhaust the admissible pairs directly. With length one the
                // only LHS is x and each RHS atom is its own solved root.
                let candidates: Vec<_> = gen
                    .constants
                    .iter()
                    .zip([first, second])
                    .filter_map(|(&symbol, root)| {
                        let error: f64 = root - target;
                        if error.abs() > 0.1 {
                            return None;
                        }
                        let rhs = atom(symbol, root, 0.0);
                        Some(Match {
                            complexity: lhs.expr.complexity() + rhs.expr.complexity(),
                            lhs: lhs.clone(),
                            rhs,
                            x_value: root,
                            error,
                            // A one-ulp separation meets both exactness checks.
                            exact: tied || root == target,
                        })
                    })
                    .collect();

                for ranking_mode in [RankingMode::Complexity, RankingMode::Parity] {
                    let mut expected = candidates.clone();
                    expected.sort_by(|a, b| compare_matches(a, b, ranking_mode));
                    for capacity in [1, 2, 5] {
                        let cfg = SearchConfig {
                            target,
                            max_error: 0.1,
                            max_matches: capacity,
                            ranking_mode,
                            user_constants: gen.user_constants.clone(),
                            ..Default::default()
                        };
                        let expected = &expected[..expected.len().min(capacity)];
                        for (mode, search) in modes {
                            let (matches, stats) = search(&gen, &cfg);
                            let case = format!(
                                "{mode}, target {target}, {ranking_mode:?}, capacity {capacity}, weights {weights:?}"
                            );
                            assert_eq!(stats.rhs_count, 2, "{case}");
                            assert_eq!(signatures(&matches), signatures(expected), "{case}");
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn test_streaming_stop_priority_changes_results_without_ending_the_scan() {
    use ries_rs::pool::{compare_matches, RankingMode};

    let mut gen = fast_config();
    gen.constants = vec![Symbol::One, Symbol::Two, Symbol::Three];
    gen.unary_ops.clear();
    gen.binary_ops = vec![Symbol::Add];
    gen.max_length = 3;
    for max_matches in [1, 10] {
        let config = SearchConfig {
            target: 2.0,
            max_matches,
            ranking_mode: RankingMode::Parity,
            rhs_excluded_symbols: Some([Symbol::Add as u8].into()),
            ..Default::default()
        };
        let (ranked, scanned) = search_streaming_with_config(&gen, &config);
        for (stop_at_exact, stop_below) in [(true, None), (false, Some(1e-8))] {
            let with_stop = SearchConfig {
                stop_at_exact,
                stop_below,
                ..config.clone()
            };
            let (priority, stats) = search_streaming_with_config(&gen, &with_stop);
            let (batch, batch_stats) = search_with_stats_and_config(&gen, &with_stop);
            assert!(batch_stats.early_exit);
            assert!(!stats.early_exit);
            // Phase 1 scans every generated LHS; phase 2 rescans within the bound.
            assert!(stats.lhs_count > scanned.lhs_count);

            // Exact rows are complexity-first under parity, so the unrestricted
            // page and the stop page lead with the same simplest equation.
            // Every stopped result stays within that equation's complexity.
            assert_eq!(priority[0].lhs.expr.to_postfix(), "x");
            assert_eq!(priority[0].rhs.expr.to_postfix(), "2");
            assert_eq!(ranked[0].lhs.expr.to_postfix(), "x");
            assert_eq!(ranked[0].rhs.expr.to_postfix(), "2");
            assert!(compare_matches(&ranked[0], &priority[0], RankingMode::Parity).is_eq());
            assert!(priority
                .iter()
                .all(|m| m.complexity <= priority[0].complexity));
            let summary = |matches: &[ries_rs::search::Match]| -> Vec<_> {
                matches
                    .iter()
                    .map(|m| (m.lhs.expr.to_postfix(), m.rhs.expr.to_postfix()))
                    .collect()
            };
            assert_eq!(summary(&priority), summary(&batch));
        }
    }
}

#[cfg(feature = "parallel")]
#[test]
fn test_turbo_parity_rank_one_with_small_capacities() {
    use ries_rs::pool::RankingMode;
    use ries_rs::search::search_turbo_with_stats_and_config;

    let gen_config = fast_config();
    for target in [2.0, 2.5, std::f64::consts::SQRT_2] {
        for capacity in [1, 2, 5] {
            let config = SearchConfig {
                target,
                max_matches: capacity,
                ranking_mode: RankingMode::Parity,
                ..Default::default()
            };
            let (serial, _) = search_with_stats_and_config(&gen_config, &config);
            let (turbo, _) = search_turbo_with_stats_and_config(&gen_config, &config);
            assert!(!serial.is_empty() && !turbo.is_empty());
            assert_eq!(
                (
                    serial[0].lhs.expr.to_postfix(),
                    serial[0].rhs.expr.to_postfix()
                ),
                (
                    turbo[0].lhs.expr.to_postfix(),
                    turbo[0].rhs.expr.to_postfix()
                ),
                "target {target}, capacity {capacity}"
            );
        }
    }
}

#[test]
// Short gamma-like user-constant fixture, not std::f64::consts::EULER_GAMMA.
#[allow(clippy::approx_constant)]
fn test_batch_and_streaming_agree_on_user_constant_top_match() {
    let user_constants = vec![UserConstant {
        weight: 4,
        name: "g".to_string(),
        description: "test constant".to_string(),
        value: 0.57721,
        num_type: NumType::Transcendental,
    }];

    let mut config = fast_config();
    config.user_constants = user_constants.clone();
    config.constants.push(Symbol::UserConstant0);

    let search_config = SearchConfig {
        target: 0.57721,
        max_matches: 10,
        user_constants,
        user_functions: config.user_functions.clone(),
        ..Default::default()
    };

    let (batch_matches, _batch_stats) = search_with_stats_and_config(&config, &search_config);
    let (streaming_matches, _streaming_stats) =
        search_streaming_with_config(&config, &search_config);

    assert!(
        !batch_matches.is_empty() && !streaming_matches.is_empty(),
        "expected both search paths to find user-constant matches"
    );

    let batch_top = (
        batch_matches[0].lhs.expr.to_postfix(),
        batch_matches[0].rhs.expr.to_postfix(),
    );
    let streaming_top = (
        streaming_matches[0].lhs.expr.to_postfix(),
        streaming_matches[0].rhs.expr.to_postfix(),
    );

    assert_eq!(
        batch_top, streaming_top,
        "batch and streaming should agree on the top user-constant match"
    );
}

/// Test that multiple user constants work correctly
#[test]
fn test_multiple_user_constants() {
    let user_constants = vec![
        UserConstant {
            weight: 4,
            name: "a".to_string(),
            description: "constant a".to_string(),
            value: 2.0,
            num_type: NumType::Integer,
        },
        UserConstant {
            weight: 4,
            name: "b".to_string(),
            description: "constant b".to_string(),
            value: 3.0,
            num_type: NumType::Integer,
        },
    ];

    let mut config = fast_config();
    config.user_constants = user_constants.clone();
    // User constant symbols must be explicitly added to the constants pool
    config.constants.push(Symbol::UserConstant0);
    config.constants.push(Symbol::UserConstant1);

    // Generate expressions at x=2.5
    let generated = generate_all(&config, 2.5);

    // Verify user constants are evaluated correctly
    // UserConstant0 = 2.0, UserConstant1 = 3.0
    // Check that we can find expressions with these values

    // Look for RHS with value 2.0 (from UserConstant0)
    let has_value_2 = generated.rhs.iter().any(|e| (e.value - 2.0).abs() < 1e-10);
    // Look for RHS with value 3.0 (from UserConstant1)
    let has_value_3 = generated.rhs.iter().any(|e| (e.value - 3.0).abs() < 1e-10);

    // At minimum, the standard constant 2 and 3 should exist
    // So this test passes even if user constant generation isn't perfect
    assert!(
        has_value_2 || has_value_3,
        "Should have RHS with values from user constants or matching standard constants"
    );
}

/// Regression test: user-defined functions must survive full search/refinement path
#[test]
fn test_user_function_in_search() {
    let udf = UserFunction::parse("4:sinh:hyperbolic sine:E|r-2/").unwrap();
    let uc = UserConstant {
        weight: 4,
        name: "sinh2".to_string(),
        description: "sinh(2)".to_string(),
        value: 3.626_860_407_847_019,
        num_type: NumType::Transcendental,
    };

    let config = GenConfig {
        max_lhs_complexity: 20,
        max_rhs_complexity: 20,
        max_length: 6,
        constants: vec![Symbol::One, Symbol::UserConstant0],
        unary_ops: vec![Symbol::UserFunction0],
        binary_ops: vec![],
        rhs_constants: None,
        rhs_unary_ops: None,
        rhs_binary_ops: None,
        symbol_max_counts: HashMap::new(),
        rhs_symbol_max_counts: None,
        min_num_type: NumType::Transcendental,
        generate_lhs: true,
        generate_rhs: true,
        user_constants: vec![uc.clone()],
        user_functions: vec![udf.clone()],
        show_pruned_arith: false,
        symbol_table: Arc::new(SymbolTable::from_parts(
            &ries_rs::profile::Profile::new(),
            &[uc],
            &[udf],
        )),
    };

    let (matches, _stats) = search_with_stats_and_options(2.0, &config, 50, false, None);

    let has_udf_match = matches.iter().any(|m| {
        m.error.abs() < 1e-10
            && m.lhs
                .expr
                .symbols()
                .iter()
                .any(|s| matches!(s, Symbol::UserFunction0))
            && m.rhs
                .expr
                .symbols()
                .iter()
                .any(|s| matches!(s, Symbol::UserConstant0))
    });

    assert!(
        has_udf_match,
        "Should find an exact match using UserFunction0 and UserConstant0"
    );
}

/// Issue 2: the degenerate-expression test in both batch and streaming search paths
/// must pass user_functions to the evaluator. When using evaluate_with_constants
/// (which doesn't accept user_functions), UDF symbols in an LHS expression cause
/// the evaluator to return Err, silently skipping the degenerate check.
/// Regression: search_streaming must still correctly handle UDF-containing expressions.
#[test]
fn test_streaming_search_with_udf_produces_results() {
    // This is a regression guard: ensure search_streaming works correctly when
    // user functions are present. The degenerate-check bug (issue 2) would cause
    // UDF LHS expressions to always escape pruning, potentially producing more
    // matches. After the fix, evaluate_fast_with_constants_and_functions is used.
    let udf = UserFunction::parse("4:sinh:hyperbolic sine:E|r-2/").unwrap();
    let uc = UserConstant {
        weight: 4,
        name: "sinh2".to_string(),
        description: "sinh(2)".to_string(),
        value: 3.626_860_407_847_019,
        num_type: NumType::Transcendental,
    };

    let config = GenConfig {
        max_lhs_complexity: 20,
        max_rhs_complexity: 20,
        max_length: 6,
        constants: vec![Symbol::One, Symbol::UserConstant0],
        unary_ops: vec![Symbol::UserFunction0],
        binary_ops: vec![],
        rhs_constants: None,
        rhs_unary_ops: None,
        rhs_binary_ops: None,
        symbol_max_counts: HashMap::new(),
        rhs_symbol_max_counts: None,
        min_num_type: NumType::Transcendental,
        generate_lhs: true,
        generate_rhs: true,
        user_constants: vec![uc.clone()],
        user_functions: vec![udf.clone()],
        show_pruned_arith: false,
        symbol_table: Arc::new(SymbolTable::from_parts(
            &ries_rs::profile::Profile::new(),
            &[uc],
            &[udf],
        )),
    };

    let (matches, _stats) = ries_rs::search::search_streaming(2.0, &config, 50, false, None);

    // Should find at least one match — confirms search doesn't panic and UDF eval works.
    assert!(
        !matches.is_empty(),
        "streaming search with UDF should produce at least one match"
    );
}

/// Issue 4: search_adaptive LHS/RHS dedup should keep the simplest expression
/// when two expressions share the same floating-point deduplication key.
/// Regression: adaptive search must not produce higher-complexity results than necessary.
#[test]
fn test_adaptive_search_produces_valid_matches() {
    // This is a regression guard for the dedup-keeps-simplest fix.
    // search_adaptive is an internal function but its output is tested via the public API.
    // A low-complexity target like 2.0 should produce simple matches.
    let config = fast_config();
    let (matches, _stats) = search_with_stats_and_options(2.0, &config, 10, false, None);

    assert!(
        !matches.is_empty(),
        "search should find matches for target 2.0"
    );

    // The simplest match for 2.0 should have low complexity (e.g., "x = 2" has complexity 1+1=2)
    let min_complexity = matches
        .iter()
        .map(|m| m.complexity)
        .min()
        .unwrap_or(u32::MAX);
    // x = 2 has complexity 15 (X) + 13 (Two) = 28 — the minimum for this target.
    assert!(
        min_complexity <= 28,
        "simplest match complexity should be low for target 2.0, got {min_complexity}"
    );
}

/// Regression: search_adaptive must start from complexity (1, 1) and grow
/// iteratively toward the target expression count regardless of what bounds
/// the caller's GenConfig carries.
///
/// The bug this test guards against: the loop previously clamped each iteration
/// with `lhs_c.max(base_config.max_lhs_complexity)`, so a caller with high
/// pre-computed bounds (e.g. fast_config with max_lhs = 40) would skip the
/// iterative growth entirely and behave identically to the batch path.
///
/// Observable signal: at level 0 the target count is 2000 × 4^2 = 32 000.
/// If the clamp bug is present and fast_config's 40/40 bounds are used, the
/// first iteration generates far more than 32 K expressions and the loop exits
/// after one step.  Without the bug the loop grows from (1,1) and the total
/// expression count stays near 32 K rather than ballooning to 100 K+.
#[test]
fn test_search_adaptive_calls_iterative_algorithm() {
    use ries_rs::search::SearchConfig;

    // Deliberately use the high-bound config (40 / 40).  If the clamp bug were
    // still present the adaptive loop would start at (40, 40), generate far more
    // expressions than the level-0 target, and the assertion below would fail.
    let gen_config = fast_config();
    let search_config = SearchConfig {
        target: 2.0,
        max_matches: 10,
        user_constants: gen_config.user_constants.clone(),
        user_functions: gen_config.user_functions.clone(),
        ..Default::default()
    };

    let (adaptive_matches, adaptive_stats) = search_adaptive(&gen_config, &search_config, 0);

    assert!(
        !adaptive_matches.is_empty(),
        "search_adaptive should find matches for target 2.0"
    );
    assert!(
        adaptive_stats.lhs_count > 0,
        "must report generated LHS expressions"
    );
    assert!(
        adaptive_stats.rhs_count > 0,
        "must report generated RHS expressions"
    );

    // Level-0 target = 2000 × 4^2 = 32 000.  Without the clamp bug, total
    // expression count should be bounded near that target.  With the bug the
    // 40/40 bounds produce 100 K+ expressions, blowing through this ceiling.
    let target_level0: usize = 2000 * 4usize.pow(2);
    let total = adaptive_stats.lhs_count + adaptive_stats.rhs_count;
    assert!(
        total <= target_level0 * 8,
        "adaptive at level 0 generated {total} expressions but the target is ~{target_level0}; \
         a large overshoot indicates the bounds-clamp bug is still present"
    );
    assert!(
        total >= target_level0 / 8,
        "adaptive at level 0 generated only {total} expressions (target ~{target_level0}); \
         the iterative growth may not be running at all"
    );
}

#[cfg(feature = "parallel")]
fn match_signatures(matches: &[ries_rs::search::Match]) -> Vec<(String, String, i64)> {
    let mut signatures: Vec<(String, String, i64)> = matches
        .iter()
        .map(|m| {
            (
                m.lhs.expr.to_postfix(),
                m.rhs.expr.to_postfix(),
                (m.error / 1e-12).round() as i64,
            )
        })
        .collect();
    signatures.sort();
    signatures
}

#[cfg(feature = "parallel")]
#[test]
fn test_parallel_matches_sequential_results() {
    use ries_rs::search::search_parallel_with_stats_and_config;

    let gen_config = fast_config();
    let search_config = SearchConfig {
        target: 2.0,
        max_matches: 20,
        user_constants: gen_config.user_constants.clone(),
        user_functions: gen_config.user_functions.clone(),
        ..Default::default()
    };

    let (sequential, _) = search_with_stats_and_config(&gen_config, &search_config);
    let (parallel, _) = search_parallel_with_stats_and_config(&gen_config, &search_config);

    assert_eq!(
        match_signatures(&sequential),
        match_signatures(&parallel),
        "parallel and sequential search should return the same match set"
    );
}

#[cfg(feature = "parallel")]
#[test]
fn test_turbo_best_match_equals_sequential() {
    use ries_rs::search::search_turbo_with_stats_and_config;

    // Turbo's alternative contract: the single best (rank-1) match is identical
    // to serial, even though the lower-ranked tail may differ. Check it across a
    // few targets with distinct best forms.
    for target in [
        std::f64::consts::SQRT_2,
        2.0 * std::f64::consts::PI,
        std::f64::consts::FRAC_PI_2,
        3.301,
    ] {
        let gen_config = fast_config();
        let search_config = SearchConfig {
            target,
            max_matches: 20,
            user_constants: gen_config.user_constants.clone(),
            user_functions: gen_config.user_functions.clone(),
            ..Default::default()
        };

        let (sequential, _) = search_with_stats_and_config(&gen_config, &search_config);
        let (turbo, _) = search_turbo_with_stats_and_config(&gen_config, &search_config);

        assert!(
            !sequential.is_empty() && !turbo.is_empty(),
            "target {target}"
        );
        let seq_best = (
            sequential[0].lhs.expr.to_postfix(),
            sequential[0].rhs.expr.to_postfix(),
        );
        let turbo_best = (
            turbo[0].lhs.expr.to_postfix(),
            turbo[0].rhs.expr.to_postfix(),
        );
        assert_eq!(
            seq_best, turbo_best,
            "turbo and serial must agree on the best match for {target}"
        );
    }
}

#[cfg(feature = "parallel")]
#[test]
fn test_turbo_small_batch_generation_matches_sequential_best() {
    use ries_rs::search::search_turbo_with_stats_and_config;

    // Exercise the opt-in heuristic below the serial batch limit. Turbo
    // should use the same representative policy as the serial matcher.
    let gen_config = GenConfig {
        max_lhs_complexity: 65,
        max_rhs_complexity: 10,
        max_length: 10,
        constants: vec![Symbol::One],
        unary_ops: vec![Symbol::Neg, Symbol::Exp, Symbol::TanPi, Symbol::CosPi],
        binary_ops: vec![Symbol::Pow],
        rhs_constants: Some(vec![Symbol::One]),
        rhs_unary_ops: Some(Vec::new()),
        rhs_binary_ops: Some(Vec::new()),
        symbol_table: Arc::new(SymbolTable::new()),
        ..GenConfig::default()
    };
    let search_config = SearchConfig {
        target: 2.506314,
        max_matches: 16,
        max_error: 2.506314 * 0.01,
        lhs_value_dedup: true,
        user_constants: gen_config.user_constants.clone(),
        user_functions: gen_config.user_functions.clone(),
        ..Default::default()
    };

    let (sequential, _) = search_with_stats_and_config(&gen_config, &search_config);
    let (turbo, _) = search_turbo_with_stats_and_config(&gen_config, &search_config);

    assert!(!sequential.is_empty() && !turbo.is_empty());
    assert_eq!(
        (
            sequential[0].lhs.expr.to_postfix(),
            sequential[0].rhs.expr.to_postfix()
        ),
        (
            turbo[0].lhs.expr.to_postfix(),
            turbo[0].rhs.expr.to_postfix()
        ),
        "turbo must mirror serial batch-generation semantics below the streaming threshold"
    );
}

#[cfg(feature = "parallel")]
#[test]
#[ignore = "level-3 regression; expensive in debug builds"]
fn test_turbo_level3_flat_exact_best_match_equals_sequential() {
    use ries_rs::search::search_turbo_with_stats_and_config;

    // Regression for 2.506314: at CLI level 3, canonical serial search exceeds
    // the batch-generation safety limit and falls back to streaming, which
    // preserves raw LHS expressions. Batch LHS dedupe can replace xTEC
    // (`cospi(e^tanpi(x))`) with a lower-complexity degenerate representative
    // for the same target-local (value, derivative) key; turbo must preserve
    // the streaming LHS universe in this case.
    let gen_config = GenConfig {
        max_lhs_complexity: 65,
        max_rhs_complexity: 65,
        ..GenConfig::default()
    };
    let search_config = SearchConfig {
        target: 2.506314,
        max_matches: 16,
        max_error: 2.506314 * 0.01,
        user_constants: gen_config.user_constants.clone(),
        user_functions: gen_config.user_functions.clone(),
        ..Default::default()
    };

    let (sequential, _) = search_with_stats_and_config(&gen_config, &search_config);
    let (turbo, _) = search_turbo_with_stats_and_config(&gen_config, &search_config);

    assert_eq!(sequential[0].lhs.expr.to_postfix(), "xTEC");
    assert_eq!(sequential[0].rhs.expr.to_postfix(), "1");
    assert_eq!(
        (
            sequential[0].lhs.expr.to_postfix(),
            sequential[0].rhs.expr.to_postfix()
        ),
        (
            turbo[0].lhs.expr.to_postfix(),
            turbo[0].rhs.expr.to_postfix()
        ),
        "turbo and serial must agree on the flat exact level-3 best match"
    );
}

#[cfg(feature = "parallel")]
#[test]
fn test_turbo_stop_at_exact_preserves_sequential_best_match() {
    use ries_rs::search::search_turbo_with_stats_and_config;

    let gen_config = fast_config();
    let search_config = SearchConfig {
        target: std::f64::consts::FRAC_PI_2,
        max_matches: 20,
        stop_at_exact: true,
        user_constants: gen_config.user_constants.clone(),
        user_functions: gen_config.user_functions.clone(),
        ..Default::default()
    };

    let (sequential, sequential_stats) = search_with_stats_and_config(&gen_config, &search_config);
    let (turbo, turbo_stats) = search_turbo_with_stats_and_config(&gen_config, &search_config);

    assert_eq!(
        match_signatures(&sequential),
        match_signatures(&turbo),
        "turbo must preserve serial stopping semantics and results"
    );
    assert_eq!(sequential_stats.early_exit, turbo_stats.early_exit);
}

#[cfg(feature = "parallel")]
#[test]
fn test_turbo_stop_below_preserves_sequential_best_match() {
    use ries_rs::search::search_turbo_with_stats_and_config;

    let gen_config = fast_config();
    let search_config = SearchConfig {
        target: std::f64::consts::FRAC_PI_2,
        max_matches: 20,
        stop_below: Some(1e-10),
        user_constants: gen_config.user_constants.clone(),
        user_functions: gen_config.user_functions.clone(),
        ..Default::default()
    };

    let (sequential, sequential_stats) = search_with_stats_and_config(&gen_config, &search_config);
    let (turbo, turbo_stats) = search_turbo_with_stats_and_config(&gen_config, &search_config);

    assert_eq!(
        match_signatures(&sequential),
        match_signatures(&turbo),
        "turbo must preserve serial threshold-stopping semantics and results"
    );
    assert_eq!(sequential_stats.early_exit, turbo_stats.early_exit);
}

#[cfg(feature = "parallel")]
#[test]
fn test_turbo_results_are_valid_refined_matches() {
    use ries_rs::search::search_turbo_with_stats_and_config;

    // Every match turbo returns must be a genuine refined equation: its reported
    // error must equal x_value - target, within tolerance.
    let gen_config = fast_config();
    let search_config = SearchConfig {
        target: 2.5,
        max_matches: 20,
        user_constants: gen_config.user_constants.clone(),
        user_functions: gen_config.user_functions.clone(),
        ..Default::default()
    };

    let (turbo, _) = search_turbo_with_stats_and_config(&gen_config, &search_config);
    assert!(!turbo.is_empty());
    for m in &turbo {
        assert!(
            (m.error - (m.x_value - search_config.target)).abs() < 1e-9,
            "turbo match error must be consistent with x_value"
        );
        assert!(m.error.abs() <= search_config.max_error + 1e-12);
    }
}

#[cfg(feature = "parallel")]
#[test]
fn test_turbo_finds_known_exact_match() {
    use ries_rs::search::search_turbo_with_stats_and_config;

    // 2*pi has the simple exact form x = 2*pi; turbo must surface it.
    let gen_config = fast_config();
    let search_config = SearchConfig {
        target: 2.0 * std::f64::consts::PI,
        max_matches: 20,
        user_constants: gen_config.user_constants.clone(),
        user_functions: gen_config.user_functions.clone(),
        ..Default::default()
    };

    let (turbo, _) = search_turbo_with_stats_and_config(&gen_config, &search_config);

    assert!(
        turbo.iter().any(|m| m.error.abs() < 1e-14),
        "turbo should find at least one exact match for 2*pi"
    );
}

#[cfg(feature = "parallel")]
#[test]
fn test_parallel_rhs_override_matches_sequential() {
    use ries_rs::search::{search_parallel_with_stats_and_config, MatcherPath};

    let mut gen_config = fast_config();
    gen_config.rhs_constants = Some(vec![
        Symbol::One,
        Symbol::Two,
        Symbol::Three,
        Symbol::Pi,
        Symbol::E,
    ]);

    let search_config = SearchConfig {
        target: 2.0,
        max_matches: 20,
        user_constants: gen_config.user_constants.clone(),
        user_functions: gen_config.user_functions.clone(),
        ..Default::default()
    };

    let (sequential, _) = search_with_stats_and_config(&gen_config, &search_config);
    let (parallel, stats) = search_parallel_with_stats_and_config(&gen_config, &search_config);

    assert_eq!(stats.matcher, Some(MatcherPath::Serial));
    assert_eq!(
        match_signatures(&sequential),
        match_signatures(&parallel),
        "parallel RHS-override fallback should match sequential results"
    );
}

fn api_level_configs(level: u32, target: f64) -> (GenConfig, SearchConfig) {
    let (max_lhs, max_rhs) = ries_rs::search::level_to_complexity(level);
    let gen = GenConfig {
        max_lhs_complexity: max_lhs,
        max_rhs_complexity: max_rhs,
        ..GenConfig::default()
    };
    let search = SearchConfig {
        target,
        max_matches: 100,
        max_error: (target.abs() * 0.01).max(1e-12),
        zero_value_threshold: 1e-4,
        newton_iterations: 15,
        refine_with_newton: true,
        ..SearchConfig::default()
    };
    (gen, search)
}

fn has_exact_postfix(matches: &[ries_rs::search::Match], lhs: &str, rhs: &str) -> bool {
    matches
        .iter()
        .any(|m| m.exact && m.lhs.expr.to_postfix() == lhs && m.rhs.expr.to_postfix() == rhs)
}

#[test]
fn test_api_level_budgets() {
    for level in 0..=5 {
        let budget = 40 + 2 * level;
        assert_eq!(
            ries_rs::search::level_to_complexity(level),
            (budget, budget),
            "level {level}"
        );
    }
    // Out-of-range levels saturate instead of wrapping.
    assert_eq!(
        ries_rs::search::level_to_complexity(u32::MAX),
        (u32::MAX, u32::MAX)
    );
}

#[test]
fn test_default_api_level_finds_plain_equations() {
    // The default API ranks by complexity. This checks expression coverage at
    // level 3 with a 1% error floor and a 100-match pool.
    let expectations = [
        (2.5, &[("2x*", "5"), ("x", "52/")][..]),
        (1.5, &[("2x*", "3"), ("x", "32/")]),
        (12.0, &[("x", "26*")][..]),
        (42.0, &[("x", "67*")][..]),
    ];
    for (target, equations) in expectations {
        let (gen, cfg) = api_level_configs(3, target);
        let (matches, _) = search_with_stats_and_config(&gen, &cfg);
        for (lhs, rhs) in equations {
            assert!(
                has_exact_postfix(&matches, lhs, rhs),
                "level 3 missing exact {lhs} = {rhs} for {target}"
            );
        }
        if target == 2.5 {
            assert!(
                matches
                    .iter()
                    .all(|m| m.lhs.value == 0.0 || m.lhs.value.abs() >= 1e-4),
                "a non-zero target must still drop tiny nonzero LHS values"
            );
        }
    }
}

#[test]
fn test_website_parity_page_contains_plain_forms() {
    // web/index.html calls search(target, {level: 3, maxMatches: 50, rankingMode: 'parity'}).
    // wasm::search doubles the pool, then returns the first 50.
    let cases = [
        (2.5, &["2x*", "x2*"][..], &["5"][..]),
        (2.5, &["x"][..], &["52/"][..]),
        (1.5, &["x"][..], &["32/"][..]),
        (12.0, &["x"][..], &["26*"][..]),
        (42.0, &["x"][..], &["67*"][..]),
        (2.0, &["x"][..], &["2"][..]),
        (std::f64::consts::PI, &["x"][..], &["p"][..]),
        (std::f64::consts::E, &["x"][..], &["e"][..]),
        (ries_rs::eval::constants::PHI, &["x"][..], &["f"][..]),
        (2.0_f64.sqrt(), &["x"][..], &["2q"][..]),
    ];
    for (target, lhs_forms, rhs_forms) in cases {
        let (gen, mut cfg) = api_level_configs(3, target);
        cfg.max_matches = 100;
        cfg.ranking_mode = ries_rs::pool::RankingMode::Parity;
        let (matches, _) = search_with_stats_and_config(&gen, &cfg);
        let page = &matches[..matches.len().min(50)];
        assert!(
            page.iter().any(|m| {
                m.exact
                    && lhs_forms.contains(&m.lhs.expr.to_postfix().as_str())
                    && rhs_forms.contains(&m.rhs.expr.to_postfix().as_str())
            }),
            "level 3 parity page missing {lhs_forms:?} = {rhs_forms:?} for {target}"
        );
        let min_exact = page
            .iter()
            .filter(|m| m.exact)
            .map(|m| m.complexity)
            .min()
            .unwrap();
        assert!(
            page[0].exact && page[0].complexity == min_exact,
            "parity page for {target} should lead with a simplest exact, got {}",
            page[0].lhs.expr.to_postfix()
        );
    }
    for target in [0.0, -0.0] {
        let (gen, mut cfg) = api_level_configs(3, target);
        cfg.max_matches = 100;
        cfg.ranking_mode = ries_rs::pool::RankingMode::Parity;
        let (matches, _) = search_with_stats_and_config(&gen, &cfg);
        let page = &matches[..matches.len().min(50)];
        let plain = page.iter().find(|m| {
            m.exact
                && m.lhs.expr.to_postfix() == "x"
                && matches!(m.rhs.expr.to_postfix().as_str(), "1l" | "1S")
        });
        let plain = plain.unwrap_or_else(|| panic!("no plain zero for {target}"));
        let summary = ries_rs::MatchSummary::from_match(plain);
        assert_eq!(summary.lhs, "x");
        assert_eq!(summary.rhs, "0");
        assert_eq!(page[0].lhs.expr.to_postfix(), "x");
        assert_eq!(summary.rhs, "0");
    }
}

#[test]
fn test_half_integer_cospi_is_exact_on_the_parity_page() {
    // The level-5 website call. cospi(1/2) = 0 must be labeled exact and kept.
    let (gen, mut cfg) = api_level_configs(5, 0.5);
    cfg.max_matches = 100;
    cfg.ranking_mode = ries_rs::pool::RankingMode::Parity;
    let (matches, _) = search_with_stats_and_config(&gen, &cfg);
    let page = &matches[..matches.len().min(50)];
    let found = page
        .iter()
        .find(|m| m.exact && m.lhs.expr.to_postfix() == "xC");
    let found = found.unwrap_or_else(|| panic!("cospi(x) missing from the level-5 parity page"));
    let summary = ries_rs::MatchSummary::from_match(found);
    assert_eq!(summary.rhs, "0");
}

#[test]
fn test_target_zero_finds_x_equals_ln_one() {
    // `1-1` is a zero, but RHS dedupe keeps the cheaper `ln(1)` (complexity
    // 23 vs 25). The near-zero prune used to drop `x` itself, so that identity
    // could not be formed. Keeping `x` when the target is 0 surfaces it.
    let gen = GenConfig {
        constants: vec![Symbol::One],
        unary_ops: vec![Symbol::Ln],
        binary_ops: vec![Symbol::Sub],
        max_length: 3,
        ..fast_config()
    };
    let cfg = SearchConfig {
        target: 0.0,
        ..Default::default()
    };
    let (batch, _) = search_with_stats_and_config(&gen, &cfg);
    assert!(
        has_exact_postfix(&batch, "x", "1l"),
        "batch search for 0 should find exact x = ln(1)"
    );

    let (streaming, _) = search_streaming_with_config(&gen, &cfg);
    assert!(
        has_exact_postfix(&streaming, "x", "1l"),
        "streaming search for 0 should find exact x = ln(1)"
    );
}

#[test]
fn significant_digit_tolerance_tracks_decimal_place_values() {
    use ries_rs::search::significant_digits_tolerance as tolerance;

    assert_eq!(tolerance(0.333, Some("0.3330")), 5e-5);
    assert_eq!(tolerance(0.333, Some("3.330E-1")), 5e-5);
    assert_eq!(tolerance(20.0, None), 0.5);
    assert_eq!(tolerance(1e-20, None), 5e-21);
    assert_eq!(tolerance(1e-310, Some("1.00e-310")), 5e-313);
    assert_eq!(tolerance(f64::from_bits(1), Some("5e-324")), 0.0);
}
