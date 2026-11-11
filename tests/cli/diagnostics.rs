use super::*;

#[test]
fn test_f1_condensed_format_accepted() {
    let (stdout, _stderr) = run_ries(&[
        "2.5",
        "-F1",
        "--classic",
        "--report",
        "false",
        "--one-sided",
        "-S",
        "x25/",
        "-n",
        "1",
    ]);
    // -F1 should work as an alias for -F0 postfix compact
    // Postfix compact for x = 5/2 would be "x52/" (no spaces)
    assert!(
        stdout.contains("52/") || stdout.contains("x52"),
        "expected -F1 to produce postfix compact output (like -F0)\n{}",
        stdout
    );
}

#[test]
fn test_verbose_output_shows_target() {
    let (stdout, _stderr) = run_ries(&[
        "2.5",
        "--verbose",
        "--report",
        "false",
        "--max-matches",
        "1",
        "-l",
        "0",
    ]);
    assert!(
        stdout.contains("Target:") || stdout.contains("target"),
        "expected --verbose to show target in header\n{}",
        stdout
    );
}

#[test]
fn test_verbose_output_shows_total_equations() {
    let (stdout, _stderr) = run_ries(&[
        "2.5",
        "--verbose",
        "--report",
        "false",
        "--max-matches",
        "1",
        "-l",
        "0",
    ]);
    // Should show total equations tested or similar summary info
    let lower = stdout.to_lowercase();
    assert!(
        lower.contains("total") || lower.contains("equations") || lower.contains("summary"),
        "expected --verbose to show summary with total/equations in footer\n{}",
        stdout
    );
}

/// Slow: `-Do` emits one diagnostic line per match candidate, which produces
/// enormous output and takes ~24 minutes. Run with `cargo test -- --ignored`.
#[test]
#[ignore]
fn test_diagnostic_channel_o_recognized() {
    let (stdout, stderr) = run_ries(&["2.5", "-Do", "--report", "false", "--max-matches", "1"]);
    // -Do should not warn about unsupported channel (checking both "unsupported" and "not implemented")
    let combined = format!("{}{}", stdout, stderr).to_lowercase();
    assert!(
        !combined.contains("unsupported") && !combined.contains("not implemented"),
        "expected -Do to be recognized as valid diagnostic channel, but got:\nstdout:\n{}\nstderr:\n{}",
        stdout,
        stderr
    );
}

/// Slow: same as `test_diagnostic_channel_o_recognized` — `-Do` floods output.
/// Run with `cargo test -- --ignored`.
#[test]
#[ignore]
fn test_diagnostic_o_shows_match_output() {
    let (stdout, stderr) = run_ries(&["2.5", "-Do", "--report", "false", "--max-matches", "1"]);
    // -Do should output match check information to stderr
    let output = format!("{}{}", stdout, stderr).to_lowercase();
    assert!(
        output.contains("match") || output.contains("candidate") || output.contains("check"),
        "expected -Do to show match check output, but got:\nstdout:\n{}\nstderr:\n{}",
        stdout,
        stderr
    );
}

/// Slow: `-Dn` emits one diagnostic line per Newton iteration across all candidates.
/// Run with `cargo test -- --ignored`.
#[test]
#[ignore]
fn test_diagnostic_n_shows_newton_iterations() {
    let (stdout, stderr) = run_ries(&["2.5", "-Dn", "--report", "false", "--max-matches", "1"]);
    // -Dn should show Newton iteration values
    let output = format!("{}{}", stdout, stderr).to_lowercase();
    assert!(
        output.contains("newton") || output.contains("iteration") || output.contains("converg"),
        "expected -Dn to show Newton iteration diagnostic output\nstdout:\n{}\nstderr:\n{}",
        stdout,
        stderr
    );
}

#[test]
fn test_diagnostic_a_recognized() {
    let (_stdout, stderr) = run_ries(&[
        "2.5",
        "-DA",
        "--report",
        "false",
        "--max-matches",
        "1",
        "-l",
        "0",
    ]);
    // -DA should not warn about unsupported channel
    assert!(!stderr.to_lowercase().contains("unsupported"));
}

#[test]
fn test_match_all_digits_option_accepted() {
    // Just verify the option is accepted and doesn't crash
    let (stdout, _) = run_ries(&[
        "2.5",
        "--match-all-digits",
        "--report",
        "false",
        "--max-matches",
        "1",
        "-l",
        "0",
    ]);
    assert!(stdout.contains("x"));
}

#[test]
fn test_derivative_margin_option_accepted() {
    // Just verify the option is accepted and doesn't crash
    let (stdout, _) = run_ries(&[
        "2.5",
        "--derivative-margin",
        "1e-10",
        "--report",
        "false",
        "--max-matches",
        "1",
        "-l",
        "0",
    ]);
    assert!(stdout.contains("x"));
}

#[test]
fn test_diagnostic_g_recognized() {
    let (stdout, stderr) = run_ries(&[
        "2.5",
        "-DG",
        "--report",
        "false",
        "--max-matches",
        "1",
        "-l",
        "0",
    ]);
    // -DG should not warn about unsupported channel
    let combined = format!("{}{}", stdout, stderr).to_lowercase();
    assert!(
        !combined.contains("unsupported") && !combined.contains("not implemented"),
        "expected -DG to be recognized as valid diagnostic channel, but got:\nstdout:\n{}\nstderr:\n{}",
        stdout,
        stderr
    );
}

#[test]
fn test_diagnostic_g_shows_db_add_output() {
    let (stdout, stderr) = run_ries(&[
        "2.5",
        "-DG",
        "--report",
        "false",
        "--max-matches",
        "1",
        "-l",
        "0",
    ]);
    // -DG should output database add information to stderr
    let output = format!("{}{}", stdout, stderr).to_lowercase();
    assert!(
        output.contains("db add") || output.contains("insert") || output.contains("pool"),
        "expected -DG to show database add diagnostic output\nstdout:\n{}\nstderr:\n{}",
        stdout,
        stderr
    );
}

#[test]
fn test_diagnostic_b_recognized() {
    let (_stdout, stderr) = run_ries(&[
        "2.5",
        "-DB",
        "--report",
        "false",
        "--max-matches",
        "1",
        "-l",
        "0",
    ]);
    // -DB should not warn about unsupported channel
    assert!(!stderr.to_lowercase().contains("unsupported"));
}

#[test]
fn test_additional_diagnostic_channels_are_recognized() {
    let (_stdout, stderr) = run_ries(&[
        "2.5",
        "-DCEFHIKL",
        "--report",
        "false",
        "--max-matches",
        "1",
        "-l",
        "0",
    ]);
    let lower = stderr.to_lowercase();
    assert!(
        !lower.contains("unsupported") && !lower.contains("not implemented"),
        "expected compatibility diagnostic channels to be recognized\n{}",
        stderr
    );
}

#[test]
fn test_report_mode_honors_format() {
    // Report mode with -F0 should show postfix format
    let (stdout0, _) = run_ries(&["2.5", "-F0", "--max-matches", "1", "-l", "0"]);
    // Report mode with -F2 should show infix format
    let (stdout2, _) = run_ries(&["2.5", "-F2", "--max-matches", "1", "-l", "0"]);

    // -F0 should show postfix notation (compact postfix like "x52/" without spaces)
    // Classic mode shows "52/" for 5/2, report mode should do the same
    let has_postfix_compact = stdout0.contains("x52/") || stdout0.contains("52/");
    // -F2 should show infix notation (like "5/2" with mathematical operators)
    let has_infix = stdout2.contains("5/2") || stdout2.contains("x = ");

    // Both formats should work correctly
    assert!(
        has_postfix_compact,
        "-F0 should produce postfix compact output in report mode\n-F0 output:\n{}",
        stdout0
    );
    assert!(
        has_infix,
        "-F2 should produce infix output in report mode\n-F2 output:\n{}",
        stdout2
    );
}

#[test]
fn explicit_unverified_fallback_respects_no_slow_messages() {
    let output = run_ries_raw(&[
        "2.5",
        "--report",
        "false",
        "--max-matches",
        "1",
        "--allow-unverified",
        "--precision",
        "256",
        "--no-slow-messages",
        "-l",
        "0",
    ]);
    assert!(output.status.success(), "command should still succeed");
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    assert!(
        !stderr.to_lowercase().contains("warning"),
        "expected --no-slow-messages to suppress compatibility warnings\n{}",
        stderr
    );
}

#[test]
fn test_s_flag_prints_the_branch_in_report_and_classic_modes() {
    // Report mode used to print `x = {rhs}` (`x = (1/7)^2`, `x = -7^2`).
    let (report, _) = run_ries(&[
        "2.506628274631",
        "--deterministic",
        "-s",
        "-n",
        "8",
        "-l",
        "0",
    ]);
    assert!(
        report.contains("x = atan2(1, sqrt(1-1^2))/pi+2"),
        "report -s should print the sine branch near 2.5\n{report}"
    );
    assert!(
        !report.contains("x = (1/7)^2") && !report.contains("x = -7^2"),
        "report -s should not present the raw right-hand side as x\n{report}"
    );

    let (classic, _) = run_ries(&[
        "2.506628274631",
        "--deterministic",
        "--report",
        "false",
        "-s",
        "-n",
        "8",
        "-l",
        "0",
    ]);
    assert!(
        classic.contains("x = atan2("),
        "classic -s should solve for x\n{classic}"
    );
    assert!(
        !classic.contains("x = (1/7)^2") && !classic.contains("x = -7^2"),
        "classic -s should not present the raw right-hand side as x\n{classic}"
    );
}

#[test]
fn test_s_flag_large_period_prints_a_decimal_integer() {
    let (stdout, _) = run_ries(&[
        "2000002.001",
        "--deterministic",
        "--json",
        "-S",
        "xC1",
        "-s",
        "-n",
        "8",
        "-l",
        "1",
        "--report",
        "false",
    ]);
    assert!(
        stdout.contains("\"equation\": \"cospi(x) = 1\""),
        "expected cospi(x) = 1 among the matches\n{stdout}"
    );
    assert!(
        stdout.contains("\"solve_for_x\": \"x = 2000002\""),
        "the period shift should print as a decimal integer\n{stdout}"
    );
    assert!(
        !stdout.contains("\"solve_for_x\": \"x = atan2"),
        "the period shift must not be displayed as the principal inverse\n{stdout}"
    );
}

#[test]
fn test_s_flag_uses_mathematica_and_sympy_formatting() {
    let (mathematica, _) = run_ries(&[
        "1.4142135623730951",
        "--deterministic",
        "--report",
        "false",
        "-s",
        "-n",
        "1",
        "-l",
        "0",
        "--format",
        "mathematica",
        "-S",
        "x2s",
    ]);
    assert!(
        mathematica.contains("x = Sqrt[2]"),
        "mathematica -s should format the solved expression\n{mathematica}"
    );

    let (sympy, _) = run_ries(&[
        "4",
        "--deterministic",
        "--report",
        "false",
        "-s",
        "-n",
        "6",
        "-l",
        "1",
        "--format",
        "sympy",
        "-S",
        "x2q",
    ]);
    assert!(
        sympy.contains("x = 2**2"),
        "sympy -s should format powers with **\n{sympy}"
    );
}

#[test]
fn test_s_flag_solves_supported_equation_forms() {
    // At level 1 for this target, x^2 = 2*pi is a top match and should
    // transform into x = sqrt(2*pi) under -s.
    let (stdout, _stderr) = run_ries(&[
        "2.5063",
        "--classic",
        "--report",
        "false",
        "-l",
        "1",
        "-s",
        "-n",
        "1",
    ]);
    assert!(
        stdout.contains("x ="),
        "expected -s to isolate x for supported equation forms\n{}",
        stdout
    );
}

#[test]
fn test_trig_argument_scale_changes_evaluation() {
    let (default_stdout, _stderr) = run_ries(&["--find-expression", "xS", "--at", "1"]);
    let (scaled_stdout, _stderr) = run_ries(&[
        "--find-expression",
        "xS",
        "--at",
        "1",
        "--trig-argument-scale",
        "1",
    ]);

    assert!(
        default_stdout.contains("Value = 0.000000000000000"),
        "expected default sinpi(1) == 0\n{}",
        default_stdout
    );
    assert!(
        !scaled_stdout.contains("Value = 0.000000000000000"),
        "expected scaled trig argument to change evaluation\n{}",
        scaled_stdout
    );
}

#[test]
fn strict_configuration_rejects_ignored_or_invalid_inputs() {
    for options in [
        vec!["--define", "4:outer:wrapper:H"],
        vec!["-X", "invalid"],
        vec!["--symbol-weights", "garbage"],
        vec!["--symbol-weights", ":pp:25"],
        vec!["--symbol-weights", ":p:bad"],
        vec!["--symbol-names", ":pp:PI"],
        vec!["--max-memory", "bogus"],
        vec!["--memory-abort-threshold", "1.5"],
        vec!["--trig-argument-scale", "0"],
        vec!["-S", "x?"],
    ] {
        let mut args = vec!["2", "--strict-config", "--json", "--classic"];
        args.extend(options);
        let output = run_ries_raw(&args);
        assert_eq!(output.status.code(), Some(2), "{args:?}");
        assert!(
            output.stdout.is_empty(),
            "invalid configuration started a search: {args:?}"
        );
    }
    let (stdout, _) = run_ries(&[
        "2",
        "--strict-config",
        "--json",
        "--classic",
        "--symbol-weights",
        ":p:25 :e:30",
    ]);
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&stdout).unwrap()["results"][0]["equation"],
        "x = 2"
    );
    // Permissive compatibility mode remains available.
    let (_, stderr) = run_ries(&["2", "--classic", "--define", "4:outer:wrapper:H"]);
    assert!(stderr.contains("Nested user-function"));
}

#[test]
fn json_preserves_original_decimal_literal() {
    let (stdout, _) = run_ries(&["2.00000000000000000000", "--json", "--classic"]);
    let data: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(data["target_literal"], "2.00000000000000000000");
}

#[test]
fn decimal_precision_loss_and_underflow_are_rejected_before_search() {
    for args in [
        vec!["1.000000000000000000000000000001", "--match-all-digits"],
        vec!["-1.000000000000000000000000000001", "--match-all-digits"],
        vec!["1e-400"],
        vec!["-1e-400"],
    ] {
        let mut args = args;
        let target = args.remove(0);
        args.extend(["--json", "--report=false", "-l0", "-Sx1", "--", target]);
        let output = run_ries_raw(&args);
        assert!(!output.status.success(), "accepted {args:?}");
        assert!(output.stdout.is_empty());
        let error = String::from_utf8_lossy(&output.stderr);
        assert!(
            error.contains("tolerance") || error.contains("underflows"),
            "{error}"
        );
    }
    let (stdout, _) = run_ries(&[
        "1.000000000000000000000000000000",
        "--match-all-digits",
        "--json",
        "-l0",
        "-Sx1",
    ]);
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&stdout).unwrap()["results"][0]["exact"],
        true
    );
}

#[test]
fn precision_availability_requires_explicit_fallback() {
    let output = run_ries_raw(&["2", "--json", "--precision", "53", "--no-slow-messages"]);
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    let (stdout, _) = run_ries(&[
        "2",
        "--json",
        "--classic",
        "--precision",
        "53",
        "--allow-unverified",
    ]);
    let data: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(
        data["high_precision_verification"]["results"][0]["status"],
        "unavailable"
    );
    assert_eq!(
        data["high_precision_verification"]["results"][0]["is_verified"],
        false
    );
    #[cfg(not(feature = "highprec"))]
    {
        let output = run_ries_raw(&["2", "--json", "--precision", "128"]);
        assert_eq!(output.status.code(), Some(2));
        assert!(output.stdout.is_empty());
    }
}

#[test]
fn conflicting_search_modes_cannot_silently_ignore_memory_hints() {
    for mode in ["--adaptive", "--one-sided"] {
        for options in [
            vec!["--streaming"],
            vec!["--turbo"],
            vec!["--max-memory", "1M"],
            vec!["--min-memory", "1M"],
            vec!["--memory-abort-threshold", "0.5"],
        ] {
            let mut args = vec!["2", "--json", mode];
            args.extend(options);
            let output = run_ries_raw(&args);
            assert_eq!(output.status.code(), Some(2), "{args:?}");
            assert!(output.stdout.is_empty());
        }
    }
}

#[test]
fn adaptive_json_and_manifest_report_actual_and_requested_bounds() {
    let path = unique_tmp_path("adaptive-bounds");
    let (stdout, _) = run_ries(&[
        "2.506314",
        "--adaptive",
        "--deterministic",
        "-l0",
        "-Sx12+",
        "--max-match-distance",
        "0.1",
        "-n10",
        "--json",
        "--report=false",
        "--emit-manifest",
        path.to_str().unwrap(),
    ]);
    let data: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    let manifest: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    assert_eq!(data["matcher"], "adaptive");
    assert_eq!(data["requested_max_lhs_complexity"], 35);
    assert_eq!(data["requested_max_rhs_complexity"], 35);
    assert!(data["max_lhs_complexity"].as_u64().unwrap() > 35);
    assert_eq!(
        data["max_lhs_complexity"],
        manifest["config"]["max_lhs_complexity"]
    );
    assert_eq!(
        data["max_rhs_complexity"],
        manifest["config"]["max_rhs_complexity"]
    );
    assert_eq!(
        manifest["config"]["replay"]["requested_complexity_bounds"]["lhs"],
        35
    );
    for row in data["results"].as_array().unwrap() {
        let lhs = ries_rs::Expression::parse(row["lhs_postfix"].as_str().unwrap()).unwrap();
        let rhs = ries_rs::Expression::parse(row["rhs_postfix"].as_str().unwrap()).unwrap();
        assert!(u64::from(lhs.complexity()) <= data["max_lhs_complexity"].as_u64().unwrap());
        assert!(u64::from(rhs.complexity()) <= data["max_rhs_complexity"].as_u64().unwrap());
    }
    let _ = std::fs::remove_file(path);
}

#[test]
fn pslq_cannot_bypass_decimal_validation() {
    for (target, precision) in [
        ("1e-400", false),
        ("1.000000000000000000000000000001", true),
    ] {
        let mut args = vec![target, "--pslq", "--pslq-max-coeff", "10"];
        if precision {
            args.push("--match-all-digits");
        }
        let output = run_ries_raw(&args);
        assert_eq!(output.status.code(), Some(2));
        assert!(
            output.stdout.is_empty(),
            "special mode printed a result for {target}"
        );
    }
}

#[test]
fn manifests_preserve_decimal_spelling_and_signed_zero() {
    for literal in ["1.000000000000000000000000000000", "-0"] {
        let path = unique_tmp_path("target-literal");
        run_ries(&[
            "--match-all-digits",
            "-l0",
            "-Sx1",
            "--json",
            "--emit-manifest",
            path.to_str().unwrap(),
            "--",
            literal,
        ]);
        let data: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(data["config"]["target_literal"], literal);
        assert_eq!(data["config"]["replay"]["cli_args"][0], literal);
        let Some(python) = super::basics::replay_python() else {
            let _ = std::fs::remove_file(path);
            continue;
        };
        let script = format!("{}/tools/replay_manifest.py", env!("CARGO_MANIFEST_DIR"));
        let output = Command::new(python)
            .args([
                script.as_str(),
                path.to_str().unwrap(),
                "--bin",
                env!("CARGO_BIN_EXE_ries-rs"),
            ])
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(String::from_utf8_lossy(&output.stdout).contains("identical f64 bit patterns"));
        let _ = std::fs::remove_file(path);
    }
}

#[test]
fn evaluation_point_cannot_bypass_decimal_validation() {
    for (point, precision) in [
        ("1e-400", false),
        ("1.000000000000000000000000000001", true),
    ] {
        let mut args = vec!["--eval-expression", "x", "--at", point];
        if precision {
            args.push("--match-all-digits");
        }
        let output = run_ries_raw(&args);
        assert_eq!(output.status.code(), Some(2));
        assert!(output.stdout.is_empty());
    }
}
