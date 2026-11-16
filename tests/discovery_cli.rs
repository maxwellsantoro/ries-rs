#![cfg(not(target_arch = "wasm32"))]

use serde_json::Value;
use std::process::Command;

fn run(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_ries-rs"))
        .args(args)
        .output()
        .expect("run discovery CLI")
}

#[test]
fn discovery_cli_keeps_literal_and_finds_simple_pi_before_truncation() {
    let output = run(&[
        "3.14159265",
        "--discover",
        "--json",
        "--deterministic",
        "-l3",
        "-n1",
    ]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let data: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(data["schema"], "ries-discovery-v1");
    assert_eq!(data["target_literal"], "3.14159265");
    assert_eq!(data["hypotheses"].as_array().unwrap().len(), 1);
    assert_eq!(
        data["hypotheses"][0]["representative"]["solve_for_x"],
        "x = pi"
    );
    assert_eq!(data["hypotheses"][0]["within_input_precision"], true);
    assert_eq!(data["config"]["max_lhs_complexity"], 46);
}

#[test]
fn discovery_cli_does_not_silently_ignore_raw_search_configuration() {
    for extra in ["--classic", "--streaming", "--report=false", "--algebraic"] {
        let output = run(&["3.14159265", "--discover", "--json", extra]);
        assert_eq!(output.status.code(), Some(2), "{extra}");
        assert!(output.stdout.is_empty(), "{extra}");
        assert!(
            String::from_utf8_lossy(&output.stderr).contains("does not accept"),
            "{extra}"
        );
    }
    for args in [
        vec!["--discover", "--json"],
        vec!["2", "--discover", "-l2.5"],
        vec!["1e-400", "--discover", "--json"],
        vec!["2", "--discover", "-n0"],
    ] {
        let output = run(&args);
        assert_eq!(output.status.code(), Some(2), "{args:?}");
        assert!(output.stdout.is_empty(), "{args:?}");
    }
}

#[test]
fn discovery_cli_exports_explicit_unavailable_verification_and_plain_text() {
    let output = run(&[
        "1.41421356",
        "--discover",
        "--json",
        "--deterministic",
        "--precision=53",
        "--allow-unverified",
    ]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let data: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        data["hypotheses"][0]["validation"]["high_precision"]["status"],
        "unavailable"
    );
    let output = run(&["1.41421356", "--discover", "--deterministic"]);
    assert!(output.status.success());
    let text = String::from_utf8(output.stdout).unwrap();
    assert!(text.contains("Numerical hypotheses for 1.41421356"));
    assert!(text.contains("x = sqrt(2)"));
    assert!(text.contains("not a symbolic proof"));
}

#[cfg(feature = "highprec")]
#[test]
fn discovery_cli_requires_explicit_fallback_for_unsupported_highprec_operators() {
    let args = [
        "0.56714329",
        "--discover",
        "--json",
        "--precision=128",
        "--deterministic",
        "-n1",
    ];
    let rejected = run(&args);
    assert_eq!(rejected.status.code(), Some(2));
    assert!(rejected.stdout.is_empty());
    assert!(String::from_utf8_lossy(&rejected.stderr).contains("verification unavailable"));

    let mut allowed_args = args.to_vec();
    allowed_args.push("--allow-unverified");
    let accepted = run(&allowed_args);
    assert!(
        accepted.status.success(),
        "{}",
        String::from_utf8_lossy(&accepted.stderr)
    );
    let data: Value = serde_json::from_slice(&accepted.stdout).unwrap();
    assert_eq!(
        data["hypotheses"][0]["representative"]["solve_for_x"],
        "x = W(1)"
    );
    assert_eq!(
        data["hypotheses"][0]["validation"]["high_precision"]["status"],
        "unavailable"
    );
}
