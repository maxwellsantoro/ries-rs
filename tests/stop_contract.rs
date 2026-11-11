//! Differential checks for the stop contract shared by every matcher.
//!
//! With `stop_at_exact`, the batch and streaming matchers must report the
//! same first equation, and a certified fast match must equal it. A separate
//! exhaustive analytic oracle checks the winner independently of shared stop
//! logic, generation, and Newton refinement.

#![cfg(not(target_arch = "wasm32"))]

use ries_rs::eval::EvalContext;
use ries_rs::gen::GenConfig;
use ries_rs::pool::RankingMode;
use ries_rs::profile::{Profile, UserConstant};
use ries_rs::search::{
    search_streaming_with_config, search_with_stats_and_config, Match, SearchConfig,
};
use ries_rs::symbol::{NumType, Symbol};
use ries_rs::{find_certified_fast_match, FastMatchConfig, SymbolTable};
use std::collections::HashSet;
use std::sync::Arc;

const TARGETS: &[f64] = &[
    1.0,
    2.0,
    5.0,
    0.5,
    0.25,
    2.0 / 3.0,
    0.75,
    std::f64::consts::PI,
    std::f64::consts::E,
    std::f64::consts::GOLDEN_RATIO,
    std::f64::consts::SQRT_2,
    1.732_050_807_568_877_2,
    std::f64::consts::LN_2,
    2.141_592_653_589_793,
    2.618_033_988_749_895,
    0.367_879_441_171_442_33,
];

fn gen_config(user_constants: Vec<UserConstant>, max_complexity: u32) -> GenConfig {
    let mut constants = Symbol::constants().to_vec();
    constants.push(Symbol::X);
    constants.extend(
        Symbol::ALL
            .into_iter()
            .filter(|sym| sym.user_constant_index().is_some())
            .take(user_constants.len()),
    );
    let profile = Profile {
        constants: user_constants.clone(),
        ..Profile::default()
    };
    GenConfig {
        max_lhs_complexity: max_complexity,
        max_rhs_complexity: max_complexity,
        constants,
        user_constants,
        symbol_table: Arc::new(SymbolTable::from_profile(&profile)),
        ..GenConfig::default()
    }
}

fn search_config(
    gen: &GenConfig,
    target: f64,
    ranking_mode: RankingMode,
    stop_below: Option<f64>,
) -> SearchConfig {
    SearchConfig {
        target,
        max_matches: 16,
        max_error: 1.0,
        stop_at_exact: true,
        // `--classic` sets both stop conditions.
        stop_below,
        ranking_mode,
        user_constants: gen.user_constants.clone(),
        ..Default::default()
    }
}

fn equation(m: &Match) -> (String, String, u32) {
    (
        m.lhs.expr.to_postfix(),
        m.rhs.expr.to_postfix(),
        m.complexity,
    )
}

fn check_agreement(gen: &GenConfig, label: &str) -> usize {
    let mut certified = 0;
    for &target in TARGETS {
        for (ranking_mode, stop_below) in [
            (RankingMode::Complexity, None),
            (RankingMode::Parity, None),
            (RankingMode::Parity, Some(1e-10)),
        ] {
            let config = search_config(gen, target, ranking_mode, stop_below);
            let (batch, _) = search_with_stats_and_config(gen, &config);
            let (streaming, _) = search_streaming_with_config(gen, &config);
            let context = format!("{label}, target {target}, {ranking_mode:?}, {stop_below:?}");
            let batch_first = batch
                .first()
                .unwrap_or_else(|| panic!("{context}: no match"));
            assert!(
                batch_first.exact || stop_below.is_some(),
                "{context}: first match is not exact"
            );
            assert_eq!(
                equation(batch_first),
                equation(streaming.first().expect("streaming match")),
                "{context}: batch and streaming disagree"
            );
            // Every result respects the bound set by the simplest exact match.
            assert!(
                batch.iter().all(|m| m.complexity <= batch_first.complexity),
                "{context}: result past the stop bound"
            );

            let empty = HashSet::new();
            let fast_config = FastMatchConfig {
                excluded_symbols: &empty,
                allowed_symbols: None,
                min_num_type: NumType::Transcendental,
            };
            let eval = EvalContext::from_slices(&gen.user_constants, &gen.user_functions);
            if let Some(fast) = find_certified_fast_match(
                target,
                &eval,
                &fast_config,
                gen,
                ranking_mode,
                stop_below,
                |m| m.error.abs() <= config.max_error,
            ) {
                certified += 1;
                assert_eq!(
                    equation(&fast),
                    equation(batch_first),
                    "{context}: certified fast match differs from search"
                );
            }
        }
    }
    certified
}

#[test]
fn matchers_and_fast_path_agree_on_the_first_exact_match() {
    let certified = check_agreement(&gen_config(Vec::new(), 48), "builtin symbols");
    assert!(
        certified >= 4,
        "only {certified} fast matches were certified"
    );
}

#[test]
fn expensive_user_constant_does_not_beat_a_cheaper_builtin() {
    let expensive_pi = UserConstant {
        weight: 40,
        name: "c".into(),
        description: "expensive pi".into(),
        value: std::f64::consts::PI,
        num_type: NumType::Transcendental,
    };
    let gen = gen_config(vec![expensive_pi], 48);
    check_agreement(&gen, "expensive user constant");

    let config = search_config(&gen, std::f64::consts::PI, RankingMode::Complexity, None);
    let (matches, _) = search_with_stats_and_config(&gen, &config);
    assert_eq!(equation(&matches[0]), ("x".into(), "p".into(), 29));
}

#[test]
fn cheaper_constant_within_stop_below_leads_over_exact_builtin() {
    // `x = c` (27) is inexact but within classic's `stop_below`, so it is the
    // stop match; the fast path must not certify the exact `x = pi` (29).
    let near_pi = UserConstant {
        weight: 12,
        name: "c".into(),
        description: "near pi".into(),
        value: 3.141_592_653_590_793,
        num_type: NumType::Transcendental,
    };
    let gen = gen_config(vec![near_pi], 48);
    check_agreement(&gen, "near-pi user constant");

    let config = search_config(&gen, std::f64::consts::PI, RankingMode::Parity, Some(1e-10));
    let (matches, _) = search_with_stats_and_config(&gen, &config);
    assert_eq!(
        equation(&matches[0]).1,
        Symbol::UserConstant0.to_postfix_char().to_string()
    );
}

fn tiny_value_and_slope(symbols: &[Symbol], target: f64) -> (f64, f64) {
    let atom = |s| {
        if s == Symbol::X {
            (target, 1.0)
        } else {
            (f64::from(s as u8 - b'0'), 0.0)
        }
    };
    match symbols {
        [a] => atom(*a),
        [a, b, op] => {
            let (a, da) = atom(*a);
            let (b, db) = atom(*b);
            match op {
                Symbol::Add => (a + b, da + db),
                Symbol::Mul => (a * b, da * b + a * db),
                _ => unreachable!(),
            }
        }
        _ => unreachable!(),
    }
}

/// Independent oracle: enumerate every length-1/3 expression in a tiny grammar
/// and solve its linear/quadratic LHS analytically. No generator, Newton step,
/// representative policy, or StopBound is used to choose the expected winner.
#[test]
fn stop_matches_equal_an_exhaustive_analytic_oracle() {
    use ries_rs::expr::EvaluatedExpr;
    use ries_rs::Expression;
    // Reproducible randomized grammars, costs and per-side budgets. The seed
    // is fixed so failures replay identically on every platform.
    let mut state = 0x5eed_u64;
    for scenario in 0..18 {
        let mut next = || {
            state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
            (state >> 32) as u32
        };
        let mut atoms = vec![Symbol::X, Symbol::One, Symbol::Two];
        if scenario < 2 || next() % 2 == 0 {
            atoms.push(Symbol::Three);
        }
        if scenario < 2 || next() % 2 == 0 {
            atoms.push(Symbol::Five);
        }
        let ops = match scenario % 3 {
            0 if scenario >= 2 => vec![Symbol::Add],
            1 if scenario >= 2 => vec![Symbol::Mul],
            _ => vec![Symbol::Add, Symbol::Mul],
        };
        let budget = if scenario < 2 { 100 } else { 25 + next() % 26 };
        let mut profile = Profile::default();
        if scenario == 1 {
            profile.symbol_weights.insert(Symbol::Two, 35);
            profile.symbol_weights.insert(Symbol::Add, 4);
            profile.symbol_weights.insert(Symbol::Mul, 7);
        } else if scenario >= 2 {
            for &symbol in atoms.iter().chain(&ops) {
                profile.symbol_weights.insert(symbol, 1 + next() % 20);
            }
        }
        let table = Arc::new(SymbolTable::from_profile(&profile));
        let mut expressions = Vec::new();
        let make = |symbols: &[Symbol]| {
            let mut e = Expression::new();
            for &s in symbols {
                e.push_with_table(s, &table);
            }
            e
        };
        for &a in &atoms {
            expressions.push(make(&[a]));
            for &b in &atoms {
                for &op in &ops {
                    expressions.push(make(&[a, b, op]));
                }
            }
        }
        let gen = GenConfig {
            constants: atoms.to_vec(),
            unary_ops: vec![],
            binary_ops: ops,
            max_length: 3,
            max_lhs_complexity: budget,
            max_rhs_complexity: budget,
            symbol_table: table,
            ..Default::default()
        };
        for target in [0.5, 2.0, 2.5, 2.49, std::f64::consts::SQRT_2] {
            for ranking_mode in [RankingMode::Complexity, RankingMode::Parity] {
                for stop_at_exact in [false, true] {
                    let config = SearchConfig {
                        target,
                        ranking_mode,
                        stop_at_exact,
                        stop_below: (!stop_at_exact).then_some(0.02),
                        max_error: 0.1,
                        max_matches: 1,
                        ..Default::default()
                    };
                    let mut qualifying = Vec::new();
                    for lhs in expressions
                        .iter()
                        .filter(|e| e.contains_x() && e.complexity() <= budget)
                    {
                        for rhs in expressions
                            .iter()
                            .filter(|e| !e.contains_x() && e.complexity() <= budget)
                        {
                            let (rv, _) = tiny_value_and_slope(rhs.symbols(), target);
                            let symbols = lhs.symbols();
                            let root = if symbols.len() == 1 {
                                rv
                            } else {
                                let [a, b, op] = symbols else { unreachable!() };
                                let xs =
                                    usize::from(*a == Symbol::X) + usize::from(*b == Symbol::X);
                                match (*op, xs) {
                                    (Symbol::Add, 2) => rv / 2.0,
                                    (Symbol::Mul, 2) => rv.sqrt(),
                                    (Symbol::Add, 1) | (Symbol::Mul, 1) => {
                                        let constant = if *a == Symbol::X { *b } else { *a };
                                        let c = f64::from(constant as u8 - b'0');
                                        if *op == Symbol::Add {
                                            rv - c
                                        } else {
                                            rv / c
                                        }
                                    }
                                    _ => unreachable!(),
                                }
                            };
                            let error = root - target;
                            let exact = error == 0.0;
                            if error.abs() > config.max_error
                                || !(stop_at_exact && exact
                                    || config.stop_below.is_some_and(|t| error.abs() < t))
                            {
                                continue;
                            }
                            let (lv, slope) = tiny_value_and_slope(lhs.symbols(), target);
                            qualifying.push(Match {
                                lhs: EvaluatedExpr::new(lhs.clone(), lv, slope, NumType::Rational),
                                rhs: EvaluatedExpr::new(rhs.clone(), rv, 0.0, NumType::Rational),
                                x_value: root,
                                error,
                                exact,
                                complexity: lhs.complexity() + rhs.complexity(),
                            });
                        }
                    }
                    let expected = qualifying.iter().min_by(|a, b| {
                        a.complexity
                            .cmp(&b.complexity)
                            .then_with(|| ries_rs::pool::compare_matches(a, b, ranking_mode))
                    });
                    let Some(expected) = expected else {
                        for (actual, _) in [
                            search_with_stats_and_config(&gen, &config),
                            search_streaming_with_config(&gen, &config),
                        ] {
                            assert!(actual.iter().all(|m| !(config.stop_at_exact && m.exact
                                || config.stop_below.is_some_and(|t| m.error.abs() < t))));
                        }
                        continue;
                    };
                    for (actual, _) in [
                        search_with_stats_and_config(&gen, &config),
                        search_streaming_with_config(&gen, &config),
                    ] {
                        assert_eq!(equation(&actual[0]), equation(expected), "target={target}, ranking={ranking_mode:?}, exact={stop_at_exact}, scenario={scenario}, budget={budget}");
                    }
                }
            }
        }
    }
}
