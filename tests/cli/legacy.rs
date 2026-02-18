use super::*;

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
