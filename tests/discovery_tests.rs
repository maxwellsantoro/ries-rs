use ries_rs::{discover, DiscoveryOptions, DiscoveryRun};

fn options(max_matches: usize) -> DiscoveryOptions {
    DiscoveryOptions {
        level: 3,
        max_matches,
        ..DiscoveryOptions::default()
    }
}

#[test]
fn rounded_known_answers_survive_even_a_one_hypothesis_limit() {
    for (literal, solved) in [
        ("3.14159265", "x = pi"),
        ("1.41421356", "x = sqrt(2)"),
        ("1.64872127", "x = sqrt(e)"),
    ] {
        let run = discover(literal, &options(1)).unwrap();
        assert_eq!(run.hypotheses.len(), 1, "{literal}");
        let hypothesis = &run.hypotheses[0];
        assert_eq!(
            hypothesis.representative.solve_for_x.as_deref(),
            Some(solved)
        );
        assert!(hypothesis.within_input_precision);
        assert_eq!(
            run.config.effective_total_complexity_bound,
            Some(hypothesis.representative.complexity)
        );
        assert!(hypothesis.representative.complexity < 50);
    }
}

#[test]
fn a_run_preserves_literal_resolved_configuration_and_serializes_evidence() {
    let run = discover(" 1.4142135600 ", &options(8)).unwrap();
    assert_eq!(run.target_literal, " 1.4142135600 ");
    assert_eq!(run.config.input_tolerance, 5e-11);
    assert_eq!(run.config.max_lhs_complexity, 46);
    assert_eq!(run.config.max_rhs_complexity, 46);
    assert!(run.config.grammar.iter().any(|symbol| symbol.symbol == "x"));
    assert!(run.stats.lhs_count > 0);
    let value = serde_json::to_value(&run).unwrap();
    assert_eq!(value["schema"], "ries-discovery-v1");
    assert_eq!(
        value["hypotheses"][0]["validation"]["numerical"]["status"],
        "numerical"
    );
    assert_eq!(
        value["hypotheses"][0]["validation"]["high_precision"]["status"],
        "not_requested"
    );
    assert_eq!(
        value["hypotheses"][0]["validation"]["polynomial_certificate"]["status"],
        "unavailable"
    );
    let restored: DiscoveryRun = serde_json::from_value(value).unwrap();
    assert_eq!(restored.target_literal, run.target_literal);
}

#[test]
fn discovery_is_deterministic_and_parallel_request_reports_actual_matcher() {
    let a = discover("1.41421356", &options(8)).unwrap();
    let b = discover(
        "1.41421356",
        &DiscoveryOptions {
            parallel: true,
            ..options(8)
        },
    )
    .unwrap();
    let signatures = |run: &DiscoveryRun| {
        run.hypotheses
            .iter()
            .map(|hypothesis| {
                (
                    hypothesis.key.clone(),
                    hypothesis.representative.lhs_postfix.clone(),
                    hypothesis.representative.rhs_postfix.clone(),
                    hypothesis.representative.x_value.to_bits(),
                    hypothesis
                        .supporting_equations
                        .iter()
                        .map(|m| (m.lhs_postfix.clone(), m.rhs_postfix.clone()))
                        .collect::<Vec<_>>(),
                )
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(signatures(&a), signatures(&b));
    assert!(b.config.parallel_requested);
    assert_eq!(b.stats.matcher, a.stats.matcher);
    assert_eq!(
        a.hypotheses
            .iter()
            .filter(
                |hypothesis| hypothesis.representative.solve_for_x.as_deref()
                    == Some("x = sqrt(2)")
            )
            .count(),
        1
    );
    assert!(!a.hypotheses[0].supporting_equations.is_empty());
    assert!(a.hypotheses[0]
        .supporting_equations
        .iter()
        .all(
            |support| support.lhs_postfix != a.hypotheses[0].representative.lhs_postfix
                || support.rhs_postfix != a.hypotheses[0].representative.rhs_postfix
        ));
}

#[test]
fn invalid_literals_and_unbounded_options_fail_before_search() {
    for literal in [
        "",
        "NaN",
        "inf",
        "1e-400",
        "9007199254740993",
        "1+2",
        "1.0000000000000000000000000001",
    ] {
        assert!(discover(literal, &options(1)).is_err(), "{literal}");
    }
    assert!(discover("2", &options(0)).is_err());
    assert!(discover("2", &options(1001)).is_err());
    assert!(discover(
        "2",
        &DiscoveryOptions {
            level: 6,
            ..options(1)
        }
    )
    .is_err());
    assert!(discover(
        "2",
        &DiscoveryOptions {
            verification_bits: Some(4097),
            ..options(1)
        }
    )
    .is_err());
    assert!(discover(
        "2",
        &DiscoveryOptions {
            preset: Some("unknown".into()),
            ..options(1)
        }
    )
    .is_err());
}

#[test]
fn requested_unavailable_precision_is_distinct_from_not_requested() {
    let run = discover(
        "2",
        &DiscoveryOptions {
            verification_bits: Some(53),
            ..options(1)
        },
    )
    .unwrap();
    let check = &run.hypotheses[0].validation.high_precision;
    assert_eq!(check.status, "unavailable");
    assert_eq!(check.precision_bits, Some(53));
    assert!(check.root_error.is_none());
    assert!(!check.reason.is_empty());
}

#[test]
fn unresolved_fallback_is_explicit_instead_of_claiming_input_agreement() {
    let run = discover("0.123456789", &options(3)).unwrap();
    assert!(run.config.effective_total_complexity_bound.is_none());
    assert!(!run.hypotheses.is_empty());
    assert!(run
        .hypotheses
        .iter()
        .all(|hypothesis| !hypothesis.within_input_precision));
}

#[cfg(feature = "highprec")]
#[test]
fn requested_supported_precision_corroborates_a_simple_root() {
    let run = discover(
        "1.41421356",
        &DiscoveryOptions {
            verification_bits: Some(128),
            ..options(1)
        },
    )
    .unwrap();
    let check = &run.hypotheses[0].validation.high_precision;
    assert_eq!(check.status, "verified");
    assert!(check.root_error.is_some_and(f64::is_finite));
}
