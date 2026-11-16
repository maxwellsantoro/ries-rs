//! Contracts linking evaluation, structural constraints, and restricted generation.
use ries_rs::eval::{evaluate, evaluate_with_constants, EvalError};
use ries_rs::gen::{expression_respects_constraints, generate_all, ExpressionConstraintOptions};
use ries_rs::{Expression, GenConfig, NumType, Symbol, UserConstant};

fn expression(symbols: &[Symbol]) -> Expression {
    let mut expr = Expression::new();
    for &symbol in symbols {
        expr.push(symbol);
    }
    expr
}

fn eval(postfix: &str, x: f64) -> ries_rs::EvalResult {
    evaluate(&Expression::parse(postfix).unwrap(), x).unwrap()
}

#[test]
fn variable_exponents_and_indices_do_not_inherit_polynomial_types() {
    // Include stationary samples: zero derivative does not prove constancy.
    for x in [0.0, 1.0, 2.5] {
        for postfix in ["2x^", "2xs^", "xx^", "x2v", "xs1+2v"] {
            if x == 0.0 && postfix == "x2v" {
                continue;
            }
            assert_eq!(
                eval(postfix, x).num_type,
                NumType::Transcendental,
                "{postfix} at {x}"
            );
        }
        for postfix in ["xs", "xx*", "x2^"] {
            assert_eq!(eval(postfix, x).num_type, NumType::Integer, "{postfix}");
        }
    }
    let udf = ries_rs::UserFunction::parse("1:f:variable exponent:2@^").unwrap();
    let ctx = ries_rs::EvalContext::from_slices(&[], std::slice::from_ref(&udf));
    let expr = expression(&[Symbol::X, Symbol::UserFunction0]);
    assert_eq!(
        ries_rs::eval::evaluate_with_context(&expr, 2.5, &ctx)
            .unwrap()
            .num_type,
        NumType::Transcendental
    );
}

#[test]
fn equivalent_algebraic_forms_agree_on_values_slopes_and_admissibility() {
    for x in [0.125, 1.0, 2.0, 123.0] {
        let expected = eval("xq", x);
        for postfix in ["x12/^", "2xv"] {
            let actual = eval(postfix, x);
            assert!((actual.value / expected.value - 1.0).abs() < 1e-14);
            assert!((actual.derivative / expected.derivative - 1.0).abs() < 1e-14);
            assert!(actual.num_type >= NumType::Algebraic);
        }
    }
    // Algebraic bases, not only rational bases, are closed under rational powers.
    assert_eq!(eval("f12/^", 0.0).num_type, NumType::Algebraic);
    let opts = ExpressionConstraintOptions {
        rational_trig_args: true,
        ..Default::default()
    };
    for postfix in ["2qS", "212/^S", "22vS"] {
        assert!(!expression_respects_constraints(
            &Expression::parse(postfix).unwrap(),
            opts
        ));
    }
    for postfix in ["22n^S", "221+^S"] {
        assert!(expression_respects_constraints(
            &Expression::parse(postfix).unwrap(),
            opts
        ));
    }
}

#[test]
fn restricted_generation_preserves_rational_powers_and_rejects_variable_powers() {
    let config = GenConfig {
        constants: vec![Symbol::X, Symbol::One, Symbol::Two],
        unary_ops: vec![],
        binary_ops: vec![Symbol::Pow, Symbol::Div, Symbol::Root],
        max_length: 5,
        max_lhs_complexity: 80,
        max_rhs_complexity: 80,
        min_num_type: NumType::Algebraic,
        ..Default::default()
    };
    let generated = generate_all(&config, 2.5);
    assert!(!generated
        .lhs
        .iter()
        .any(|e| matches!(e.expr.to_postfix().as_str(), "2x^" | "xx^" | "x2v")));
    // Isolate the power spelling so equivalent-root deduplication cannot mask it.
    let powers = GenConfig {
        binary_ops: vec![Symbol::Pow, Symbol::Div],
        ..config
    };
    assert!(generate_all(&powers, 2.5)
        .rhs
        .iter()
        .any(|e| e.expr.to_postfix() == "212/^"));
}

#[test]
fn nonzero_subnormal_divisors_are_not_domain_errors() {
    let context_constant = |value| UserConstant {
        weight: 1,
        name: "c".into(),
        description: String::new(),
        value,
        num_type: NumType::Rational,
    };
    let quotient = expression(&[Symbol::X, Symbol::UserConstant0, Symbol::Div]);
    for denominator in [
        f64::from_bits(1),
        f64::MIN_POSITIVE.next_down(),
        f64::MIN_POSITIVE,
        f64::MIN_POSITIVE.next_up(),
    ] {
        let result =
            evaluate_with_constants(&quotient, denominator, &[context_constant(denominator)])
                .unwrap();
        assert_eq!(result.value, 1.0);
        if (1.0 / denominator).is_finite() {
            assert_eq!(result.derivative, 1.0 / denominator);
        }
    }
    for x in [1e-154, -1e-154] {
        let actual = eval("xxs/", x);
        let expected = eval("xr", x);
        assert!(actual.value.is_finite() && actual.derivative.is_finite());
        assert!((actual.value / expected.value - 1.0).abs() < 1e-14);
        assert!((actual.derivative / expected.derivative - 1.0).abs() < 1e-14);
    }
    assert_eq!(
        evaluate_with_constants(&quotient, 1.0, &[context_constant(0.0)]).unwrap_err(),
        EvalError::DivisionByZero
    );
    assert_eq!(
        evaluate(&Expression::parse("xr").unwrap(), 0.0).unwrap_err(),
        EvalError::DivisionByZero
    );
}

#[test]
fn algebraic_search_modes_share_power_eligibility() {
    use ries_rs::search::{
        search_one_sided_with_stats_and_config, search_streaming_with_config,
        search_with_stats_and_config,
    };
    let config = GenConfig {
        constants: vec![Symbol::X, Symbol::One, Symbol::Two],
        unary_ops: vec![],
        binary_ops: vec![Symbol::Pow, Symbol::Div],
        max_length: 5,
        max_lhs_complexity: 80,
        max_rhs_complexity: 80,
        min_num_type: NumType::Algebraic,
        ..Default::default()
    };
    let search_config = ries_rs::SearchConfig {
        target: 2.0_f64.sqrt(),
        max_matches: 100,
        stop_at_exact: false,
        ..Default::default()
    };
    let results = vec![
        search_with_stats_and_config(&config, &search_config),
        search_streaming_with_config(&config, &search_config),
        search_one_sided_with_stats_and_config(&config, &search_config),
        #[cfg(feature = "parallel")]
        ries_rs::search::search_parallel_with_stats_and_config(&config, &search_config),
        #[cfg(feature = "parallel")]
        ries_rs::search::search_turbo_with_stats_and_config(&config, &search_config),
    ];
    for (matches, _) in results {
        assert!(matches
            .iter()
            .any(|m| m.lhs.expr.to_postfix() == "x" && m.rhs.expr.to_postfix() == "212/^"));
        for m in matches {
            assert!(m.lhs.num_type >= NumType::Algebraic && m.rhs.num_type >= NumType::Algebraic);
            assert!(!matches!(m.lhs.expr.to_postfix().as_str(), "2x^" | "xx^"));
        }
    }
}

#[test]
fn power_classification_is_independent_of_the_sample() {
    // A small exhaustive grammar checks negative, zero, and positive constant
    // powers, including compound exponents. No random sampling is needed.
    for base in 2..=9 {
        for exponent in 1..=5 {
            for (suffix, minimum) in [
                (format!("{exponent}^"), NumType::Integer),
                (format!("{exponent}n^"), NumType::Rational),
                (format!("1{exponent}/^"), NumType::Algebraic),
            ] {
                let postfix = format!("{base}{suffix}");
                assert!(eval(&postfix, 0.0).num_type >= minimum, "{postfix}");
                assert!(eval(&postfix, 123.0).num_type >= minimum, "{postfix}");
            }
        }
    }
}

#[test]
fn rhs_search_filters_are_resolved_before_rewrite_pruning() {
    use ries_rs::search::{
        search_one_sided_with_stats_and_config, search_streaming_with_config,
        search_with_stats_and_config,
    };
    use std::collections::HashSet;
    let gen = GenConfig {
        constants: vec![Symbol::X, Symbol::Two],
        unary_ops: vec![Symbol::Sqrt],
        binary_ops: vec![Symbol::Root],
        max_length: 3,
        max_lhs_complexity: 55,
        max_rhs_complexity: 55,
        min_num_type: NumType::Algebraic,
        ..Default::default()
    };
    for allowed in [false, true] {
        let mut config = ries_rs::SearchConfig {
            target: 2.0_f64.sqrt(),
            max_matches: 8,
            stop_at_exact: true,
            ..Default::default()
        };
        if allowed {
            config.rhs_allowed_symbols =
                Some(HashSet::from([Symbol::Two as u8, Symbol::Root as u8]));
        } else {
            config.rhs_excluded_symbols = Some(HashSet::from([Symbol::Sqrt as u8]));
        }
        let results = vec![
            search_with_stats_and_config(&gen, &config),
            search_streaming_with_config(&gen, &config),
            search_one_sided_with_stats_and_config(&gen, &config),
            #[cfg(feature = "parallel")]
            ries_rs::search::search_parallel_with_stats_and_config(&gen, &config),
            #[cfg(feature = "parallel")]
            ries_rs::search::search_turbo_with_stats_and_config(&gen, &config),
        ];
        for (matches, _) in results {
            let first = matches
                .first()
                .expect("RHS-only filter must preserve the second-root spelling");
            assert_eq!(first.lhs.expr.to_postfix(), "x");
            assert_eq!(first.rhs.expr.to_postfix(), "22v");
            assert!(first.exact);
            assert!(matches
                .iter()
                .all(|m| !m.rhs.expr.symbols().contains(&Symbol::Sqrt)));
        }
    }
    assert!(
        gen.unary_ops.contains(&Symbol::Sqrt),
        "resolving RHS must not mutate the caller's grammar"
    );
}

#[cfg(feature = "highprec")]
#[test]
#[allow(clippy::approx_constant)] // The typed, rounded target is the regression fixture.
fn oscillatory_cosine_fixture_has_a_real_nearby_high_precision_root() {
    use ries_rs::{HighPrec, RiesFloat};
    let bits = 192;
    let one = HighPrec::one_with_prec(bits);
    // cos(pi/(pi-x)) = 1 iff 1/(pi-x) is an even integer.
    // This is the branch discovered near the typed target 3.14159.
    let phase = HighPrec::from_str_with_prec("376848", bits);
    let pi = HighPrec::pi_with_prec(bits);
    let root = pi.clone() - one.clone() / phase.clone();
    let recovered = one.clone() / (pi.clone() - root.clone());
    assert!((recovered - phase).abs().to_f64() < 1e-45);
    let residual = (pi.clone() / (pi - root.clone())).cos() - one;
    assert!(residual.abs().to_f64() < 1e-45);
    let reported = 3.141_590_000_000_015_f64;
    let ulp = reported.next_up() - reported;
    assert!(
        (root.clone() - HighPrec::from_f64_with_prec(reported, bits))
            .abs()
            .to_f64()
            <= 8.0 * ulp
    );
    let target = HighPrec::from_f64_with_prec(3.141_59, bits);
    let error = (root - target).abs().to_f64();
    assert!(
        error > 8.0 * ulp && error < 2e-14,
        "this is a nearby approximation, not an exact match: {error}"
    );
}

#[test]
fn oscillatory_cosine_does_not_admit_an_out_of_range_rhs() {
    use ries_rs::expr::EvaluatedExpr;
    use ries_rs::search::{ExprDatabase, SearchConfig};
    let lhs = Expression::parse("px-rC").unwrap();
    let root = std::f64::consts::PI - 1.0 / 376_848.0;
    for offset in -8_i64..=8 {
        let target = f64::from_bits((root.to_bits() as i64 + offset) as u64);
        let lv = evaluate(&lhs, target).unwrap();
        for excess in [1e-9, 1e-8, 1e-7, 1e-6] {
            let constants = [UserConstant {
                weight: 1,
                name: "outside".into(),
                description: String::new(),
                value: 1.0 + excess,
                num_type: NumType::Rational,
            }];
            let mut db = ExprDatabase::new();
            db.insert_rhs(vec![EvaluatedExpr::new(
                expression(&[Symbol::UserConstant0]),
                1.0 + excess,
                0.0,
                NumType::Rational,
            )]);
            let (matches, _) = db.find_matches_with_stats(
                &[EvaluatedExpr::new(
                    lhs.clone(),
                    lv.value,
                    lv.derivative,
                    lv.num_type,
                )],
                &SearchConfig {
                    target,
                    user_constants: constants.to_vec(),
                    max_error: 1e-10,
                    zero_value_threshold: 0.0,
                    ..Default::default()
                },
            );
            assert!(
                matches.is_empty(),
                "cosine cannot equal 1+{excess}, offset={offset}, matches={matches:?}"
            );
        }
    }
}

#[test]
fn impossible_trig_levels_are_rejected_with_and_without_refinement() {
    use ries_rs::search::{search_streaming_with_config, search_with_stats_and_config};
    let constants = vec![UserConstant {
        weight: 1,
        name: "outside".into(),
        description: String::new(),
        value: 1.000_000_001,
        num_type: NumType::Rational,
    }];
    let gen = GenConfig {
        constants: vec![Symbol::Pi, Symbol::X],
        unary_ops: vec![Symbol::Recip, Symbol::CosPi, Symbol::SinPi],
        binary_ops: vec![Symbol::Sub],
        rhs_constants: Some(vec![Symbol::UserConstant0]),
        rhs_unary_ops: Some(vec![]),
        rhs_binary_ops: Some(vec![]),
        user_constants: constants.clone(),
        max_length: 5,
        max_lhs_complexity: 55,
        max_rhs_complexity: 55,
        ..Default::default()
    };
    for refine_with_newton in [false, true] {
        let config = ries_rs::SearchConfig {
            target: std::f64::consts::PI - 1.0 / 376_848.0,
            user_constants: constants.clone(),
            max_error: 1e-10,
            zero_value_threshold: 0.0,
            refine_with_newton,
            ..Default::default()
        };
        for (matches, _) in [
            search_with_stats_and_config(&gen, &config),
            search_streaming_with_config(&gen, &config),
        ] {
            assert!(
                matches.is_empty(),
                "impossible trig level survived, refinement={refine_with_newton}: {matches:?}"
            );
        }
    }
}

#[test]
fn reciprocal_trig_levels_reject_impossible_roots_but_keep_real_ones() {
    use ries_rs::expr::EvaluatedExpr;
    use ries_rs::search::{ExprDatabase, SearchConfig};

    // sec and csc have no real values strictly between -1 and 1. This
    // target used to pass Newton's residual test on the rounded plateau.
    for (postfix, target, sign) in [
        ("xCr", 2.001, 1.0),
        ("xCrn", 2.001, -1.0),
        ("xCnr", 2.001, -1.0),
        ("xSr", 0.501, 1.0),
    ] {
        let lhs = Expression::parse(postfix).unwrap();
        let at = evaluate(&lhs, target).unwrap();
        let lhs = EvaluatedExpr::new(lhs, at.value, at.derivative, at.num_type);
        for (level, possible) in [(1.0_f64.next_down(), false), (1.000_001, true)] {
            let constant = UserConstant {
                weight: 1,
                name: "level".into(),
                description: String::new(),
                value: sign * level,
                num_type: NumType::Rational,
            };
            let mut db = ExprDatabase::new();
            db.insert_rhs(vec![EvaluatedExpr::new(
                expression(&[Symbol::UserConstant0]),
                constant.value,
                0.0,
                constant.num_type,
            )]);
            for refine_with_newton in [false, true] {
                let config = SearchConfig {
                    target,
                    max_error: 0.1,
                    user_constants: vec![constant.clone()],
                    refine_with_newton,
                    ..Default::default()
                };
                let (matches, _) = db.find_matches_with_stats(std::slice::from_ref(&lhs), &config);
                assert_eq!(
                    !matches.is_empty(),
                    possible,
                    "{postfix}, level={}, refinement={refine_with_newton}: {matches:?}",
                    constant.value
                );
            }
        }
    }
}

#[test]
fn every_matcher_rejects_reciprocal_cosine_inside_its_range_gap() {
    use ries_rs::search::{search_streaming_with_config, search_with_stats_and_config};
    let constants = vec![UserConstant {
        weight: 1,
        name: "below_one".into(),
        description: String::new(),
        value: 1.0_f64.next_down(),
        num_type: NumType::Rational,
    }];
    let gen = GenConfig {
        constants: vec![Symbol::X],
        unary_ops: vec![Symbol::CosPi, Symbol::Recip],
        binary_ops: vec![],
        rhs_constants: Some(vec![Symbol::UserConstant0]),
        rhs_unary_ops: Some(vec![]),
        rhs_binary_ops: Some(vec![]),
        max_length: 3,
        max_lhs_complexity: 35,
        max_rhs_complexity: 35,
        user_constants: constants.clone(),
        ..Default::default()
    };
    for refine_with_newton in [false, true] {
        let search = ries_rs::SearchConfig {
            target: 2.001,
            max_error: 0.1,
            user_constants: constants.clone(),
            refine_with_newton,
            ..Default::default()
        };
        #[allow(unused_mut)]
        let mut results = vec![
            search_with_stats_and_config(&gen, &search),
            search_streaming_with_config(&gen, &search),
        ];
        #[cfg(feature = "parallel")]
        results.extend([
            ries_rs::search::search_parallel_with_stats_and_config(&gen, &search),
            ries_rs::search::search_turbo_with_stats_and_config(&gen, &search),
        ]);
        for (matches, _) in results {
            assert!(!matches.iter().any(|m| m.lhs.expr.to_postfix() == "xCr"));
            // The guard must retain the nearby, possible cospi(x) = below_one.
            assert!(matches.iter().any(|m| m.lhs.expr.to_postfix() == "xC"));
        }
    }
}

#[test]
fn every_matcher_checks_squared_trig_ranges_without_losing_real_roots() {
    use ries_rs::search::{search_streaming_with_config, search_with_stats_and_config};

    let above_one = 1.0_f64.next_up();
    let below_one = 1.0_f64.next_down();
    for (target, level, forbidden, required) in [
        (2.001, above_one, vec!["xCs", "xCns", "xCss"], vec!["xCrs"]),
        (2.001, below_one, vec!["xCrs", "xCsr", "xCnrs"], vec!["xCs"]),
        (2.001, -above_one, vec!["xCsn"], vec!["xCrn"]),
        (2.001, -below_one, vec!["xCrsn"], vec!["xCsn"]),
        (2.001, 1.0, vec![], vec!["xCs", "xCrs"]),
        (2.001, -1.0, vec![], vec!["xCsn"]),
        (2.24, 0.5, vec!["xCrs", "xCsr"], vec!["xCs"]),
        (2.24, 2.0, vec!["xCs", "xCns"], vec!["xCrs"]),
    ] {
        let constants = vec![UserConstant {
            weight: 1,
            name: "level".into(),
            description: "squared-trig range boundary control".into(),
            value: level,
            num_type: NumType::Rational,
        }];
        let gen = GenConfig {
            constants: vec![Symbol::X],
            unary_ops: vec![Symbol::CosPi, Symbol::Neg, Symbol::Recip, Symbol::Square],
            binary_ops: vec![],
            rhs_constants: Some(vec![Symbol::UserConstant0]),
            rhs_unary_ops: Some(vec![]),
            rhs_binary_ops: Some(vec![]),
            max_length: 5,
            max_lhs_complexity: 70,
            max_rhs_complexity: 70,
            user_constants: constants.clone(),
            ..Default::default()
        };
        for refine_with_newton in [false, true] {
            let config = ries_rs::SearchConfig {
                target,
                max_error: 0.1,
                max_matches: 2048,
                user_constants: constants.clone(),
                refine_with_newton,
                ..Default::default()
            };
            #[allow(unused_mut)]
            let mut runs = vec![
                ("serial", search_with_stats_and_config(&gen, &config)),
                ("streaming", search_streaming_with_config(&gen, &config)),
            ];
            #[cfg(feature = "parallel")]
            runs.extend([
                (
                    "parallel",
                    ries_rs::search::search_parallel_with_stats_and_config(&gen, &config),
                ),
                (
                    "turbo",
                    ries_rs::search::search_turbo_with_stats_and_config(&gen, &config),
                ),
            ]);
            for (mode, (matches, _)) in runs {
                let expressions: Vec<_> = matches.iter().map(|m| m.lhs.expr.to_postfix()).collect();
                for lhs in &forbidden {
                    assert!(
                        !expressions.iter().any(|postfix| postfix == lhs),
                        "{mode}, refinement={refine_with_newton}: impossible {lhs} = {level}"
                    );
                }
                for lhs in &required {
                    assert!(
                        expressions.iter().any(|postfix| postfix == lhs),
                        "{mode}, refinement={refine_with_newton}: lost real {lhs} = {level}: {expressions:?}"
                    );
                }
            }
        }
    }
}

#[test]
fn equation_complexity_saturates_at_the_public_u32_limit() {
    use ries_rs::search::{
        search_one_sided_with_stats_and_config, search_streaming_with_config,
        search_with_stats_and_config,
    };
    let profile = ries_rs::Profile {
        symbol_weights: std::collections::HashMap::from([(Symbol::X, u32::MAX)]),
        ..Default::default()
    };
    let gen = GenConfig {
        constants: vec![Symbol::Two, Symbol::X],
        unary_ops: vec![],
        binary_ops: vec![],
        max_length: 1,
        max_lhs_complexity: u32::MAX,
        max_rhs_complexity: u32::MAX,
        symbol_table: std::sync::Arc::new(ries_rs::SymbolTable::from_profile(&profile)),
        ..Default::default()
    };
    let search = ries_rs::SearchConfig {
        target: 2.0,
        ..Default::default()
    };
    #[allow(unused_mut)]
    let mut results = vec![
        search_with_stats_and_config(&gen, &search),
        search_streaming_with_config(&gen, &search),
        search_one_sided_with_stats_and_config(&gen, &search),
    ];
    #[cfg(feature = "parallel")]
    results.extend([
        ries_rs::search::search_parallel_with_stats_and_config(&gen, &search),
        ries_rs::search::search_turbo_with_stats_and_config(&gen, &search),
    ]);
    for (matches, _) in results {
        assert_eq!(matches.len(), 1);
        assert!(matches[0].exact);
        assert_eq!(matches[0].complexity, u32::MAX);
    }
}
