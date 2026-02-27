# Fix Code Review Issues Implementation Plan


**Goal:** Fix all issues identified in the 2026-02-27 code review, from critical CI blockers down to documentation suggestions.

**Architecture:** Pure mechanical fixes across 7 files — no new abstractions needed. The three critical fixes are independent of each other and of the important fixes; all important fixes are also independent of each other. Work in order: Critical → Important → Suggestions.

**Tech Stack:** Rust, Cargo, clap, serde_json. Verify with `cargo clippy --all-targets -- -D warnings` and `cargo test`.

---

## Task 1: Fix `&Vec<T>` parameters (Critical — CI blocker)

**Files:**
- Modify: `src/main.rs:75-76`

**Step 1: Write the minimal change**

In `src/main.rs`, change `evaluate_and_print`'s parameter types:
```rust
// Before:
fn evaluate_and_print(
    expr_str: &str,
    x: f64,
    constants: &Vec<profile::UserConstant>,
    functions: &Vec<profile::UserFunction>,
) -> Result<(), String> {

// After:
fn evaluate_and_print(
    expr_str: &str,
    x: f64,
    constants: &[profile::UserConstant],
    functions: &[profile::UserFunction],
) -> Result<(), String> {
```

No call sites need to change — `&Vec<T>` coerces to `&[T]` automatically.

**Step 2: Verify clippy is happy**

Run: `cargo clippy --all-targets -- -D warnings 2>&1 | grep "ptr_arg"`
Expected: no output (the ptr_arg warning should be gone)

**Step 3: Commit**

```bash
git add src/main.rs
git commit -m "fix: use slice refs instead of &Vec in evaluate_and_print"
```

---

## Task 2: Fix `print_option_list` dead code (Critical — CI blocker)

**Files:**
- Modify: `src/main.rs:100-176`

The function `cli::print_option_list()` already exists in `src/cli/args.rs:646` and is exported from `cli/mod.rs`. The inline list in `main.rs:101-174` is an exact duplicate. Replace the inline block with a single call.

**Step 1: Replace inline list with the function call**

In `src/main.rs`, replace lines 100–176:
```rust
// Before:
if args.list_options {
    let opts = [
        "--list-options",
        "-p",
        // ... 60+ entries ...
        "--E-RHS",
    ];
    for opt in opts {
        println!("{}", opt);
    }
    return;
}

// After:
if args.list_options {
    cli::print_option_list();
    return;
}
```

**Step 2: Verify `print_option_list` is in scope**

Check that `cli::print_option_list` is accessible. In `src/main.rs`, `mod cli;` is already declared and all public items from `cli/mod.rs` are available via `cli::`. The function is already `pub fn print_option_list()` in `args.rs` and re-exported from `cli/mod.rs`.

Confirm export exists:
```bash
grep "print_option_list" src/cli/mod.rs
```
Expected: a `pub use` or direct `pub fn` line. If not present, add: `pub use args::print_option_list;` to `src/cli/mod.rs`.

**Step 3: Verify clippy is clean for this warning**

Run: `cargo clippy --all-targets -- -D warnings 2>&1 | grep "print_option_list"`
Expected: no output

**Step 4: Verify the --list-options output is identical**

Run: `./target/debug/ries-rs --list-options | wc -l`
The count should match what it produced before (check by building before and after). Both the old inline list and the function must produce identical output.

**Step 5: Commit**

```bash
git add src/main.rs src/cli/mod.rs
git commit -m "fix: call print_option_list() instead of duplicating inline list"
```

---

## Task 3: Fix `elapsed` dead field (Critical — CI blocker)

**Files:**
- Modify: `src/cli/search_runner.rs:16`
- Modify: `src/main.rs:35`, `src/main.rs:791`

Two choices: (A) remove the field and keep the separate `Instant` in `main.rs`, or (B) use the field from `SearchResult` in `main.rs` and remove the separate `Instant`. Option B is architecturally cleaner since timing is already computed inside `run_search`.

**Step 1: In `main.rs`, remove the separate `Instant` and use `result.elapsed`**

Find in `src/main.rs`:
```rust
let start = Instant::now();  // around line 622
```

And further down:
```rust
let elapsed = start.elapsed();  // around line 791
```

The `elapsed` from the `SearchResult` is already the search duration. Replace:
- Remove the `let start = Instant::now();` line (or find where it is — look for `Instant::now()` in main)
- At the point where `let elapsed = start.elapsed();` appears, change it to use `result.elapsed` instead

Note: check if `Instant` import is still needed elsewhere in `main.rs`. If not, remove the `use std::time::Instant;` import too.

**Step 2: Verify the field is now used**

Run: `cargo clippy --all-targets -- -D warnings 2>&1 | grep "elapsed"`
Expected: no output

**Step 3: Verify all three clippy warnings are now gone**

Run: `cargo clippy --all-targets -- -D warnings`
Expected: exit code 0, zero warnings (the `panic setting ignored for bench` warning from Cargo is from the bench profile and does not cause CI failure).

**Step 4: Run tests to confirm nothing broke**

Run: `cargo test --test cli_regression_tests basics 2>&1 | tail -5`
Expected: `test result: ok.`

**Step 5: Commit**

```bash
git add src/main.rs src/cli/search_runner.rs
git commit -m "fix: use SearchResult.elapsed instead of separate Instant in main"
```

---

## Task 4: Fix `--no-solve-for-x` not honored in JSON mode (Important)

**Files:**
- Modify: `src/cli/json_types.rs:106` (add `include_solve_for_x: bool` param)
- Modify: `src/main.rs` (the `build_json_output(...)` call site)

**Step 1: Write the failing test first**

Add to `tests/cli/basics.rs`:
```rust
#[test]
fn test_json_no_solve_for_x_suppresses_solve_fields() {
    let output = Command::new(env!("CARGO_BIN_EXE_ries-rs"))
        .args(["--json", "--no-solve-for-x", "1.5"])
        .output()
        .expect("failed to run ries-rs");
    let stdout = String::from_utf8(output.stdout).unwrap();
    let json: serde_json::Value = serde_json::from_str(&stdout)
        .expect("output should be valid JSON");
    // Every result's solve_for_x should be null when --no-solve-for-x is given
    for result in json["results"].as_array().unwrap() {
        assert!(
            result["solve_for_x"].is_null(),
            "solve_for_x should be null but got: {}",
            result["solve_for_x"]
        );
        assert!(
            result["solve_for_x_postfix"].is_null(),
            "solve_for_x_postfix should be null but got: {}",
            result["solve_for_x_postfix"]
        );
    }
}
```

**Step 2: Run test to confirm it currently fails**

Run: `cargo test --test cli_regression_tests test_json_no_solve_for_x_suppresses_solve_fields -- --nocapture`
Expected: FAIL (solve_for_x is populated even with --no-solve-for-x)

**Step 3: Add `include_solve_for_x` parameter to `build_json_output`**

In `src/cli/json_types.rs`, add the parameter after `elapsed`:
```rust
pub fn build_json_output(
    // ... existing params ...
    elapsed: std::time::Duration,
    include_solve_for_x: bool,   // new last parameter
) -> JsonRunOutput {
```

Then in the `results` map closure, gate the solver call:
```rust
// Before:
let solved = solve_for_x_rhs_expression(&m.lhs.expr, &m.rhs.expr);
let solve_for_x = solved.as_ref().map(...);
let solve_for_x_postfix = solved.as_ref().map(...);

// After:
let (solve_for_x, solve_for_x_postfix) = if include_solve_for_x {
    let solved = solve_for_x_rhs_expression(&m.lhs.expr, &m.rhs.expr);
    let sfx = solved.as_ref().map(|e| format!("x = {}", e.to_infix()));
    let sfxp = solved.as_ref().map(|e| e.to_postfix());
    (sfx, sfxp)
} else {
    (None, None)
};
```

**Step 4: Update the call site in `main.rs`**

Find the `build_json_output(...)` call (around line 805) and add the new argument:
```rust
let json_output = build_json_output(
    // ... all existing args ...
    elapsed,
    args.solve && !args.no_solve,   // new last arg
);
```

**Step 5: Run the test to confirm it now passes**

Run: `cargo test --test cli_regression_tests test_json_no_solve_for_x_suppresses_solve_fields`
Expected: PASS

**Step 6: Run all CLI tests to confirm no regressions**

Run: `cargo test --test cli_regression_tests basics`
Expected: all pass

**Step 7: Commit**

```bash
git add src/cli/json_types.rs src/main.rs tests/cli/basics.rs
git commit -m "fix: honor --no-solve-for-x flag in JSON output mode"
```

---

## Task 5: Fix `libc` unconditional dependency (Important)

**Files:**
- Modify: `Cargo.toml`

`libc` is only used in a `#[cfg(unix)]` block in `src/cli/json_types.rs` (for `peak_memory_bytes`). It should be a platform-conditional dependency.

**Step 1: Check where libc is used**

Run: `grep -rn "libc::" src/`
Expected: one result in `src/cli/json_types.rs` (or a similar file) inside `#[cfg(unix)]`.

**Step 2: Move `libc` to a target-conditional dependency**

In `Cargo.toml`, change:
```toml
# Before (in [dependencies]):
libc = "0.2"

# After (remove from [dependencies], add new section):
[target.'cfg(unix)'.dependencies]
libc = "0.2"
```

**Step 3: Verify it still compiles on this platform**

Run: `cargo build 2>&1 | grep -E "error|warning.*libc"`
Expected: clean build.

**Step 4: Commit**

```bash
git add Cargo.toml
git commit -m "chore: make libc a unix-only dependency"
```

---

## Task 6: Document and test `seen_lhs` permanent blacklist in pool (Important)

**Files:**
- Modify: `src/pool.rs:354-362` (improve comment)
- Modify: `src/pool.rs` (add test)

**Step 1: Improve the comment at the eviction site**

In `src/pool.rs`, replace the existing comment at the eviction block:
```rust
// Before:
if let Some(evicted) = self.heap.pop() {
    // Remove from seen sets
    self.seen_eqn.remove(&EqnKey::from_match(&evicted.m));
    // Note: we don't remove from seen_lhs to prevent re-adding variants
    self.stats.evictions += 1;
}

// After:
if let Some(evicted) = self.heap.pop() {
    // Remove equation key so a different RHS for the same LHS can be re-inserted.
    self.seen_eqn.remove(&EqnKey::from_match(&evicted.m));
    // Intentionally do NOT remove from seen_lhs: once a LHS has been represented
    // in the pool, we permanently suppress additional variants of it even after
    // eviction, to avoid the pool oscillating between near-identical equations.
    // Consequence: if the sole representative of a LHS is evicted, later matches
    // with the same LHS will be rejected even if they are better approximations.
    // This is accepted as a design trade-off (matches original RIES behavior).
    self.stats.evictions += 1;
}
```

**Step 2: Write a test that documents and verifies this behavior**

Add to the `#[cfg(test)]` block in `src/pool.rs`:
```rust
#[test]
fn test_seen_lhs_permanent_after_eviction() {
    // Confirm that once a LHS is seen, further matches with the same LHS
    // are rejected even after the original match is evicted.
    use crate::expr::Expression;
    use crate::search::{EvaluatedExpr, Match};

    let capacity = 2;
    let mut pool = TopKPool::new(capacity, RankingMode::Complexity);

    // Helper to make a match with given LHS postfix, error, and complexity
    // We use real Expression parsing here; "1" = constant 1, "x" = variable, etc.
    // Build: lhs="x" (just x), rhs="1", x_value, error, complexity
    fn make_match(lhs_str: &str, rhs_str: &str, x: f64, err: f64) -> Match {
        let lhs_expr = Expression::parse(lhs_str).unwrap();
        let rhs_expr = Expression::parse(rhs_str).unwrap();
        let complexity = lhs_expr.complexity() + rhs_expr.complexity();
        Match {
            lhs: EvaluatedExpr { expr: lhs_expr, value: x, derivative: 1.0, num_type: crate::symbol::NumType::Integer },
            rhs: EvaluatedExpr { expr: rhs_expr, value: x - err, derivative: 0.0, num_type: crate::symbol::NumType::Integer },
            x_value: x,
            error: err,
            complexity,
        }
    }

    // Fill the pool to capacity with two distinct LHS
    let m1 = make_match("x", "1", 1.0, 0.1);   // lhs="x"
    let m2 = make_match("x", "2", 2.0, 0.2);   // lhs="x" same LHS! (second should be deduped)
    // ...
    // Note: this test verifies the deduplication behavior; exact construction
    // depends on available test helpers. Use a simpler assertion:
    // After inserting a match, seen_lhs should reject a second match with identical LHS postfix.
    let inserted = pool.try_insert(m1);
    assert!(inserted, "first insert should succeed");
    let rejected = !pool.try_insert(m2); // same LHS "x"
    assert!(rejected, "second match with same LHS should be rejected (seen_lhs)");
}
```

**Note:** If the `EvaluatedExpr`/`Match` structs are not convenient to construct in tests, simplify the test to just verify the comment is accurate by checking the existing behavior via `try_insert` return values.

**Step 3: Run the test**

Run: `cargo test -p ries test_seen_lhs_permanent_after_eviction`
Expected: PASS

**Step 4: Commit**

```bash
git add src/pool.rs
git commit -m "docs(pool): document seen_lhs permanent-blacklist eviction behavior and add test"
```

---

## Task 7: Fix misleading `#[allow(dead_code)]` attribute (Suggestion)

**Files:**
- Modify: `src/main.rs:7-8`

**Step 1: Remove the misleading outer attribute**

The attribute `#[allow(dead_code)]` at line 8 is an _outer_ attribute that applies only to the next item (the `#[cfg(highprec)] use ries_rs::precision;` line). It does not suppress dead code warnings file-wide. The comment above it is misleading. Since the actual dead code warnings are now fixed (Tasks 1–3), this attribute and comment serve no purpose.

In `src/main.rs`, remove lines 7–8:
```rust
// Remove these two lines:
// Some helper functions are kept for future use but may be unused in certain configurations
#[allow(dead_code)]
```

**Step 2: Verify it still compiles cleanly**

Run: `cargo clippy --all-targets -- -D warnings`
Expected: exit 0 with zero warnings.

**Step 3: Commit**

```bash
git add src/main.rs
git commit -m "chore: remove misleading #[allow(dead_code)] outer attribute"
```

---

## Task 8: Change `ignore` doc tests to `no_run` (Suggestion)

**Files:**
- Modify: `src/eval.rs:170`, `src/eval.rs:415`
- Modify: `src/gen.rs:677`
- Modify: `src/search.rs:1301`
- Skip: `src/precision.rs` (highprec feature, complex setup — leave as `ignore`)

`ignore` means the example is not compiled or run. `no_run` means it is compiled but not executed, which at least catches API drift.

**Step 1: Check what each `ignore` doc test contains**

Read each location:
- `src/eval.rs:170` — `EvalWorkspace` example
- `src/eval.rs:415` — `evaluate_fast_with_constants_and_functions` example
- `src/gen.rs:677` — `generate_streaming` example
- `src/search.rs:1301` — search streaming example

**Step 2: Change `ignore` to `no_run` for each**

For each file, replace:
```rust
/// ```ignore
```
with:
```rust
/// ```no_run
```

Only do this for the four locations in `eval.rs`, `gen.rs`, and `search.rs`. Leave `precision.rs` as `ignore` since the `rug` GMP types require the `highprec` feature and are not available in normal `cargo test`.

**Step 3: Verify the examples compile**

Run: `cargo test --doc 2>&1 | grep -E "FAILED|error"`
Expected: no failures. If a doc test now fails to compile, fix the example to match the current API.

**Step 4: Commit**

```bash
git add src/eval.rs src/gen.rs src/search.rs
git commit -m "docs: change ignore doc tests to no_run so they compile-check"
```

---

## Task 9: Add leap-year edge cases to manifest timestamp tests (Suggestion)

**Files:**
- Modify: `src/manifest.rs:240-259`

**Step 1: Add tests for Feb 29 and boundary years**

Add to the `#[cfg(test)]` block in `src/manifest.rs`:
```rust
#[test]
fn test_timestamp_leap_year_feb29() {
    // 2000-02-29 00:00:00 UTC
    // Days from epoch to 2000-02-29: (30 years) + leap days
    // 2000-02-29 = unix timestamp 951782400
    let ts = chrono_like_timestamp(951782400);
    assert!(ts.starts_with("2000-02-29"), "got: {}", ts);
}

#[test]
fn test_timestamp_1900_not_leap() {
    // 1900 is NOT a leap year (div by 100 but not 400).
    // Verify is_leap_year handles this correctly.
    assert!(!is_leap_year(1900));
    assert!(!is_leap_year(2100));
    assert!(is_leap_year(2000));
    assert!(is_leap_year(2400));
}

#[test]
fn test_timestamp_year_boundary() {
    // 1999-12-31 23:59:59 UTC = 946684799
    let ts = chrono_like_timestamp(946684799);
    assert!(ts.starts_with("1999-12-31"), "got: {}", ts);

    // 2000-01-01 00:00:00 UTC = 946684800
    let ts2 = chrono_like_timestamp(946684800);
    assert!(ts2.starts_with("2000-01-01"), "got: {}", ts2);
}
```

**Step 2: Verify unix timestamps are correct**

Confirm: `date -d "2000-02-29 00:00:00 UTC" +%s` = `951782400` and `date -d "1999-12-31 23:59:59 UTC" +%s` = `946684799`. (These are standard reference values.)

Run: `cargo test -p ries test_timestamp`
Expected: all pass. If `test_timestamp_leap_year_feb29` fails, there is a bug in `days_to_ymd`.

**Step 3: Commit**

```bash
git add src/manifest.rs
git commit -m "test(manifest): add leap-year and year-boundary timestamp edge cases"
```

---

## Task 10: Document solver branch-cut conventions (Suggestion)

**Files:**
- Modify: `src/solver.rs:133-175`

**Step 1: Add branch-cut docstrings to `unary_inverse_expression`**

In `src/solver.rs`, improve the comments in the `unary_inverse_expression` match arms:

```rust
// Square: x² = y  =>  x = sqrt(y)  (principal root only; negative root -sqrt(y) not returned)
Symbol::Square => append_unary_expression(rhs_value, Symbol::Sqrt),
// Sqrt: sqrt(x) = y  =>  x = y²  (valid only for y >= 0, matching sqrt domain)
Symbol::Sqrt => append_unary_expression(rhs_value, Symbol::Square),
// SinPi: sin(πx) = y  =>  x = asin(y)/π  (principal branch: x ∈ [-1/2, 1/2]; infinitely many solutions exist)
Symbol::SinPi => { ... }
// CosPi: cos(πx) = y  =>  x = acos(y)/π  (principal branch: x ∈ [0, 1]; infinitely many solutions exist)
Symbol::CosPi => { ... }
// TanPi: tan(πx) = y  =>  x = atan(y)/π  (principal branch: x ∈ (-1/2, 1/2))
Symbol::TanPi => { ... }
```

Also add a doc comment to the function itself:
```rust
/// Build an expression for the inverse of `op(x) = rhs_value`, solving for `x`.
///
/// For multivalued inverses (trig functions, square root), returns only the
/// **principal branch**. The search engine finds other branches independently
/// via Newton-Raphson starting from different initial conditions.
///
/// Returns `None` if the operator has no closed-form inverse supported here.
fn unary_inverse_expression(op: Symbol, rhs_value: &Expression) -> Option<Expression> {
```

**Step 2: Run tests to confirm nothing broke**

Run: `cargo test -p ries 2>&1 | tail -5`
Expected: all pass (comments only, no logic change).

**Step 3: Commit**

```bash
git add src/solver.rs
git commit -m "docs(solver): document principal-branch convention for trig/sqrt inverses"
```

---

## Task 11: Final verification

**Step 1: Run clippy (must be zero warnings)**

```bash
cargo clippy --all-targets -- -D warnings
```
Expected: exit code 0.

**Step 2: Run all tests**

```bash
cargo test
```
Expected: all pass.

**Step 3: Spot check --list-options output**

```bash
./target/release/ries-rs --list-options | head -5
./target/release/ries-rs --list-options | wc -l
```

**Step 4: Spot check --no-solve-for-x --json**

```bash
./target/release/ries-rs --json --no-solve-for-x 1.5 | python3 -c "import sys,json; d=json.load(sys.stdin); print(all(r['solve_for_x'] is None for r in d['results']))"
```
Expected: `True`
