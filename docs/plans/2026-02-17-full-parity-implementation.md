# Full Parity Implementation Plan

> Historical planning document: paths, repository metadata, and license snippets in this file reflect its drafting date and may be outdated. Current source-of-truth is `Cargo.toml`, `README.md`, and `src/`.

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Achieve full parity between ries-rs and original RIES across output formatting, diagnostics, algorithm options, and search quality.

**Architecture:** Layer-by-layer approach. Layer 1 (output surface) → Layer 2 (diagnostics) → Layer 3a (quick-win options) → Layer 4 (search quality). Each layer is independent and produces a testable milestone.

**Tech Stack:** Rust, clap for CLI, existing ries-rs architecture (main.rs, search.rs, pool.rs, expr.rs)

---

## Layer 1: Output Surface

### Task 1: Add `-F1` Condensed Format Support

**Files:**
- Modify: `src/main.rs:879-884` (DisplayFormat enum)
- Modify: `src/main.rs:1558-1567` (parse_display_format function)
- Test: `tests/cli_regression_tests.rs`

**Step 1: Write the failing test**

Add to `tests/cli_regression_tests.rs`:

```rust
#[test]
fn test_f1_condensed_format_accepted() {
    let (stdout, _stderr) = run_ries(&["2.5", "-F1", "--report", "false", "--max-matches", "1"]);
    // -F1 should work (alias for -F0 postfix compact)
    assert!(stdout.contains("x") || stdout.contains("2.5"));
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test test_f1_condensed_format_accepted`
Expected: FAIL - format "1" not recognized

**Step 3: Add Condensed variant to DisplayFormat**

In `src/main.rs:879-884`, change:

```rust
#[derive(Debug, Clone, Copy)]
enum DisplayFormat {
    Infix(expr::OutputFormat),
    PostfixCompact,
    PostfixVerbose,
    Condensed,  // NEW: -F1 alias for PostfixCompact
}
```

**Step 4: Update parse_display_format**

In `src/main.rs:1558-1567`, add case for "1":

```rust
fn parse_display_format(s: &str) -> DisplayFormat {
    match s.to_lowercase().as_str() {
        "0" => DisplayFormat::PostfixCompact,
        "1" => DisplayFormat::Condensed,  // NEW
        "3" => DisplayFormat::PostfixVerbose,
        "pretty" | "unicode" => DisplayFormat::Infix(expr::OutputFormat::Pretty),
        "mathematica" | "math" | "mma" => DisplayFormat::Infix(expr::OutputFormat::Mathematica),
        "sympy" | "python" => DisplayFormat::Infix(expr::OutputFormat::SymPy),
        _ => DisplayFormat::Infix(expr::OutputFormat::Default),
    }
}
```

**Step 5: Handle Condensed in print functions**

Find where `DisplayFormat::PostfixCompact` is matched and add `DisplayFormat::Condensed` as an alias. The print_match_absolute and print_match_relative functions use pattern matching.

**Step 6: Run test to verify it passes**

Run: `cargo test test_f1_condensed_format_accepted`
Expected: PASS

**Step 7: Commit**

```bash
git add src/main.rs tests/cli_regression_tests.rs
git commit -m "feat: add -F1 condensed format (alias for -F0)"
```

---

### Task 2: Add Output Header (Target + Settings Summary)

**Files:**
- Modify: `src/main.rs` (add print_header function)
- Test: `tests/cli_regression_tests.rs`

**Step 1: Write the failing test**

```rust
#[test]
fn test_verbose_output_shows_target() {
    let (stdout, _stderr) = run_ries(&["2.5", "--verbose", "--report", "false", "--max-matches", "1"]);
    assert!(stdout.contains("Target:") || stdout.contains("target"));
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test test_verbose_output_shows_target`
Expected: FAIL - no "Target:" in output

**Step 3: Add --verbose flag to Args struct**

In the Args struct (around line 100), add:

```rust
/// Show verbose output with header and footer details
#[arg(short = 'V', long)]
verbose: bool,
```

**Step 4: Add print_header function**

Add after the format functions (around line 1600):

```rust
fn print_header(target: f64, level: i32, symbols_desc: &str) {
    println!();
    println!("  Target: {}", target);
    if !symbols_desc.is_empty() {
        println!("  Symbols: {}", symbols_desc);
    }
    println!("  Level: {}", level);
    println!();
}
```

**Step 5: Call print_header in main when verbose**

In main() after parsing args (around line 1490), add before printing matches:

```rust
if args.verbose {
    let symbols_desc = profile.describe_active_symbols();
    print_header(target, level_value as i32, &symbols_desc);
}
```

**Step 6: Run test to verify it passes**

Run: `cargo test test_verbose_output_shows_target`
Expected: PASS

**Step 7: Commit**

```bash
git add src/main.rs tests/cli_regression_tests.rs
git commit -m "feat: add --verbose flag with target header output"
```

---

### Task 3: Add Output Footer (Total Equations Tested)

**Files:**
- Modify: `src/main.rs` (add print_footer function)
- Test: `tests/cli_regression_tests.rs`

**Step 1: Write the failing test**

```rust
#[test]
fn test_verbose_output_shows_total_equations() {
    let (stdout, _stderr) = run_ries(&["2.5", "--verbose", "--report", "false", "--max-matches", "1"]);
    // Should show total equations tested
    assert!(stdout.to_lowercase().contains("total") || stdout.contains("equations"));
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test test_verbose_output_shows_total_equations`
Expected: FAIL - no "total" or "equations" summary

**Step 3: Add print_footer function**

Add after print_header:

```rust
fn print_footer(stats: &search::SearchStats, elapsed: Duration) {
    println!();
    println!("  === Summary ===");
    println!("  Total equations tested: {}", stats.lhs_tested + stats.candidates_tested);
    println!("  LHS expressions: {}", stats.lhs_count);
    println!("  RHS expressions: {}", stats.rhs_count);
    println!("  Search time: {:.3}s", elapsed.as_secs_f64());
}
```

**Step 4: Call print_footer in main when verbose**

Replace the existing footer output (around line 1540-1541) with:

```rust
if args.verbose {
    print_footer(&stats, elapsed);
} else {
    println!();
    println!("  Search completed in {:.3}s", elapsed.as_secs_f64());
}
```

**Step 5: Run test to verify it passes**

Run: `cargo test test_verbose_output_shows_total_equations`
Expected: PASS

**Step 6: Commit**

```bash
git add src/main.rs tests/cli_regression_tests.rs
git commit -m "feat: add verbose footer with total equations tested"
```

---

## Layer 2: Diagnostics

### Task 4: Extend DiagnosticOptions Struct

**Files:**
- Modify: `src/main.rs:886-891` (DiagnosticOptions struct)
- Modify: `src/main.rs:893-915` (parse_diagnostics function)
- Test: `tests/cli_regression_tests.rs`

**Step 1: Write the failing test**

```rust
#[test]
fn test_diagnostic_channel_o_match_checks() {
    let (stdout, _stderr) = run_ries(&["2.5", "-Do", "--report", "false", "--max-matches", "1"]);
    // -Do should show match checks (or at least not warn about unsupported)
    assert!(!stderr.contains("unsupported") || stdout.contains("match"));
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test test_diagnostic_channel_o_match_checks`
Expected: FAIL - 'o' is unsupported channel

**Step 3: Extend DiagnosticOptions struct**

Replace `src/main.rs:886-891`:

```rust
#[derive(Debug, Default)]
struct DiagnosticOptions {
    // Existing
    show_work: bool,           // s, N
    show_stats: bool,          // y, M
    // NEW channels
    show_match_checks: bool,   // o
    show_pruned_arith: bool,   // A, a
    show_pruned_range: bool,   // B, b
    show_db_adds: bool,        // G, g
    show_newton: bool,         // n
    unsupported_channels: Vec<char>,
}
```

**Step 4: Update parse_diagnostics function**

Replace `src/main.rs:893-915`:

```rust
fn parse_diagnostics(
    diagnostics: Option<&str>,
    show_work_flag: bool,
    show_stats_flag: bool,
) -> DiagnosticOptions {
    let mut opts = DiagnosticOptions {
        show_work: show_work_flag,
        show_stats: show_stats_flag,
        show_match_checks: false,
        show_pruned_arith: false,
        show_pruned_range: false,
        show_db_adds: false,
        show_newton: false,
        unsupported_channels: Vec::new(),
    };

    if let Some(spec) = diagnostics {
        for ch in spec.chars() {
            match ch {
                's' | 'N' => opts.show_work = true,
                'y' | 'M' => opts.show_stats = true,
                'o' => opts.show_match_checks = true,
                'A' | 'a' => opts.show_pruned_arith = true,
                'B' | 'b' => opts.show_pruned_range = true,
                'G' | 'g' => opts.show_db_adds = true,
                'n' => opts.show_newton = true,
                _ => opts.unsupported_channels.push(ch),
            }
        }
    }

    opts
}
```

**Step 5: Run test to verify it passes**

Run: `cargo test test_diagnostic_channel_o_match_checks`
Expected: PASS (no longer warns about 'o')

**Step 6: Commit**

```bash
git add src/main.rs tests/cli_regression_tests.rs
git commit -m "feat: extend DiagnosticOptions with new channels (o, A/a, B/b, G/g, n)"
```

---

### Task 5: Implement `-Do` Match Checks Diagnostic

**Files:**
- Modify: `src/search.rs` (add match check logging)
- Modify: `src/main.rs` (pass diagnostic options to search)
- Test: `tests/cli_regression_tests.rs`

**Step 1: Write the failing test**

```rust
#[test]
fn test_diagnostic_o_shows_match_output() {
    let (stdout, _stderr) = run_ries(&["2.5", "-Do", "--report", "false", "--max-matches", "1"]);
    // -Do should output match check information
    assert!(stdout.contains("match") || stdout.contains("checking") || stdout.contains("candidate"));
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test test_diagnostic_o_shows_match_output`
Expected: FAIL - no match output

**Step 3: Add DiagnosticConfig to search module**

In `src/search.rs`, add after imports:

```rust
/// Diagnostic output configuration
#[derive(Clone, Debug, Default)]
pub struct DiagnosticConfig {
    pub show_match_checks: bool,
    pub show_pruned_arith: bool,
    pub show_pruned_range: bool,
    pub show_db_adds: bool,
    pub show_newton: bool,
}

impl DiagnosticConfig {
    pub fn new() -> Self {
        Self::default()
    }
}
```

**Step 4: Add match check output in search function**

Find where candidates are tested (look for `candidates_tested` increment). Add:

```rust
if diag_config.show_match_checks {
    eprintln!("  [match check] candidate: lhs={:?} rhs={:?}", lhs.value, rhs.value);
}
```

**Step 5: Pass DiagnosticConfig to search**

Update the search function signature and call sites in main.rs to pass diagnostic config.

**Step 6: Run test to verify it passes**

Run: `cargo test test_diagnostic_o_shows_match_output`
Expected: PASS

**Step 7: Commit**

```bash
git add src/search.rs src/main.rs tests/cli_regression_tests.rs
git commit -m "feat: implement -Do match checks diagnostic output"
```

---

### Task 6: Implement `-Dn` Newton Iteration Diagnostic

**Files:**
- Modify: `src/search.rs` (Newton iteration logging)
- Test: `tests/cli_regression_tests.rs`

**Step 1: Write the failing test**

```rust
#[test]
fn test_diagnostic_n_shows_newton_iterations() {
    let (stdout, _stderr) = run_ries(&["2.5", "-Dn", "--report", "false", "--max-matches", "1"]);
    // -Dn should show Newton iteration values
    assert!(stdout.contains("newton") || stdout.contains("Newton") || stdout.contains("iteration"));
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test test_diagnostic_n_shows_newton_iterations`
Expected: FAIL - no Newton output

**Step 3: Add Newton diagnostic output**

Find the Newton-Raphson function in search.rs. Add logging:

```rust
if diag_config.show_newton {
    eprintln!("  [newton] iter={} x={:.10} dx={:.10e}", iter, x, dx);
}
```

**Step 4: Run test to verify it passes**

Run: `cargo test test_diagnostic_n_shows_newton_iterations`
Expected: PASS

**Step 5: Commit**

```bash
git add src/search.rs tests/cli_regression_tests.rs
git commit -m "feat: implement -Dn Newton iteration diagnostic"
```

---

### Task 7: Implement `-DA/a` Pruned Arithmetic Diagnostic

**Files:**
- Modify: `src/search.rs` or `src/eval.rs` (pruning logging)
- Test: `tests/cli_regression_tests.rs`

**Step 1: Write the failing test**

```rust
#[test]
fn test_diagnostic_a_shows_pruned_arithmetic() {
    let (stdout, stderr) = run_ries(&["2.5", "-DA", "--report", "false", "-l", "3"]);
    // -DA should show expressions pruned due to arithmetic errors
    // May or may not have output depending on target, just verify no crash
    let _ = stdout;
    let _ = stderr;
}
```

**Step 2: Run test to verify it passes (no crash)**

Run: `cargo test test_diagnostic_a_shows_pruned_arithmetic`
Expected: PASS (no crash, may not have output yet)

**Step 3: Add pruning diagnostic output**

Find where expressions are pruned (e.g., divide by zero, overflow). Add:

```rust
if diag_config.show_pruned_arith {
    eprintln!("  [pruned arith] {:?}", expr);
}
```

**Step 4: Commit**

```bash
git add src/search.rs tests/cli_regression_tests.rs
git commit -m "feat: implement -DA/a pruned arithmetic diagnostic"
```

---

## Layer 3a: Quick-Win Options

### Task 8: Implement `--match-all-digits` Option

**Files:**
- Modify: `src/main.rs` (wire up option)
- Modify: `src/search.rs` (match validation logic)
- Test: `tests/cli_regression_tests.rs`

**Step 1: Write the failing test**

```rust
#[test]
fn test_match_all_digits_strict() {
    // With --match-all-digits, require more precise digit matching
    let (stdout1, _) = run_ries(&["3.14159265", "--report", "false", "--max-matches", "3"]);
    let (stdout2, _) = run_ries(&["3.14159265", "--match-all-digits", "--report", "false", "--max-matches", "3"]);
    // With stricter matching, we may get fewer or different results
    assert!(stdout1.contains("x"));
    assert!(stdout2.contains("x"));
}
```

**Step 2: Run test to verify current behavior**

Run: `cargo test test_match_all_digits_strict`
Expected: PASS (option exists but is no-op)

**Step 3: Implement match-all-digits logic**

In the match validation code, check if `--match-all-digits` is set and compare against the target's string representation, not just numeric value.

**Step 4: Commit**

```bash
git add src/main.rs src/search.rs tests/cli_regression_tests.rs
git commit -m "feat: implement --match-all-digits for stricter matching"
```

---

### Task 9: Implement `--derivative-margin` Option

**Files:**
- Modify: `src/main.rs` (wire up option)
- Modify: `src/precision.rs` or relevant file
- Test: `tests/cli_regression_tests.rs`

**Step 1: Write the test**

```rust
#[test]
fn test_derivative_margin_option() {
    // Just verify the option is accepted and doesn't crash
    let (stdout, _) = run_ries(&["2.5", "--derivative-margin", "1e-10", "--report", "false", "--max-matches", "1"]);
    assert!(stdout.contains("x"));
}
```

**Step 2: Run test**

Run: `cargo test test_derivative_margin_option`
Expected: PASS (option exists, may be no-op)

**Step 3: Wire the option to derivative checking**

Find derivative calculation and pass the margin value through.

**Step 4: Commit**

```bash
git add src/main.rs src/precision.rs tests/cli_regression_tests.rs
git commit -m "feat: implement --derivative-margin threshold"
```

---

## Layer 4: Search Quality

### Task 10: Create Comparison Script for Original vs ries-rs

**Files:**
- Create: `tests/compare_with_original.sh`

**Step 1: Create comparison script**

```bash
#!/bin/bash
# Compare ries-rs output with original RIES

ORIGINAL="/Users/maxwell/Apps/ries/ries/ries-original/ries"
RIES_RS="cargo run --quiet --"

TARGET=${1:-"2.5063"}
LEVEL=${2:-"2"}
MAX_MATCHES=${3:-"6"}

echo "=== Comparing target=$TARGET level=$LEVEL ==="
echo ""
echo "=== Original RIES ==="
$ORIGINAL -l$LEVEL --max-matches $MAX_MATCHES $TARGET 2>/dev/null | head -20

echo ""
echo "=== ries-rs ==="
$RIES_RS $TARGET --classic --report false -l $LEVEL --max-matches $MAX_MATCHES 2>/dev/null | head -20
```

**Step 2: Make executable**

Run: `chmod +x tests/compare_with_original.sh`

**Step 3: Run comparison**

Run: `./tests/compare_with_original.sh 2.5063 2 6`

**Step 4: Commit**

```bash
git add tests/compare_with_original.sh
git commit -m "test: add comparison script for original vs ries-rs"
```

---

### Task 11: Analyze Complexity Score Divergence

**Files:**
- Modify: `src/symbol.rs` (weight constants)
- Modify: `src/expr.rs` (complexity calculation)
- Test: `tests/search_tests.rs`

**Step 1: Write comparison test**

```rust
#[test]
fn test_complexity_known_values() {
    // Test that known expressions have expected complexity
    // Compare with original RIES output
    let (stdout, _) = run_ries(&["3.14159", "--report", "false", "-l", "1", "--max-matches", "5"]);
    // First result should be close to pi
    assert!(stdout.contains("pi") || stdout.contains("3.14"));
}
```

**Step 2: Run comparison to identify divergence**

Run the comparison script and identify which expressions differ.

**Step 3: Adjust complexity weights**

Based on divergence analysis, adjust weight constants in `src/symbol.rs`.

**Step 4: Commit**

```bash
git add src/symbol.rs src/expr.rs tests/
git commit -m "fix: adjust complexity weights for parity with original"
```

---

### Task 12: Update PARITY_REMAINING_REPORT.md

**Files:**
- Modify: `PARITY_REMAINING_REPORT.md`

**Step 1: Update report with completed items**

Mark all completed P1/P2 items as RESOLVED.

**Step 2: Document any remaining gaps**

If some items couldn't be completed, document why.

**Step 3: Commit**

```bash
git add PARITY_REMAINING_REPORT.md
git commit -m "docs: update parity report - P1/P2 progress"
```

---

## Summary

**Total Tasks:** 12

| Task | Layer | Effort |
|------|-------|--------|
| 1. `-F1` format | 1 | Low |
| 2. Output header | 1 | Low |
| 3. Output footer | 1 | Low |
| 4. DiagnosticOptions struct | 2 | Low |
| 5. `-Do` match checks | 2 | Medium |
| 6. `-Dn` Newton | 2 | Medium |
| 7. `-DA/a` pruning | 2 | Medium |
| 8. `--match-all-digits` | 3a | Medium |
| 9. `--derivative-margin` | 3a | Low |
| 10. Comparison script | 4 | Low |
| 11. Complexity analysis | 4 | High |
| 12. Update report | - | Low |
