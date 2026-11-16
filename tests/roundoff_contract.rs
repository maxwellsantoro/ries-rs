//! Reproductions from PR #19, against the ordinary f64 search engine.
use ries_rs::eval::evaluate_with_context;
use ries_rs::expr::{EvaluatedExpr, Expression};
use ries_rs::gen::GenConfig;
use ries_rs::search::{search_streaming_with_config, search_with_stats_and_config};
use ries_rs::search::{ExprDatabase, Match, SearchConfig, SearchContext};
use ries_rs::symbol::Symbol;
use ries_rs::{NumType, UserConstant};

fn pair(lhs: &str, rhs: &str, target: f64, constants: Vec<UserConstant>) -> Vec<Match> {
    let config = SearchConfig {
        target,
        max_matches: 10,
        max_error: (target.abs() * 0.01).max(1e-12),
        user_constants: constants,
        ..Default::default()
    };
    pair_with_config(lhs, rhs, &config)
}

fn pair_with_config(lhs: &str, rhs: &str, config: &SearchConfig) -> Vec<Match> {
    let target = config.target;
    let context = SearchContext::new(config);
    let evaluated = |postfix| {
        let expr = Expression::parse(postfix).unwrap();
        let result = evaluate_with_context(&expr, target, &context.eval).unwrap();
        EvaluatedExpr::new(expr, result.value, result.derivative, result.num_type)
    };
    let lhs = evaluated(lhs);
    let mut db = ExprDatabase::new();
    db.insert_rhs(vec![evaluated(rhs)]);
    db.find_matches_with_context(&[lhs], &context)
}

fn constant(value: f64) -> UserConstant {
    UserConstant {
        name: "c".into(),
        description: String::new(),
        value,
        weight: 1,
        num_type: NumType::Rational,
    }
}

#[test]
fn amplified_rounding_does_not_receive_an_exact_label() {
    for (lhs, rhs, target) in [
        ("xrC", "8rC", 1e-15),
        ("xrC", "8rC", -1e-15),
        ("px+C", "8rCn", 1e15),
        ("xrSE", "1", 1e-12),
        ("xxEv", "1", 1e-300),
    ] {
        let results = pair(lhs, rhs, target, vec![]);
        assert!(
            !results.iter().any(|m| m.exact),
            "false exact {lhs}={rhs} at {target}: {results:?}"
        );
    }
    let crossing = pair("1x1-\u{80}-+1-1nA", "p", 1.0, vec![constant(1e-20)]);
    assert!(
        !crossing.iter().any(|m| m.exact),
        "atan2 crossed its branch cut: {crossing:?}"
    );
    let w = pair(
        "xW",
        "\u{80}",
        0.0005,
        vec![constant(0.000_499_750_187_333_331_2)],
    );
    assert!(
        !w.is_empty(),
        "the approximate W candidate should remain available"
    );
    assert!(
        w.iter().all(|m| !m.exact),
        "W solver tolerance is not ulp accuracy"
    );
}

#[test]
fn generated_batch_and_streaming_keep_valid_zeros_and_reject_amplified_exactness() {
    let gen = GenConfig {
        max_lhs_complexity: 40,
        max_rhs_complexity: 40,
        ..Default::default()
    };
    for search in [search_with_stats_and_config, search_streaming_with_config] {
        let config = SearchConfig {
            target: 0.5,
            max_matches: 100,
            ..Default::default()
        };
        let (rows, _) = search(&gen, &config);
        assert!(rows
            .iter()
            .any(|m| m.lhs.expr.to_postfix() == "xC" && m.rhs.value == 0.0 && m.exact));
        let config = SearchConfig {
            target: 1e-15,
            max_matches: 200,
            max_error: 1e-12,
            ..Default::default()
        };
        let (rows, _) = search(&gen, &config);
        let suspect: Vec<_> = rows
            .iter()
            .filter(|m| m.lhs.expr.to_postfix() == "xrC" && m.rhs.expr.to_postfix() == "8rC")
            .collect();
        assert!(!suspect.is_empty(), "retain the approximate candidate");
        assert!(suspect.iter().all(|m| !m.exact));

        let target = f64::from_bits(1);
        let restricted = GenConfig {
            constants: vec![Symbol::UserConstant0],
            unary_ops: vec![],
            binary_ops: vec![],
            user_constants: vec![constant(target)],
            ..gen.clone()
        };
        let config = SearchConfig {
            target,
            user_constants: restricted.user_constants.clone(),
            ..Default::default()
        };
        let (rows, _) = search(&restricted, &config);
        assert!(rows
            .iter()
            .any(|m| m.lhs.expr.to_postfix() == "x" && m.exact && m.error == 0.0));
    }
}

#[test]
fn half_integer_cosine_zero_survives_the_default_near_zero_prune() {
    let results = pair("xC", "1l", 0.5, vec![]);
    assert!(
        results.iter().any(|m| m.exact),
        "missing cospi(1/2)=0: {results:?}"
    );
}

#[test]
fn subnormal_targets_can_match_an_identical_configured_constant() {
    for target in [f64::from_bits(1), f64::from_bits(100), 1e-310, -1e-310] {
        let results = pair("x", "\u{80}", target, vec![constant(target)]);
        assert!(
            results.iter().any(|m| m.exact && m.error == 0.0),
            "missing x=c at {target}: {results:?}"
        );
    }
}

#[test]
fn disabling_zero_filtering_retains_nearby_inexact_trig_roots() {
    let config = SearchConfig {
        target: 1.0,
        max_error: 1e-3,
        zero_value_threshold: 0.0,
        user_constants: vec![constant(1e-6)],
        ..Default::default()
    };
    let results = pair_with_config("xS", "\u{80}", &config);
    assert!(results
        .iter()
        .any(|m| !m.exact && (m.x_value - (1.0 - 1e-6 / std::f64::consts::PI)).abs() < 1e-12));
}
