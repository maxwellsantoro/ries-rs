use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

fn run_ries_raw(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_ries-rs"))
        .args(args)
        .output()
        .expect("failed to run ries-rs")
}

fn run_ries(args: &[&str]) -> (String, String) {
    let output = run_ries_raw(args);

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    assert!(
        output.status.success(),
        "command failed\nargs: {:?}\nstdout:\n{}\nstderr:\n{}",
        args,
        stdout,
        stderr
    );

    (stdout, stderr)
}

fn run_ries_owned(args: &[String]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_ries-rs"))
        .args(args)
        .output()
        .expect("failed to run ries-rs")
}

fn parse_stat_value(stdout: &str, key: &str) -> Option<usize> {
    stdout.lines().find_map(|line| {
        if !line.contains(key) {
            return None;
        }
        line.split_whitespace().last()?.parse::<usize>().ok()
    })
}

fn parse_generated_counts(stdout: &str) -> Option<(usize, usize)> {
    stdout.lines().find_map(|line| {
        if !line.starts_with("Generated ") {
            return None;
        }
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 5 {
            return None;
        }
        let lhs = parts.get(1)?.parse::<usize>().ok()?;
        let rhs = parts.get(4)?.parse::<usize>().ok()?;
        Some((lhs, rhs))
    })
}

fn parse_first_complexity(stdout: &str) -> Option<u32> {
    stdout.lines().find_map(|line| {
        let start = line.rfind('{')?;
        let end = line.rfind('}')?;
        if end <= start + 1 {
            return None;
        }
        line[start + 1..end].trim().parse::<u32>().ok()
    })
}

fn parse_first_match_line(stdout: &str) -> Option<String> {
    stdout
        .lines()
        .find(|line| line.contains('=') && line.contains('{'))
        .map(|line| line.trim().to_string())
}

fn parse_match_lines(stdout: &str) -> Vec<String> {
    stdout
        .lines()
        .filter(|line| line.contains('=') && line.contains('{'))
        .map(|line| line.trim().to_string())
        .collect()
}

fn unique_tmp_path(stem: &str) -> std::path::PathBuf {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before unix epoch")
        .as_nanos();
    std::env::temp_dir().join(format!(
        "ries-rs-{}-{}-{}.ries",
        stem,
        std::process::id(),
        now
    ))
}

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
    let (stdout, _stderr) = run_ries(&[
        "2.506314",
        "--classic",
        "--max-match-distance",
        "1e-5",
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
    assert!(
        stdout.contains("{106}"),
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

    let (rhs_stdout, _stderr) = run_ries(&["2.5", "--report", "false", "-n", "1", "--O-RHS", "1*"]);
    let (_lhs_rhs, rhs_restricted) =
        parse_generated_counts(&rhs_stdout).expect("missing rhs-restricted generated counts");

    assert!(
        rhs_restricted < rhs_base,
        "expected --O-RHS to reduce RHS generation\nbase:\n{}\nrestricted:\n{}",
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

#[test]
fn test_s_flag_shows_equation_form_not_misleading_x_equals() {
    // The -s flag should avoid misleading direct assignment for complex LHS forms.
    // It may either keep equation form, or show a valid transformed x = ... expression.
    let (stdout, _stderr) =
        run_ries(&["2.5063", "--classic", "--report", "false", "-s", "-n", "1"]);

    assert!(
        stdout.contains("tanpi(x) =") || stdout.contains("atan2("),
        "expected -s to either preserve equation form or show a valid inverse transform\n{}",
        stdout
    );
    assert!(
        !stdout.contains("x = 4-e^4"),
        "expected -s to avoid misleading direct assignment\n{}",
        stdout
    );
}

#[test]
fn test_s_flag_without_complex_lhs_works_correctly() {
    // When LHS is just x, the -s flag should still work (though the output is the same)
    let (stdout, _stderr) = run_ries(&["2.5", "--classic", "--report", "false", "-s", "-n", "1"]);

    // For 2.5, the first match should be x = 5/2, which is correct even with -s
    assert!(
        stdout.contains(" = 5/2") && stdout.contains("('exact' match)"),
        "expected -s to work for simple x = value equations\n{}",
        stdout
    );
}

#[test]
fn test_p_flag_without_file_accepts_target() {
    // Original ries behavior: ries -p 2.5 -> uses default profile, searches for 2.5
    // The -p flag should NOT greedily consume the target value as a profile filename
    let output = run_ries_raw(&["-p", "2.5", "--classic", "--report", "false", "-n", "1"]);
    assert!(
        output.status.success(),
        "Should accept target after -p\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("2.5"),
        "Should show target value 2.5\n{}",
        stdout
    );
}

#[test]
fn test_l_flag_liouvillian_mode() {
    // Original: ries -l 2.5 -> Liouvillian mode, target 2.5
    // Legacy semantics: "-l" with a float value and no explicit target means
    // liouvillian mode + target value
    let output = run_ries_raw(&["-l", "2.5", "--classic", "--report", "false", "-n", "1"]);
    assert!(
        output.status.success(),
        "Should parse -l as liouvillian + target\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("2.5"),
        "Should show target value 2.5\n{}",
        stdout
    );
}

#[test]
fn test_level_flag_with_integer() {
    // For explicit level, use -l3 or --level 3 with a target
    let output = run_ries_raw(&[
        "--level",
        "1",
        "2.5",
        "--classic",
        "--report",
        "false",
        "-n",
        "1",
    ]);
    assert!(
        output.status.success(),
        "Should parse --level 1 with target 2.5\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("2.5"),
        "Should show target value 2.5\n{}",
        stdout
    );
}

#[test]
fn test_i_flag_fallback_to_r() {
    // Original: ries -i 2.5 -> warns and uses -r
    let output = run_ries_raw(&["-i", "2.5", "--classic", "--report", "false", "-n", "1"]);
    assert!(
        output.status.success(),
        "Should fallback to -r mode\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    // Check for warning in either stdout or stderr
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        combined.contains("Replacing -i with -r") || combined.contains("replacing -i with -r"),
        "Should warn about fallback\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("2.5") || stdout.contains("2 x = 5"),
        "Should find matches for 2.5\n{}",
        stdout
    );
}

#[test]
fn test_ie_integer_exact_mode() {
    // --ie = integer exact mode (stops at first exact match)
    let output = run_ries_raw(&["--ie", "3.0", "--classic", "--report", "false"]);
    assert!(output.status.success(), "Should succeed with --ie flag");
    let stdout = String::from_utf8_lossy(&output.stdout);
    // Should find x=3 as exact match and stop quickly
    assert!(
        stdout.contains("3") && stdout.contains("('exact' match)"),
        "Should find x=3 as exact match\n{}",
        stdout
    );
}

#[test]
fn test_re_rational_exact_mode() {
    // --re = rational exact mode (stops at first exact match)
    let output = run_ries_raw(&["--re", "2.5", "--classic", "--report", "false"]);
    assert!(output.status.success(), "Should succeed with --re flag");
    let stdout = String::from_utf8_lossy(&output.stdout);
    // Should find 2x=5 or x=5/2 as exact match
    assert!(
        stdout.contains("('exact' match)"),
        "Should find exact match for 2.5\n{}",
        stdout
    );
}

#[test]
fn test_s_bare_symbol_table() {
    // -S without argument should print the symbol table and exit
    let output = run_ries_raw(&["-S"]);
    assert!(output.status.success(), "Should succeed with bare -S");
    let stdout = String::from_utf8_lossy(&output.stdout);
    // Should print symbol table with pi, e, and other symbols
    assert!(
        stdout.contains("pi") && stdout.contains("e"),
        "Should print symbol table with pi and e\n{}",
        stdout
    );
    assert!(
        stdout.contains("Explicit values") || stdout.contains("description"),
        "Should show symbol table header\n{}",
        stdout
    );
}

#[test]
fn test_e_bare_enable_all() {
    // -E without argument should enable all symbols and treat next arg as target
    // Original ries: "ries -E 2.5" means "enable all and search for 2.5"
    let output = run_ries_raw(&["-E", "2.5", "--classic", "--report", "false", "-n", "1"]);
    assert!(
        output.status.success(),
        "Should succeed with bare -E: {:?}",
        output
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("2.5"),
        "Should show target value\n{}",
        stdout
    );
}

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
