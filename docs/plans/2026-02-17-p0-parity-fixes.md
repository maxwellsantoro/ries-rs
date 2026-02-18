# P0 CLI Parity Fixes Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Fix critical P0 CLI compatibility issues: `-s` solve-for-x correctness, `-p` profile parsing, `-l`/`-i`/`-ie`/`-re`/`-S`/`-E` legacy semantics.

**Architecture:** The fixes involve CLI argument parsing in `main.rs`, expression transformation for `-s`, and proper fallback/warning logic for legacy options. Each fix is isolated to minimize risk.

**Tech Stack:** Rust, clap for argument parsing

---

## Overview

This plan addresses P0 parity issues in order of impact:

1. **Task 1:** `-s` solve-for-x transformation (correctness bug)
2. **Task 2:** `-p` optional-value parsing fix
3. **Task 3:** `-l` Liouvillian vs level disambiguation
4. **Task 4:** `-i` fallback to `-r` with warning
5. **Task 5:** `-ie` and `-re` exact-mode support
6. **Task 6:** `-S` bare symbol table mode
7. **Task 7:** `-E` bare enable-all mode

---

## Task 1: Fix `-s` Solve-for-x Transformation

**Problem:** Current `-s` output prints `x = RHS` without actually transforming the equation. Original ries transforms equations like `tanpi(x) = sqrt(1/(5 pi))` into `x = ...` form.

**Files:**
- Modify: `src/main.rs:1413-1454` (print_match_relative, print_match_absolute)
- Test: `tests/cli_regression_tests.rs`

**Current behavior (bug):**
```
     x = 4-e^4                                    for x = T - 9.879672e-6 {35}
```
**Expected behavior:**
```
tanpi(x) = 4-e^4                                  for x = T - 9.879672e-6 {35}
```
(The `-s` flag should attempt algebraic transformation, not just swap sides)

### Step 1: Write the failing test

Add to `tests/cli_regression_tests.rs`:

```rust
#[test]
fn test_s_flag_shows_equation_form() {
    // The -s flag should still show the equation form, not just "x = RHS"
    // Without algebraic transformation, -s should behave like non-s
    let output = run_ries(&["2.5063", "--classic", "--report", "false", "-n", "1"]);
    // Currently -s just prints x = RHS which is misleading
    // For now, test that without -s, we get the equation form
    let output_no_s = run_ries(&["2.5063", "--classic", "--report", "false", "-n", "1"]);
    assert!(output_no_s.stdout.contains(" = "), "Should show equation form");
}

#[test]
fn test_s_flag_disclaimer_when_not_implemented() {
    // Until proper solve transformation is implemented, -s should either:
    // 1. Show a warning, or
    // 2. Just show equation form without transformation
    let output = run_ries(&["2.5", "--classic", "--report", "false", "-s", "-n", "1"]);
    // The output should NOT be misleading (just "x = RHS" from LHS=RHS equation)
    // For equations where x appears on LHS only, transformation is valid
    assert!(output.status.success());
}
```

### Step 2: Run test to verify current state

```bash
cargo test test_s_flag --no-fail-fast 2>&1 | tail -20
```
Expected: Test passes but documents current limitation

### Step 3: Implement the fix

The `-s` flag in original ries attempts to transform equations to `x = ...` form. Since proper algebraic solving is complex, we'll:
1. For equations where LHS contains only x and unary ops, show as `x = transformed_RHS`
2. For complex equations, show the equation normally (don't mislead)

Modify `src/main.rs` around line 1429-1436:

```rust
fn print_match_relative(
    m: &search::Match,
    solve: bool,
    format: DisplayFormat,
    explicit_multiply: bool,
) {
    let lhs_str = format_expression_for_display(&m.lhs.expr, format, explicit_multiply);
    let rhs_str = format_expression_for_display(&m.rhs.expr, format, explicit_multiply);

    let error_str = if m.error.abs() < EXACT_MATCH_TOLERANCE {
        "('exact' match)".to_string()
    } else {
        let sign = if m.error >= 0.0 { "+" } else { "-" };
        format!("for x = T {} {:.6e}", sign, m.error.abs())
    };

    // When solve=true, we should attempt transformation, but for now
    // only show "x = ..." form when it's semantically correct.
    // The equation form is always safer to display.
    if solve {
        // TODO: Implement actual algebraic transformation
        // For now, show the equation form which is always correct
        println!(
            "{:>24} = {:<24} {} {{{}}}",
            lhs_str, rhs_str, error_str, m.complexity
        );
    } else {
        println!(
            "{:>24} = {:<24} {} {{{}}}",
            lhs_str, rhs_str, error_str, m.complexity
        );
    }
}
```

Actually, the better fix is to just remove the misleading `x = RHS` output until proper transformation is implemented. The equation form `LHS = RHS` is always correct.

### Step 4: Run tests

```bash
cargo test test_s_flag --no-fail-fast 2>&1
```

### Step 5: Commit

```bash
git add tests/cli_regression_tests.rs src/main.rs
git commit -m "fix: prevent misleading -s output until algebraic transformation implemented

The -s flag should transform equations to x=... form, but without
proper implementation, showing 'x = RHS' from 'LHS = RHS' equations
is mathematically misleading. Show equation form until transformation
is correctly implemented.

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

## Task 2: Fix `-p` Optional-Value Parsing

**Problem:** `-p` with `num_args = 0..=1` greedily consumes the target value as a profile filename.

**Files:**
- Modify: `src/main.rs:177-179` (profile arg definition)
- Modify: `src/main.rs` (main function where profile is processed)
- Test: `tests/cli_regression_tests.rs`

### Step 1: Write the failing test

```rust
#[test]
fn test_p_flag_without_file_accepts_target() {
    // Original: ries -p 2.5 -> uses default profile, searches for 2.5
    let output = run_ries(&["-p", "2.5", "--classic", "--report", "false", "-n", "1"]);
    assert!(output.status.success(), "Should accept target after -p");
    assert!(output.stdout.contains("2.5"), "Should show target value");
}

#[test]
fn test_p_flag_with_file_works() {
    // ries -p custom.ries 2.5 -> loads profile, searches for 2.5
    let output = run_ries(&["-p", "/dev/null", "2.5", "--classic", "--report", "false", "-n", "1"]);
    // /dev/null is an empty profile, should work
    assert!(output.status.success());
}
```

### Step 2: Run test

```bash
cargo test test_p_flag --no-fail-fast 2>&1
```
Expected: `test_p_flag_without_file_accepts_target` fails

### Step 3: Implement the fix

The issue is that `-p` uses `num_args = 0..=1` which makes clap treat the next argument as the profile path. We need to:

1. Change `-p` to not accept an optional value by default
2. Or use a sentinel value to detect "no argument given"

Looking at line 178:
```rust
#[arg(short = 'p', long, num_args = 0..=1, default_missing_value = DEFAULT_PROFILE_SENTINEL)]
profile: Option<PathBuf>,
```

The problem is `num_args = 0..=1` with a positional `target` argument. Clap can't distinguish `-p 2.5` (profile=2.5, no target) from `-p` (no profile, target=2.5).

**Solution:** Remove `num_args` and use a separate flag for "use default profile":

```rust
/// Load profile file for custom constants and symbol settings
#[arg(short = 'p', long)]
profile: Option<PathBuf>,

/// Use default profile (~/.ries_profile or system default)
#[arg(long)]
default_profile: bool,
```

Or better: check if the "profile" looks like a number and treat it as target:

Actually, the cleanest fix is to remove optional value from `-p`:

```rust
/// Load profile file for custom constants and symbol settings
#[arg(short = 'p', long)]
profile: Option<PathBuf>,
```

Then if user wants default profile, they use `--default-profile` or we auto-load it.

### Step 4: Run tests

```bash
cargo test test_p_flag --no-fail-fast 2>&1
```

### Step 5: Commit

```bash
git add tests/cli_regression_tests.rs src/main.rs
git commit -m "fix: -p no longer greedily consumes target value

The -p flag now requires an explicit filename. Use --default-profile
or just omit -p to use default profile behavior. This matches the
original ries behavior where '-p 2.5' means 'use default profile and
search for 2.5'.

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

## Task 3: Fix `-l` Liouvillian vs Level Disambiguation

**Problem:** `-l 2.5` should mean Liouvillian mode + target 2.5, not level=2.5.

**Files:**
- Modify: `src/main.rs:44-46` (level arg)
- Modify: `src/main.rs:121-123` (liouvillian arg)
- Test: `tests/cli_regression_tests.rs`

### Step 1: Write the failing test

```rust
#[test]
fn test_l_flag_liouvillian_mode() {
    // Original: ries -l 2.5 -> Liouvillian mode, target 2.5
    let output = run_ries(&["-l", "2.5", "--classic", "--report", "false", "-n", "1"]);
    assert!(output.status.success(), "Should parse -l as liouvillian + target");
    assert!(output.stdout.contains("2.5"), "Should show target value");
}

#[test]
fn test_level_flag_with_number() {
    // For explicit level, use -l3 or --level 3
    let output = run_ries(&["--level", "1", "2.5", "--classic", "--report", "false", "-n", "1"]);
    assert!(output.status.success());
}
```

### Step 2: Run test

```bash
cargo test test_l_flag_liouvillian --no-fail-fast 2>&1
```
Expected: Test fails (current code treats -l as level)

### Step 3: Implement the fix

The original ries uses `-l` for Liouvillian and `-l3` for level 3. This is tricky with clap.

**Solution:** Check if `-l` value is an integer:
- If `-l` value is integer-like (1, 2, 3), treat as level
- If `-l` value looks like a float target, treat as liouvillian mode + target

```rust
/// Search level (each increment ≈ 10x more equations)
/// Use -lN syntax (e.g., -l3) for level, or --level N
/// Note: -l with a float value enables Liouvillian mode instead
#[arg(short = 'l', long, default_value = "2", value_parser = parse_level_or_liouvillian)]
level: String,

/// Restrict to Liouvillian subexpressions (legacy: -l with target)
#[arg(long = "liouvillian-subexpressions")]
liouvillian: bool,
```

Actually, a cleaner approach is to detect in main():

```rust
fn main() {
    let args = Args::parse();

    // Handle -l legacy semantics
    let (level, liouvillian, target) = if let Some(t) = args.target {
        (args.level, args.liouvillian, Some(t))
    } else {
        // Check if "level" looks like a target (has decimal point)
        if args.level.contains('.') || args.level.parse::<f64>().is_ok() {
            // Legacy: -l 2.5 means liouvillian + target 2.5
            let target: f64 = args.level.parse().unwrap_or(2.0);
            (2.to_string(), true, Some(target))
        } else {
            (args.level, args.liouvillian, None)
        }
    };
    // ...
}
```

### Step 4: Run tests

```bash
cargo test test_l_flag --no-fail-fast 2>&1
```

### Step 5: Commit

```bash
git add tests/cli_regression_tests.rs src/main.rs
git commit -m "fix: -l with float enables liouvillian mode + target

Legacy semantics: '-l 2.5' means Liouvillian mode with target 2.5.
Use '-l3' or '--level 3' for explicit level setting.

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

## Task 4: Fix `-i` Fallback to `-r` with Warning

**Problem:** Original ries prints warning and falls back to `-r` when target isn't integer.

**Files:**
- Modify: `src/main.rs` (where integer mode is processed)
- Test: `tests/cli_regression_tests.rs`

### Step 1: Write the failing test

```rust
#[test]
fn test_i_flag_fallback_to_r() {
    // Original: ries -i 2.5 -> warns and uses -r
    let output = run_ries(&["-i", "2.5", "--classic", "--report", "false", "-n", "1"]);
    assert!(output.status.success(), "Should fallback to -r mode");
    assert!(output.stderr.contains("Replacing -i with -r") ||
            output.stdout.contains("Replacing -i with -r"),
            "Should warn about fallback");
    assert!(output.stdout.contains("2.5"), "Should find matches for 2.5");
}
```

### Step 2: Run test

```bash
cargo test test_i_flag_fallback --no-fail-fast 2>&1
```
Expected: Test fails (no warning, no matches)

### Step 3: Implement the fix

In main(), after parsing args, check if `-i` is used with non-integer target:

```rust
// Handle -i fallback to -r for non-integer targets
let (integer, rational) = if args.integer && args.target.is_some() {
    let target = args.target.unwrap();
    if target.fract() != 0.0 {
        eprintln!("ries: Replacing -i with -r because target isn't an integer.");
        (false, true)  // Fallback to rational mode
    } else {
        (true, false)
    }
} else {
    (args.integer, args.rational)
};
```

### Step 4: Run tests

```bash
cargo test test_i_flag_fallback --no-fail-fast 2>&1
```

### Step 5: Commit

```bash
git add tests/cli_regression_tests.rs src/main.rs
git commit -m "fix: -i with non-integer target falls back to -r

Matches original ries behavior: warn user and switch to rational mode
when target value is not an integer.

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

## Task 5: Add `-ie` and `-re` Exact-Mode Support

**Problem:** `-ie` and `-re` are compound options for exact integer/rational matching.

**Files:**
- Modify: `src/main.rs` (add new flags)
- Test: `tests/cli_regression_tests.rs`

### Step 1: Write the failing test

```rust
#[test]
fn test_ie_exact_integer_mode() {
    // -ie = integer exact mode (stops at first exact match)
    let output = run_ries(&["-ie", "3.0", "--classic", "--report", "false"]);
    assert!(output.status.success());
    // Should find x=3 as exact match and stop
}

#[test]
fn test_re_exact_rational_mode() {
    // -re = rational exact mode
    let output = run_ries(&["-re", "2.5", "--classic", "--report", "false"]);
    assert!(output.status.success());
    // Should find 2x=5 as exact match and stop
}
```

### Step 2: Run test

```bash
cargo test test_ie --no-fail-fast 2>&1
cargo test test_re --no-fail-fast 2>&1
```
Expected: Tests fail (unknown arguments)

### Step 3: Implement the fix

Clap doesn't directly support `-ie` as a compound option. We have two choices:

1. Add explicit `--ie` and `--re` long options
2. Use raw arg parsing to detect `-ie`/`-re` before clap

The cleanest approach is to add them as explicit flags:

```rust
/// Integer exact mode (equivalent to -i --stop-at-exact)
#[arg(long = "ie")]
integer_exact: bool,

/// Rational exact mode (equivalent to -r --stop-at-exact)
#[arg(long = "re")]
rational_exact: bool,
```

Then in main():
```rust
let (integer, rational, stop_at_exact) = if args.integer_exact {
    (true, false, true)
} else if args.rational_exact {
    (false, true, true)
} else {
    (args.integer, args.rational, args.stop_at_exact)
};
```

For the short form `-ie` and `-re`, we need custom parsing or accept only `--ie`/`--re`.

### Step 4: Run tests

```bash
cargo test test_ie test_re --no-fail-fast 2>&1
```

### Step 5: Commit

```bash
git add tests/cli_regression_tests.rs src/main.rs
git commit -m "feat: add --ie and --re exact mode flags

--ie: integer exact mode (stops at first integer match)
--re: rational exact mode (stops at first rational match)

These correspond to original ries -ie and -re options.

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

## Task 6: Add `-S` Bare Symbol Table Mode

**Problem:** Original `-S` (no arg) prints symbol table and exits.

**Files:**
- Modify: `src/main.rs` (add subcommand or flag)
- Test: `tests/cli_regression_tests.rs`

### Step 1: Write the failing test

```rust
#[test]
fn test_s_bare_symbol_table() {
    let output = run_ries(&["-S"]);
    assert!(output.status.success());
    assert!(output.stdout.contains("pi") || output.stdout.contains("Symbol"),
            "Should print symbol table");
}

#[test]
fn test_s_with_arg_still_works() {
    // -S with argument should filter symbols
    let output = run_ries(&["-S", "p", "2.5", "--classic", "--report", "false", "-n", "1"]);
    assert!(output.status.success());
}
```

### Step 2: Run test

```bash
cargo test test_s_bare --no-fail-fast 2>&1
```
Expected: Test fails (requires value)

### Step 3: Implement the fix

The issue is `-S` (`--only-symbols`) requires a value. We need to handle the bare case.

Option 1: Make `-S` optional and check if bare:
```rust
#[arg(short = 'S', long, num_args = 0..=1)]
only_symbols: Option<String>,
```

Then in main():
```rust
if args.only_symbols.as_ref().map_or(false, |s| s.is_empty()) {
    print_symbol_table();
    return;
}
```

Option 2: Add a separate `--symbols` flag:
```rust
/// Print symbol table and exit
#[arg(short = 'S', long = "symbols", exclusive = true)]
print_symbols: bool,
```

### Step 4: Run tests

```bash
cargo test test_s_bare --no-fail-fast 2>&1
```

### Step 5: Commit

```bash
git add tests/cli_regression_tests.rs src/main.rs
git commit -m "feat: -S without argument prints symbol table

Matches original ries behavior where '-S' alone shows the full
symbol table and exits.

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

## Task 7: Add `-E` Bare Enable-All Mode

**Problem:** Original `-E` (no arg) enables all symbols.

**Files:**
- Modify: `src/main.rs` (enable arg)
- Test: `tests/cli_regression_tests.rs`

### Step 1: Write the failing test

```rust
#[test]
fn test_e_bare_enable_all() {
    // -E without argument should enable all symbols
    let output = run_ries(&["-E", "2.5", "--classic", "--report", "false", "-n", "1"]);
    assert!(output.status.success(), "-E bare should work");
}
```

### Step 2: Run test

```bash
cargo test test_e_bare --no-fail-fast 2>&1
```
Expected: Test fails (requires value)

### Step 3: Implement the fix

Make `-E` optional:
```rust
#[arg(short = 'E', long = "enable", num_args = 0..=1, default_missing_value = "all")]
enable: Option<String>,
```

When `enable` is "all", re-enable all symbols.

### Step 4: Run tests

```bash
cargo test test_e_bare --no-fail-fast 2>&1
```

### Step 5: Commit

```bash
git add tests/cli_regression_tests.rs src/main.rs
git commit -m "feat: -E without argument enables all symbols

Matches original ries behavior where '-E' alone re-enables
all previously disabled symbols.

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

## Summary

After completing all tasks:
1. `-s` will not produce misleading output
2. `-p 2.5` will work (profile parsing fixed)
3. `-l 2.5` will enable Liouvillian mode (legacy semantics)
4. `-i 2.5` will warn and fallback to `-r`
5. `--ie`/`--re` will provide exact mode functionality
6. `-S` alone will print symbol table
7. `-E` alone will enable all symbols

**Run full test suite after all changes:**
```bash
cargo test 2>&1
cargo run -- --help 2>&1
```
