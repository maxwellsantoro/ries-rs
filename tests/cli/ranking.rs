use super::*;

#[test]
fn default_report_surfaces_simple_forms_of_rounded_constants() {
    for (target, expected) in [
        ("1.41421356", &["x = sqrt(2)", "x^2 = 2"][..]),
        ("3.14159265", &["x = pi"][..]),
    ] {
        let (stdout, _) = run_ries(&[target]);
        assert!(!stdout.contains("('exact' match)"), "{target}\n{stdout}");
        let best = stdout.split_once("-- Best approximations").unwrap().1;
        let first = best.lines().find(|line| line.contains("for x")).unwrap();
        assert!(first.trim().starts_with(expected[0]), "{target}\n{stdout}");
        let elegant = stdout.split_once("-- Elegant/efficient").unwrap().1;
        let equations: Vec<_> = elegant
            .lines()
            .skip(2)
            .take(expected.len())
            .map(|row| {
                let (lhs, rhs) = row.split_once('=').unwrap();
                let rhs = rhs.split_once("for x").unwrap().0.trim();
                format!("{} = {rhs}", lhs.trim())
            })
            .collect();
        assert_eq!(equations, expected, "{stdout}");
    }
}

#[test]
fn test_parity_ranking_flag_is_accepted() {
    let (stdout, _stderr) = run_ries(&[
        "2.5",
        "--report",
        "false",
        "--parity-ranking",
        "-n",
        "2",
        "-l",
        "1",
    ]);
    assert!(
        stdout.contains("Search completed"),
        "expected --parity-ranking to run successfully\n{}",
        stdout
    );
}

#[test]
fn test_parity_ranking_changes_first_match_for_some_target() {
    let targets = ["2.5", "2.5063", "1.6180339887", "0.9159655942", "3.2"];

    let mut changed = false;
    let mut samples = Vec::new();

    for target in targets {
        let (base_stdout, _stderr) = run_ries(&[target, "--report", "false", "-n", "6", "-l", "1"]);
        let (parity_stdout, _stderr) = run_ries(&[
            target,
            "--report",
            "false",
            "--parity-ranking",
            "-n",
            "6",
            "-l",
            "1",
        ]);

        let base_first = parse_first_match_line(&base_stdout).unwrap_or_default();
        let parity_first = parse_first_match_line(&parity_stdout).unwrap_or_default();
        if !base_first.is_empty() && !parity_first.is_empty() {
            samples.push(format!(
                "target={} | base={} | parity={}",
                target, base_first, parity_first
            ));
        }
        if base_first != parity_first {
            changed = true;
            break;
        }
    }

    assert!(
        changed,
        "expected --parity-ranking to alter first match ordering for at least one benchmark target\n{}",
        samples.join("\n")
    );
}

#[test]
fn test_classic_defaults_to_parity_ranking() {
    let target = "2.5063";
    let args_base = [
        target,
        "--classic",
        "--report",
        "false",
        "-n",
        "6",
        "-l",
        "1",
    ];

    let (classic_stdout, _stderr) = run_ries(&args_base);
    let classic_first = parse_first_match_line(&classic_stdout).unwrap_or_default();

    let mut parity_args: Vec<&str> = args_base.to_vec();
    parity_args.push("--parity-ranking");
    let (parity_stdout, _stderr) = run_ries(&parity_args);
    let parity_first = parse_first_match_line(&parity_stdout).unwrap_or_default();

    assert_eq!(
        classic_first, parity_first,
        "expected classic mode default ordering to match --parity-ranking\nclassic:\n{}\nparity:\n{}",
        classic_stdout, parity_stdout
    );
}

#[test]
fn test_complexity_ranking_overrides_classic_default() {
    // Near 1.2, matches within the classic stop bound share an error, so the
    // ranking tie-break decides their order: parity puts `x = apery` last.
    let target = "1.2";
    let args_base = [
        target,
        "--classic",
        "--report",
        "false",
        "-n",
        "6",
        "-l",
        "2",
    ];

    let (classic_stdout, _stderr) = run_ries(&args_base);
    let classic_lines = parse_match_lines(&classic_stdout);

    let mut complexity_args: Vec<&str> = args_base.to_vec();
    complexity_args.push("--complexity-ranking");
    let (complexity_stdout, _stderr) = run_ries(&complexity_args);
    let complexity_lines = parse_match_lines(&complexity_stdout);

    assert_ne!(
        classic_lines, complexity_lines,
        "expected --complexity-ranking to override classic default parity ranking\nclassic:\n{}\ncomplexity:\n{}",
        classic_stdout, complexity_stdout
    );
}

#[test]
fn test_ranking_flags_conflict() {
    let output = run_ries_raw(&[
        "2.5",
        "--report",
        "false",
        "--parity-ranking",
        "--complexity-ranking",
        "-n",
        "1",
    ]);
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    assert!(
        !output.status.success(),
        "expected conflicting ranking flags to fail"
    );
    assert!(
        stderr.contains("cannot be used with") || stderr.contains("conflicts with"),
        "expected clap conflict error for ranking flags\n{}",
        stderr
    );
}

#[test]
fn zero_target_prints_a_literal_zero() {
    for options in [&[][..], &["--report", "false"][..], &["-s"][..]] {
        let mut args = vec!["0", "--deterministic"];
        args.extend_from_slice(options);
        let (stdout, _) = run_ries(&args);
        let first = parse_first_match_line(&stdout).expect("zero match");
        assert!(first.starts_with("x = 0 "), "{stdout}");
        if options.is_empty() {
            let exact = stdout
                .split_once("-- Exact matches")
                .unwrap()
                .1
                .split_once("-- Elegant/efficient")
                .unwrap()
                .0;
            assert_eq!(
                parse_match_lines(exact)
                    .iter()
                    .filter(|row| row.starts_with("x = 0 "))
                    .count(),
                1,
                "{stdout}"
            );
        }
    }
}

#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "default-level performance regression; run in release mode"
)]
fn tiny_targets_keep_newton_work_bounded() {
    use std::process::Stdio;
    use std::time::{Duration, Instant};
    for target in ["1e-15", "-1e-15", "1e-300"] {
        // Redirect to files so output cannot fill a pipe while polling the deadline.
        let path =
            std::env::temp_dir().join(format!("ries-tiny-{}-{target}.txt", std::process::id()));
        let output = std::fs::File::create(&path).unwrap();
        let mut child = Command::new(env!("CARGO_BIN_EXE_ries-rs"))
            .args(["--deterministic", "--stats", "--", target])
            .stdout(output)
            .stderr(Stdio::inherit())
            .spawn()
            .unwrap();
        let start = Instant::now();
        let status = loop {
            if let Some(status) = child.try_wait().unwrap() {
                break status;
            }
            if start.elapsed() > Duration::from_secs(30) {
                child.kill().unwrap();
                child.wait().unwrap();
                let _ = std::fs::remove_file(&path);
                panic!("default search for {target} exceeded 30 seconds");
            }
            std::thread::sleep(Duration::from_millis(50));
        };
        let stdout = std::fs::read_to_string(&path).unwrap();
        std::fs::remove_file(&path).unwrap();
        assert!(status.success(), "{target}: {stdout}");
        let calls = parse_stat_value(&stdout, "Newton calls:").unwrap();
        assert!(
            calls < 25_000_000,
            "{target}: {calls} Newton calls\n{stdout}"
        );
        assert!(
            parse_first_match_line(&stdout).is_some(),
            "{target}: {stdout}"
        );
    }
}

fn first_json_equation(args: &[&str]) -> (String, String, u64) {
    let (stdout, _stderr) = run_ries(args);
    let parsed: serde_json::Value = serde_json::from_str(&stdout).expect("valid JSON output");
    let first = &parsed["results"][0];
    (
        first["lhs_postfix"]
            .as_str()
            .expect("lhs_postfix")
            .to_string(),
        first["rhs_postfix"]
            .as_str()
            .expect("rhs_postfix")
            .to_string(),
        first["complexity"].as_u64().expect("complexity"),
    )
}

#[test]
fn fast_path_follows_complexity_ranking_over_expensive_user_constant() {
    let base = [
        "3.141592653589793",
        "--classic",
        "--complexity-ranking",
        "--user-constant",
        "40:c:expensive pi:3.141592653589793",
        "-l",
        "1",
        "--json",
    ];
    let expected = ("x".to_string(), "p".to_string(), 29);
    assert_eq!(first_json_equation(&base), expected);
    let mut streaming = base.to_vec();
    streaming.push("--streaming");
    assert_eq!(first_json_equation(&streaming), expected);
}

#[test]
fn classic_stop_returns_the_simplest_exact_equation() {
    // `x = 1/2` spelled `2r` (35) is simpler than `1 2 /` (43); `x = phi^2`
    // (42) is simpler than `x = phi+1` (47). Neither is a certified fast match.
    for (target, rhs, complexity) in [("0.5", "2r", 35), ("2.618033988749895", "fs", 42)] {
        for ranking in ["--parity-ranking", "--complexity-ranking"] {
            assert_eq!(
                first_json_equation(&[target, "--classic", ranking, "--json"]),
                ("x".to_string(), rhs.to_string(), complexity),
                "{target} {ranking}"
            );
        }
    }
}

#[test]
fn root_pruning_keeps_two_index_root_without_sqrt() {
    let (stdout, _stderr) = run_ries(&[
        "1.4142135623730951",
        "-S",
        "x2v",
        "-a",
        "-l",
        "2",
        "--deterministic",
        "--json",
    ]);
    let parsed: serde_json::Value = serde_json::from_str(&stdout).expect("valid JSON output");
    let results = parsed["results"].as_array().expect("results array");
    assert!(
        results
            .iter()
            .any(|r| r["lhs_postfix"] == "x" && r["rhs_postfix"] == "22v"),
        "x = 2nd root of 2 should survive pruning when sqrt is unavailable\n{stdout}"
    );
}

#[test]
fn cli_preserves_stop_winner_across_matcher_modes() {
    let base = [
        "2.12345",
        "-l0",
        "--report=false",
        "-n5",
        "--parity-ranking",
        "--stop-below",
        "5.551115123125783e-15",
        "-X",
        "16:a:first:2.123450000000006",
        "-X",
        "16:b:second:2.1234500000000054",
        "--json",
    ];
    for options in [
        vec![],
        vec!["--deterministic"],
        vec!["--streaming"],
        vec!["--turbo"],
        vec!["--turbo", "--deterministic"],
    ] {
        let mut args = base.to_vec();
        args.extend(options);
        let (stdout, _) = run_ries(&args);
        let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();
        let results = json["results"].as_array().unwrap();
        assert_eq!(results[0]["equation"], "x = b", "{args:?}");
        assert!(results[0]["error"].as_f64().unwrap() < 5.551115123125783e-15);
        assert!(results.iter().any(|m| m["equation"] == "x = a"));
    }
}

#[test]
fn stop_threshold_winner_beats_a_cheaper_nonqualifying_constant() {
    for options in [
        vec![],
        vec!["--deterministic"],
        vec!["--streaming"],
        vec!["--turbo"],
        vec!["--parallel"],
    ] {
        let mut args = vec![
            "1",
            "-l0",
            "-S",
            "x\u{80}\u{81}",
            "--report=false",
            "-n5",
            "--parity-ranking",
            "--stop-below",
            "1e-6",
            "-X",
            "16:a:first:1.0000010000000001",
            "-X",
            "20:b:second:1.000001",
            "--json",
        ];
        args.extend(options);
        let (stdout, _) = run_ries(&args);
        let data: serde_json::Value = serde_json::from_str(&stdout).unwrap();
        assert_eq!(data["results"][0]["equation"], "x = b", "{args:?}");
        assert!(data["results"][0]["error"].as_f64().unwrap() < 1e-6);
    }
}
