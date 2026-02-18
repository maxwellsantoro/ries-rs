use super::*;

#[test]
fn test_f1_condensed_format_accepted() {
    let (stdout, _stderr) = run_ries(&["2.5", "-F1", "--classic", "--report", "false", "-n", "1"]);
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
    ]);
    // Should show total equations tested or similar summary info
    let lower = stdout.to_lowercase();
    assert!(
        lower.contains("total") || lower.contains("equations") || lower.contains("summary"),
        "expected --verbose to show summary with total/equations in footer\n{}",
        stdout
    );
}

#[test]
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

#[test]
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

#[test]
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
    let (_stdout, stderr) = run_ries(&["2.5", "-DA", "--report", "false", "--max-matches", "1"]);
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
    ]);
    assert!(stdout.contains("x"));
}

#[test]
fn test_diagnostic_g_recognized() {
    let (stdout, stderr) = run_ries(&["2.5", "-DG", "--report", "false", "--max-matches", "1"]);
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
    let (stdout, stderr) = run_ries(&["2.5", "-DG", "--report", "false", "--max-matches", "1"]);
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
    let (_stdout, stderr) = run_ries(&["2.5", "-DB", "--report", "false", "--max-matches", "1"]);
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
    let (stdout0, _) = run_ries(&["2.5", "-F0", "--max-matches", "1"]);
    // Report mode with -F2 should show infix format
    let (stdout2, _) = run_ries(&["2.5", "-F2", "--max-matches", "1"]);

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
fn test_no_slow_messages_suppresses_precision_warning() {
    let output = run_ries_raw(&[
        "2.5",
        "--report",
        "false",
        "--max-matches",
        "1",
        "--precision",
        "256",
        "--no-slow-messages",
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
