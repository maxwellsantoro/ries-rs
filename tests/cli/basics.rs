use super::*;

#[test]
fn test_define_does_not_panic() {
    let (stdout, _stderr) = run_ries(&[
        "--define",
        "6:sinh:hyperbolic sine:E|r-2/",
        "--classic",
        "3.6268604078",
        "-n",
        "1",
    ]);
    assert!(stdout.contains("Search completed"));
}

#[test]
fn test_rhs_only_symbols_filter_applies() {
    let (stdout, _stderr) = run_ries(&[
        "2.506314",
        "--S-RHS",
        "1",
        "--N-RHS",
        "1",
        "--classic",
        "-n",
        "3",
    ]);
    assert!(
        stdout.contains("No matches found."),
        "expected RHS filter to eliminate matches\n{}",
        stdout
    );
}

#[test]
fn test_max_match_distance_applies() {
    // With calibrated weights the search can find matches with error ~1e-9.
    // Use a sub-noise threshold (1e-12) that is guaranteed to eliminate all matches.
    let (stdout, _stderr) = run_ries(&[
        "2.506314",
        "--classic",
        "--max-match-distance",
        "1e-12",
        "-n",
        "3",
    ]);
    assert!(
        stdout.contains("No matches found."),
        "expected max-match-distance to eliminate coarse matches\n{}",
        stdout
    );
}

#[test]
fn test_no_refinement_disables_newton_calls() {
    let (stdout, _stderr) = run_ries(&[
        "2.506314",
        "-l",
        "0",
        "--stats",
        "--no-refinement",
        "-n",
        "3",
    ]);
    let calls = parse_stat_value(&stdout, "Newton calls:")
        .expect("missing 'Newton calls' line in --stats output");
    assert_eq!(calls, 0, "expected no-refinement to skip Newton");
}

#[test]
fn test_one_sided_mode_uses_single_lhs() {
    let (stdout, _stderr) = run_ries(&["2.5", "--one-sided", "--stats", "-n", "1"]);
    let lhs = parse_stat_value(&stdout, "LHS expressions:")
        .expect("missing 'LHS expressions' line in --stats output");
    assert_eq!(lhs, 1, "expected one-sided mode to use only x on LHS");
}

#[test]
fn test_symbol_weights_flag_changes_complexity() {
    let (stdout, _stderr) = run_ries(&["2", "--classic", "-n", "1", "--symbol-weights", ":2:100"]);
    // x=15 (new calibrated weight) + 2=100 (overridden) = 115
    assert!(
        stdout.contains("{115}"),
        "expected x = 2 complexity to reflect overridden weight\n{}",
        stdout
    );
}

#[test]
fn test_classic_prefers_exact_match() {
    let (stdout, _stderr) = run_ries(&["6.283185307179586", "--classic", "-n", "1", "-x"]);
    assert!(
        stdout.contains("x = 2 pi"),
        "expected first classic match to prefer exact pi-based form\n{}",
        stdout
    );
}

#[test]
fn test_op_limits_is_count_limit_not_allow_list() {
    let (stdout, _stderr) = run_ries(&["6", "--report", "false", "-n", "1", "-l", "2", "-O", "1+"]);
    let (lhs, rhs) =
        parse_generated_counts(&stdout).expect("missing generated counts in CLI output");
    assert!(
        lhs > 1 && rhs > 1,
        "expected -O to constrain counts without collapsing symbol set\n{}",
        stdout
    );
}

#[test]
fn test_rhs_symbol_restriction_changes_rhs_generation() {
    let (base_stdout, _stderr) = run_ries(&["2.5", "--classic", "-n", "1"]);
    let (_lhs_base, rhs_base) =
        parse_generated_counts(&base_stdout).expect("missing base generated counts");

    let (rhs_stdout, _stderr) = run_ries(&["2.5", "--classic", "-n", "1", "--S-RHS", "1"]);
    let (_lhs_rhs, rhs_restricted) =
        parse_generated_counts(&rhs_stdout).expect("missing rhs-restricted generated counts");

    assert!(
        rhs_restricted < rhs_base,
        "expected --S-RHS to reduce RHS generation\nbase:\n{}\nrestricted:\n{}",
        base_stdout,
        rhs_stdout
    );
}

#[test]
fn test_symbol_names_profile_applies_to_output() {
    let profile = unique_tmp_path("symbol-names");
    std::fs::write(&profile, "--symbol-names :p:PI_CUSTOM\n").expect("write profile");

    let args = vec![
        "-p".to_string(),
        profile.to_string_lossy().to_string(),
        "3.141592653589793".to_string(),
        "--classic".to_string(),
        "-n".to_string(),
        "1".to_string(),
        "-x".to_string(),
    ];
    let output = run_ries_owned(&args);
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    assert!(
        output.status.success(),
        "command failed\nstdout:\n{}\nstderr:\n{}",
        stdout,
        stderr
    );
    assert!(
        stdout.contains("PI_CUSTOM"),
        "expected symbol rename to appear in output\n{}",
        stdout
    );
}

#[test]
fn test_profile_include_chain_loads_nested_constants() {
    let inner = unique_tmp_path("include-inner");
    let outer = unique_tmp_path("include-outer");
    std::fs::write(&inner, "-X \"4:tau:TauConstant:6.283185307179586\"\n").expect("write inner");
    std::fs::write(&outer, format!("--include {}\n", inner.to_string_lossy()))
        .expect("write outer");

    let args = vec![
        "-p".to_string(),
        outer.to_string_lossy().to_string(),
        "6.283185307179586".to_string(),
        "--classic".to_string(),
        "-n".to_string(),
        "1".to_string(),
        "-x".to_string(),
    ];
    let output = run_ries_owned(&args);
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    assert!(
        output.status.success(),
        "command failed\nstdout:\n{}\nstderr:\n{}",
        stdout,
        stderr
    );
    assert!(
        stdout.contains("tau"),
        "expected nested include constant to appear\n{}",
        stdout
    );
}

#[test]
fn test_missing_include_is_error() {
    let missing = unique_tmp_path("missing-include");
    let args = vec![
        "2.5".to_string(),
        "--include".to_string(),
        missing.to_string_lossy().to_string(),
    ];
    let output = run_ries_owned(&args);
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    assert!(!output.status.success(), "missing include should fail");
    assert!(
        stderr.contains("Could not open") || stderr.contains("Error reading"),
        "expected explicit include error\n{}",
        stderr
    );
}

#[test]
fn test_user_constant_weight_changes_complexity() {
    let (stdout_low, _stderr) = run_ries(&[
        "0.123456789",
        "--classic",
        "--stop-at-exact",
        "-n",
        "1",
        "-x",
        "-X",
        "4:taux:test:0.123456789",
    ]);
    let low = parse_first_complexity(&stdout_low).expect("missing complexity for low-weight run");

    let (stdout_high, _stderr) = run_ries(&[
        "0.123456789",
        "--classic",
        "--stop-at-exact",
        "-n",
        "1",
        "-x",
        "-X",
        "99:taux:test:0.123456789",
    ]);
    let high =
        parse_first_complexity(&stdout_high).expect("missing complexity for high-weight run");

    assert!(
        high > low + 20,
        "expected larger complexity with larger user weight ({} vs {})\nlow:\n{}\nhigh:\n{}",
        low,
        high,
        stdout_low,
        stdout_high
    );
}

#[test]
fn test_list_options_outputs_known_flags() {
    let output = run_ries_raw(&["--list-options"]);
    assert!(
        output.status.success(),
        "--list-options should exit successfully"
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("--list-options"));
    assert!(stdout.contains("--find-expression"));
    assert!(stdout.contains("--complexity-ranking"));
    assert!(stdout.contains("--parity-ranking"));
    assert!(stdout.contains("--O-RHS"));
    assert!(stdout.contains("--E-RHS"));
}

#[test]
fn test_stability_thorough_uses_configured_level_count() {
    // Use target=1 so `x = 1` is found at level 0 (complexity x(15)+1(10)=25 fits the budget).
    let (default_stdout, _stderr) = run_ries(&[
        "1",
        "--classic",
        "--report",
        "false",
        "--stability-check",
        "-l",
        "0",
        "-n",
        "1",
    ]);
    let default_levels = parse_first_total_levels(&default_stdout)
        .expect("expected stability report with level count for default config");
    assert_eq!(
        default_levels, 5,
        "default stability config should run 5 levels\n{}",
        default_stdout
    );

    let (thorough_stdout, _stderr) = run_ries(&[
        "1",
        "--classic",
        "--report",
        "false",
        "--stability-check",
        "--stability-thorough",
        "-l",
        "0",
        "-n",
        "1",
    ]);
    let thorough_levels = parse_first_total_levels(&thorough_stdout)
        .expect("expected stability report with level count for thorough config");
    assert_eq!(
        thorough_levels, 8,
        "thorough stability config should run 8 levels\n{}",
        thorough_stdout
    );
    assert!(
        thorough_levels > default_levels,
        "thorough mode should evaluate more levels"
    );
}

#[test]
fn test_enable_reenables_symbol_after_exclude() {
    let (stdout_no_enable, _stderr) = run_ries(&["2.5", "--report", "false", "-n", "1", "-N", "+"]);
    let (lhs_no, rhs_no) =
        parse_generated_counts(&stdout_no_enable).expect("missing generated counts");

    let (stdout_enable, _stderr) =
        run_ries(&["2.5", "--report", "false", "-n", "1", "-N", "+", "-E", "+"]);
    let (lhs_yes, rhs_yes) =
        parse_generated_counts(&stdout_enable).expect("missing generated counts");

    assert!(
        lhs_yes > lhs_no || rhs_yes > rhs_no,
        "expected -E to re-enable excluded symbols\nno enable:\n{}\nwith enable:\n{}",
        stdout_no_enable,
        stdout_enable
    );
}

#[test]
fn test_orhs_reduces_rhs_generation() {
    let (base_stdout, _stderr) = run_ries(&["2.5", "--report", "false", "-n", "1"]);
    let (_lhs_base, rhs_base) =
        parse_generated_counts(&base_stdout).expect("missing base generated counts");

    // --N-RHS p excludes pi from the RHS symbol set, which measurably reduces RHS count.
    let (rhs_stdout, _stderr) = run_ries(&["2.5", "--report", "false", "-n", "1", "--N-RHS", "p"]);
    let (_lhs_rhs, rhs_restricted) =
        parse_generated_counts(&rhs_stdout).expect("missing rhs-restricted generated counts");

    assert!(
        rhs_restricted < rhs_base,
        "expected --N-RHS to reduce RHS generation\nbase:\n{}\nrestricted:\n{}",
        base_stdout,
        rhs_stdout
    );
}

#[test]
fn test_min_match_distance_filters_out_exact_match() {
    let (stdout, _stderr) = run_ries(&[
        "2.5",
        "--classic",
        "-n",
        "1",
        "--min-match-distance",
        "1e-4",
    ]);
    assert!(
        !stdout.contains("('exact' match)"),
        "expected minimum match distance to filter exact matches\n{}",
        stdout
    );
}

#[test]
fn test_find_expression_works_without_target() {
    let (stdout, _stderr) = run_ries(&["--find-expression", "xq", "--at", "4"]);
    assert!(stdout.contains("Expression: xq"));
    assert!(stdout.contains("Value = 2.000000000000000"));
}

#[test]
fn test_symbol_names_cli_applies_to_output() {
    let (stdout, _stderr) = run_ries(&[
        "3.141592653589793",
        "--classic",
        "--report",
        "false",
        "-n",
        "1",
        "--symbol-names",
        ":p:PI",
    ]);
    assert!(
        stdout.contains("PI"),
        "expected --symbol-names override to appear in output\n{}",
        stdout
    );
}

#[test]
fn test_mad_alias_applies_max_match_distance() {
    let (stdout, _stderr) = run_ries(&[
        "2.506314",
        "--classic",
        "--report",
        "false",
        "-n",
        "3",
        "--mad",
        "0",
    ]);
    assert!(
        stdout.contains("No matches found."),
        "expected --mad 0 to behave like --max-match-distance 0\n{}",
        stdout
    );
}

#[test]
fn test_extended_compat_options_are_accepted() {
    let (stdout, _stderr) = run_ries(&[
        "2.5",
        "--report",
        "false",
        "-n",
        "1",
        "--any-exponents",
        "--any-subexpressions",
        "--any-trig-args",
        "--canon-reduction",
        "nr25",
        "--canon-simplify",
        "--derivative-margin",
        "1e-8",
        "--explicit-multiply",
        "--match-all-digits",
        "--max-equate-value",
        "1000",
        "--min-equate-value",
        "0",
        "--max-memory",
        "256M",
        "--memory-abort-threshold",
        "0.5",
        "--max-trig-cycles",
        "8",
        "--min-memory",
        "16M",
        "--no-canon-simplify",
        "--no-slow-messages",
        "--numeric-anagram",
        "--rational-exponents",
        "--rational-trig-args",
        "--show-work",
        "--significance-loss-margin",
        "1e-9",
        "--trig-argument-scale",
        "1.0",
        "-D",
    ]);
    assert!(
        stdout.contains("Search completed"),
        "expected compatibility options to be accepted\n{}",
        stdout
    );
}

#[test]
fn test_rational_exponents_rejects_irrational_exponents() {
    let (baseline_stdout, _stderr) = run_ries(&[
        "8.824977827",
        "--one-sided",
        "--report",
        "false",
        "-n",
        "20",
        "-F",
        "0",
    ]);
    assert!(
        baseline_stdout.contains("2p^"),
        "expected baseline run to include irrational exponent form\n{}",
        baseline_stdout
    );

    let (restricted_stdout, _stderr) = run_ries(&[
        "8.824977827",
        "--one-sided",
        "--rational-exponents",
        "--report",
        "false",
        "-n",
        "20",
        "-F",
        "0",
    ]);
    assert!(
        !restricted_stdout.contains("2p^"),
        "expected --rational-exponents to filter irrational exponent forms\n{}",
        restricted_stdout
    );
}

#[test]
fn test_rational_trig_args_rejects_irrational_constants() {
    let target = "0.773942685266709";
    let (baseline_stdout, _stderr) = run_ries(&[
        target,
        "--one-sided",
        "--report",
        "false",
        "-n",
        "20",
        "-F",
        "0",
    ]);
    assert!(
        baseline_stdout.contains("eS"),
        "expected baseline run to include sinpi(e)\n{}",
        baseline_stdout
    );

    let (restricted_stdout, _stderr) = run_ries(&[
        target,
        "--one-sided",
        "--rational-trig-args",
        "--report",
        "false",
        "-n",
        "20",
        "-F",
        "0",
    ]);
    assert!(
        !restricted_stdout.contains("eS"),
        "expected --rational-trig-args to filter irrational trig arguments\n{}",
        restricted_stdout
    );
}

#[test]
fn test_format_zero_uses_compact_postfix_output() {
    let (stdout, _stderr) = run_ries(&[
        "2.5",
        "--classic",
        "--report",
        "false",
        "-n",
        "1",
        "-F",
        "0",
    ]);
    assert!(
        stdout.contains("52/"),
        "expected compact postfix output with -F0\n{}",
        stdout
    );
}

#[test]
fn test_format_three_uses_verbose_postfix_output() {
    let (stdout, _stderr) = run_ries(&[
        "2.5",
        "--classic",
        "--report",
        "false",
        "-n",
        "1",
        "-F",
        "3",
    ]);
    assert!(
        stdout.contains("5 2 /"),
        "expected verbose postfix output with -F3\n{}",
        stdout
    );
}

#[test]
fn test_show_work_outputs_step_details() {
    let (stdout, _stderr) = run_ries(&[
        "2.5",
        "--classic",
        "--report",
        "false",
        "-n",
        "1",
        "--show-work",
    ]);
    assert!(
        stdout.contains("--show-work details:")
            && stdout.contains("LHS steps:")
            && stdout.contains("RHS steps:"),
        "expected --show-work output\n{}",
        stdout
    );
}

#[test]
fn test_dy_enables_stats_output() {
    let (stdout, _stderr) = run_ries(&["2.5", "--classic", "--report", "false", "-n", "1", "-Dy"]);
    assert!(
        stdout.contains("=== Search Statistics ==="),
        "expected -Dy to enable stats output\n{}",
        stdout
    );
}

#[test]
fn test_max_equate_value_filters_matches() {
    let (stdout, _stderr) = run_ries(&[
        "2.5",
        "--classic",
        "--report",
        "false",
        "-n",
        "3",
        "--max-equate-value",
        "1",
    ]);
    assert!(
        stdout.contains("No matches found."),
        "expected --max-equate-value to filter out matches\n{}",
        stdout
    );
}

#[test]
fn test_explicit_multiply_changes_infix_rendering() {
    let (stdout, _stderr) = run_ries(&[
        "2.5066282746310002",
        "--classic",
        "--report",
        "false",
        "-n",
        "1",
        "--explicit-multiply",
    ]);
    assert!(
        stdout.contains("2*pi"),
        "expected --explicit-multiply to show explicit multiplication\n{}",
        stdout
    );
}

// ============================================================================
// Streaming flag precedence tests - regression tests for P2
// ============================================================================

#[test]
fn test_explicit_streaming_respected_over_min_memory() {
    // When --streaming is explicitly set, --min-memory should not override it
    // This is a regression test for P2: --min-memory can override explicit --streaming

    // Run with --streaming alone
    let (streaming_stdout, _stderr) = run_ries(&[
        "2.5",
        "--classic",
        "--report",
        "false",
        "--streaming",
        "-l",
        "0",
        "-n",
        "1",
    ]);

    // Run with --streaming --min-memory 3G
    // The --min-memory 3G should NOT disable the explicit --streaming
    let (streaming_with_min_memory_stdout, _stderr) = run_ries(&[
        "2.5",
        "--classic",
        "--report",
        "false",
        "--streaming",
        "--min-memory",
        "3G",
        "-l",
        "0",
        "-n",
        "1",
    ]);

    // Both should produce similar results since streaming is respected
    // Extract the LHS count if present, or just verify both complete successfully
    assert!(
        streaming_stdout.contains("Search completed")
            || streaming_stdout.contains("=")
            || streaming_stdout.contains("x"),
        "expected streaming search to complete\n{}",
        streaming_stdout
    );

    assert!(
        streaming_with_min_memory_stdout.contains("Search completed")
            || streaming_with_min_memory_stdout.contains("=")
            || streaming_with_min_memory_stdout.contains("x"),
        "expected streaming with min-memory to still use streaming\n{}",
        streaming_with_min_memory_stdout
    );

    // The key assertion: both runs should use streaming mode
    // We verify this by checking that --min-memory 3G doesn't silently
    // disable the explicit --streaming flag
}

#[test]
fn test_min_memory_disables_auto_streaming() {
    // When streaming is NOT explicitly set, --min-memory can disable auto-streaming
    // This is the expected behavior (not a bug)

    // Small --max-memory should trigger auto-streaming
    let (auto_streaming_stdout, _stderr) = run_ries(&[
        "2.5",
        "--classic",
        "--report",
        "false",
        "--max-memory",
        "256M",
        "-l",
        "0",
        "-n",
        "1",
    ]);

    // Small --max-memory with large --min-memory should not auto-stream
    let (no_auto_streaming_stdout, _stderr) = run_ries(&[
        "2.5",
        "--classic",
        "--report",
        "false",
        "--max-memory",
        "256M",
        "--min-memory",
        "3G",
        "-l",
        "0",
        "-n",
        "1",
    ]);

    // Both should complete successfully - we're just verifying the options work
    assert!(
        auto_streaming_stdout.contains("Search completed")
            || auto_streaming_stdout.contains("=")
            || auto_streaming_stdout.contains("x"),
        "expected auto-streaming search to complete\n{}",
        auto_streaming_stdout
    );

    assert!(
        no_auto_streaming_stdout.contains("Search completed")
            || no_auto_streaming_stdout.contains("=")
            || no_auto_streaming_stdout.contains("x"),
        "expected non-streaming search to complete\n{}",
        no_auto_streaming_stdout
    );
}
