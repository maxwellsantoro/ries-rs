//! Compare representative selection with exhaustive matching in a tiny grammar.

#![cfg(not(target_arch = "wasm32"))]

use ries_rs::expr::EvaluatedExpr;
use ries_rs::gen::{generate_streaming, GenConfig, StreamingCallbacks};
use ries_rs::pool::{compare_matches, legacy_parity_score_expr, RankingMode};
use ries_rs::profile::UserConstant;
use ries_rs::search::{
    search_adaptive, search_one_sided_with_stats_and_config, search_streaming_with_config,
    search_with_stats_and_config, ExprDatabase, Match, SearchConfig, SearchStats,
};
use ries_rs::symbol::{NumType, Symbol, Symbol::*};
use std::collections::BTreeMap;

fn raw_expressions(gen: &GenConfig, target: f64) -> (Vec<EvaluatedExpr>, Vec<EvaluatedExpr>) {
    let mut lhs = Vec::new();
    let mut rhs = Vec::new();
    generate_streaming(
        gen,
        target,
        &mut StreamingCallbacks {
            on_lhs: &mut |expr| {
                lhs.push(expr.clone());
                true
            },
            on_rhs: &mut |expr| {
                rhs.push(expr.clone());
                true
            },
        },
    );
    (lhs, rhs)
}

fn digit(symbol: Symbol) -> f64 {
    match symbol {
        One => 1.0,
        Two => 2.0,
        Three => 3.0,
        _ => panic!("unexpected digit {symbol:?}"),
    }
}

// The raw generator defines admissibility. Solve each pair directly, without
// the search matcher, its candidate window, or Newton refinement.
fn solve(lhs: &[Symbol], rhs: f64) -> f64 {
    match lhs {
        [X] => rhs,
        [X, Recip] => 1.0 / rhs,
        [X, X, Add] => rhs / 2.0,
        [X, c, Add] | [c, X, Add] => rhs - digit(*c),
        [X, c, Sub] => rhs + digit(*c),
        [c, X, Sub] => digit(*c) - rhs,
        _ => panic!("unexpected LHS {lhs:?}"),
    }
}

fn exhaustive(lhs: &[EvaluatedExpr], rhs: &[EvaluatedExpr], config: &SearchConfig) -> Vec<Match> {
    let mut matches = Vec::new();
    for l in lhs {
        for r in rhs {
            let root = solve(l.expr.symbols(), r.value);
            let error = root - config.target;
            if error.abs() <= config.max_error {
                matches.push(Match {
                    lhs: l.clone(),
                    rhs: r.clone(),
                    x_value: root,
                    error,
                    complexity: l.expr.complexity() + r.expr.complexity(),
                    // These fixtures use exact equality or gaps well outside the ulp tolerance.
                    exact: l.value == r.value && root == config.target,
                });
            }
        }
    }
    matches.sort_by(|a, b| compare_matches(a, b, config.ranking_mode));
    matches.truncate(config.max_matches);
    matches
}

fn lhs_key(expr: &EvaluatedExpr) -> (u64, u64) {
    (expr.value.to_bits(), expr.derivative.to_bits())
}

fn representatives<K: Ord>(
    expressions: &[EvaluatedExpr],
    key: impl Fn(&EvaluatedExpr) -> K,
) -> Vec<EvaluatedExpr> {
    let mut groups = BTreeMap::<K, Vec<&EvaluatedExpr>>::new();
    for expr in expressions {
        groups.entry(key(expr)).or_default().push(expr);
    }
    let mut kept = Vec::new();
    for group in groups.values() {
        let complexity = group
            .iter()
            .copied()
            .min_by_key(|e| (e.expr.complexity(), e.expr.symbols()))
            .unwrap();
        let parity = group
            .iter()
            .copied()
            .min_by_key(|e| {
                (
                    legacy_parity_score_expr(&e.expr),
                    e.expr.complexity(),
                    e.expr.symbols(),
                )
            })
            .unwrap();
        kept.push(complexity.clone());
        if parity.expr != complexity.expr {
            kept.push(parity.clone());
        }
    }
    kept
}

fn equations(matches: &[Match]) -> Vec<(String, String)> {
    matches
        .iter()
        .map(|m| (m.lhs.expr.to_postfix(), m.rhs.expr.to_postfix()))
        .collect()
}

fn tiny_grammar() -> GenConfig {
    GenConfig {
        max_lhs_complexity: 50,
        max_rhs_complexity: 50,
        max_length: 3,
        constants: vec![One, Two, Three],
        unary_ops: vec![Recip],
        binary_ops: vec![Add, Sub],
        ..Default::default()
    }
}

#[test]
fn two_representatives_preserve_exact_rank_one_but_can_drop_top_k_equations() {
    let (lhs, rhs) = raw_expressions(&tiny_grammar(), 1.0);
    let retained_lhs = representatives(&lhs, lhs_key);
    let retained_rhs = representatives(&rhs, |e| e.value.to_bits());

    let lhs_expr = |postfix| lhs.iter().find(|e| e.expr.to_postfix() == postfix).unwrap();
    assert_eq!(lhs_key(lhs_expr("xr")), lhs_key(lhs_expr("2x-")));
    let value_three: Vec<_> = retained_rhs
        .iter()
        .filter(|e| e.value == 3.0)
        .map(|e| e.expr.to_postfix())
        .collect();
    assert_eq!(value_three, ["3", "12+"]);

    let mut winners = Vec::new();
    for ranking_mode in [RankingMode::Complexity, RankingMode::Parity] {
        let config = SearchConfig {
            target: 1.0,
            max_error: 0.02,
            max_matches: 12,
            ranking_mode,
            ..Default::default()
        };
        let full = equations(&exhaustive(&lhs, &rhs, &config));
        let reduced = equations(&exhaustive(&retained_lhs, &retained_rhs, &config));
        assert_eq!(full[0], reduced[0], "{ranking_mode:?}");
        assert_ne!(full, reduced, "{ranking_mode:?}");
        winners.push(full[0].clone());
    }
    assert_ne!(winners[0], winners[1]);
}

#[test]
fn keeping_the_parity_preferred_rhs_recovers_rank_one_in_the_one_sided_fixture() {
    let gen = tiny_grammar();
    let config = SearchConfig {
        target: 3.0,
        max_error: 0.02,
        max_matches: 1,
        ranking_mode: RankingMode::Parity,
        ..Default::default()
    };
    let (mut lhs, rhs) = raw_expressions(&gen, config.target);
    lhs.retain(|e| e.expr.symbols() == [X]);
    let full = exhaustive(&lhs, &rhs, &config);
    let reduced = exhaustive(&lhs, &representatives(&rhs, |e| e.value.to_bits()), &config);
    let (current, _) = search_one_sided_with_stats_and_config(&gen, &config);
    assert_eq!(full[0].rhs.expr.to_postfix(), "12+");
    assert_eq!(equations(&full), equations(&reduced));
    assert_eq!(equations(&current), equations(&full));
}

#[test]
fn search_modes_choose_the_best_admissible_rhs_representative() {
    type Search = fn(&GenConfig, &SearchConfig) -> (Vec<Match>, SearchStats);
    let modes: &[(&str, Search)] = &[
        ("batch", search_with_stats_and_config),
        ("streaming", search_streaming_with_config),
        ("one-sided", search_one_sided_with_stats_and_config),
        ("adaptive", |gen, config| search_adaptive(gen, config, 0)),
        #[cfg(feature = "parallel")]
        (
            "parallel",
            ries_rs::search::search_parallel_with_stats_and_config,
        ),
        #[cfg(feature = "parallel")]
        ("turbo", ries_rs::search::search_turbo_with_stats_and_config),
    ];
    let gen = tiny_grammar();
    for (ranking_mode, excluded, expected) in [
        (RankingMode::Complexity, None, "3"),
        (RankingMode::Parity, None, "12+"),
        (RankingMode::Complexity, Some(Three), "12+"),
        (RankingMode::Parity, Some(Add), "3"),
    ] {
        let config = SearchConfig {
            target: 3.0,
            max_error: 0.02,
            max_matches: 64,
            ranking_mode,
            rhs_excluded_symbols: excluded.map(|symbol| [symbol as u8].into()),
            ..Default::default()
        };
        for (mode, search) in modes {
            let (matches, _) = search(&gen, &config);
            let rhs: Vec<_> = matches
                .iter()
                .filter(|m| m.exact && m.lhs.expr.symbols() == [X])
                .map(|m| m.rhs.expr.to_postfix())
                .collect();
            assert_eq!(
                rhs,
                [expected],
                "{mode}, {ranking_mode:?}, excluded {excluded:?}"
            );
        }
    }
}

#[test]
fn equal_lhs_keys_can_hide_the_best_refined_match_in_parity_mode() {
    let gen = GenConfig {
        constants: vec![Two],
        binary_ops: vec![Sub],
        rhs_constants: Some(vec![
            UserConstant0,
            UserConstant1,
            UserConstant2,
            UserConstant3,
        ]),
        rhs_unary_ops: Some(vec![]),
        rhs_binary_ops: Some(vec![]),
        // Distant RHS values let the adaptive search grow its LHS bound far
        // enough to generate both colliding expressions.
        user_constants: [0.99, 11.0, 13.0, 17.0]
            .into_iter()
            .enumerate()
            .map(|(i, value)| UserConstant {
                weight: 16,
                name: format!("u{i}"),
                description: "refinement fixture".into(),
                value,
                num_type: NumType::Rational,
            })
            .collect(),
        ..tiny_grammar()
    };
    let config = SearchConfig {
        target: 1.0,
        max_error: 0.1,
        max_matches: 1,
        ranking_mode: RankingMode::Parity,
        user_constants: gen.user_constants.clone(),
        ..Default::default()
    };
    let (lhs, rhs) = raw_expressions(&gen, config.target);
    let full = exhaustive(&lhs, &rhs, &config);
    let reduced = exhaustive(&representatives(&lhs, lhs_key), &rhs, &config);
    assert_eq!(full[0].lhs.expr.to_postfix(), "2x-");
    assert_eq!(reduced[0].lhs.expr.to_postfix(), "x");

    // Holding the production matcher fixed isolates the effect of deduplication.
    let mut db = ExprDatabase::new();
    db.insert_rhs(rhs);
    let unpruned = db.find_matches(&lhs, &config);
    assert_eq!(equations(&unpruned), equations(&full));
    type Search = fn(&GenConfig, &SearchConfig) -> (Vec<Match>, SearchStats);
    let materialized: &[(&str, Search)] = &[
        ("batch", search_with_stats_and_config),
        ("adaptive", |gen, config| search_adaptive(gen, config, 0)),
        #[cfg(feature = "parallel")]
        (
            "parallel",
            ries_rs::search::search_parallel_with_stats_and_config,
        ),
        #[cfg(feature = "parallel")]
        ("turbo", ries_rs::search::search_turbo_with_stats_and_config),
    ];
    let heuristic = SearchConfig {
        lhs_value_dedup: true,
        ..config.clone()
    };
    for (mode, search) in materialized {
        let (matches, _) = search(&gen, &config);
        assert_eq!(equations(&matches), equations(&full), "{mode} default");
        let (matches, _) = search(&gen, &heuristic);
        assert_eq!(equations(&matches), equations(&reduced), "{mode} heuristic");
    }
    for config in [&config, &heuristic] {
        let (matches, _) = search_streaming_with_config(&gen, config);
        assert_eq!(equations(&matches), equations(&full), "streaming");
    }
}
