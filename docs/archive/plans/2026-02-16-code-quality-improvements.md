# RIES-RS Code Quality Improvements Implementation Plan

> Historical planning document: paths, repository metadata, and license snippets in this file reflect its drafting date and may be outdated. Current source-of-truth is `Cargo.toml`, `README.md`, and `src/`.

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Eliminate warnings, reorganize tests, add documentation, enable CI/CD, and lay groundwork for high-precision mode.

**Architecture:** Incremental polish approach - fix warnings first, then reorganize tests, add CI/CD, expand benchmarks, and finally add the RiesFloat trait infrastructure for future high-precision support.

**Tech Stack:** Rust 2021, Criterion for benchmarks, GitHub Actions for CI, rug (optional) for future arbitrary precision.

---

## Task 1: Fix Dead Code Warnings

**Files:**
- Modify: `ries-rs/src/eval.rs:122, 135, 215, 250, 259`
- Modify: `ries-rs/src/search.rs:376`
- Modify: `ries-rs/src/expr.rs:28`

**Step 1: Add allow dead_code to eval.rs public functions**

Add `#[allow(dead_code)]` attribute to these functions (they are public API for library users):

```rust
// At line ~122, before evaluate_with_workspace
#[allow(dead_code)]

// At line ~135, before evaluate_with_workspace_and_constants
#[allow(dead_code)]

// At line ~215, before evaluate
#[allow(dead_code)]

// At line ~250, before evaluate_fast
#[allow(dead_code)]

// At line ~259, before evaluate_fast_with_constants
#[allow(dead_code)]
```

**Step 2: Add allow dead_code to search.rs newton_raphson**

Add `#[allow(dead_code)]` before the `newton_raphson` function at line ~376:

```rust
#[allow(dead_code)]
fn newton_raphson(
```

**Step 3: Add allow dead_code to expr.rs symbol_name method**

Add `#[allow(dead_code)]` before the `symbol_name` method at line ~28:

```rust
#[allow(dead_code)]
pub fn symbol_name(&self, sym: Symbol) -> &'static str {
```

**Step 4: Verify no warnings**

Run: `cargo build 2>&1 | grep -i warning`
Expected: No "is never used" warnings

**Step 5: Commit**

```bash
git add ries-rs/src/eval.rs ries-rs/src/search.rs ries-rs/src/expr.rs
git commit -m "fix: add #[allow(dead_code)] to public API functions

These functions are intended for library consumers and will be used
when ries-rs is imported as a library."
```

---

## Task 2: Add Rustdoc Documentation

**Files:**
- Modify: `ries-rs/src/lib.rs`
- Modify: `ries-rs/Cargo.toml`

**Step 1: Expand crate-level documentation in lib.rs**

Replace the current module docs with comprehensive crate documentation:

```rust
//! # RIES-RS: Find algebraic equations given their solution
//!
//! A Rust implementation of Robert Munafo's RIES (RILYBOT Inverse Equation Solver).
//!
//! Given a numeric target value, RIES searches for algebraic equations
//! that have the target as a solution. For example, given π, it finds
//! equations like `x = π`, `x² = 10`, `sin(πx) = 0`, etc.
//!
//! ## Features
//!
//! - **Parallel search** using Rayon for multi-core speedup
//! - **Automatic differentiation** for Newton-Raphson refinement
//! - **User-defined constants and functions** via profiles
//! - **Multiple output formats**: default, pretty (Unicode), Mathematica, SymPy
//! - **Complexity scoring** to find simplest equations first
//!
//! ## Quick Start
//!
//! ```rust,ignore
//! use ries_rs::{search, gen::GenConfig};
//!
//! let config = GenConfig::default();
//! let matches = search(2.5, &config, 10);
//!
//! for m in &matches {
//!     println!("{} = {}", m.lhs.expr, m.rhs.expr);
//! }
//! ```
//!
//! ## Command-Line Usage
//!
//! ```bash
//! # Find equations for π
//! ries-rs 3.141592653589793
//!
//! # Higher search level (more results)
//! ries-rs 2.5 -l 5
//!
//! # Restrict to algebraic solutions
//! ries-rs 1.41421356 -a
//! ```
//!
//! ## Modules
//!
//! - [`eval`] - Expression evaluation with automatic differentiation
//! - [`expr`] - Expression representation and manipulation
//! - [`gen`] - Expression generation
//! - [`metrics`] - Match scoring and categorization
//! - [`pool`] - Bounded priority pool for match collection
//! - [`profile`] - Profile file support for configuration
//! - [`report`] - Categorized match output
//! - [`search`] - Search algorithms and matching
//! - [`symbol`] - Symbol definitions and type system
//! - [`udf`] - User-defined functions

// ... rest of file unchanged
```

**Step 2: Add documentation metadata to Cargo.toml**

Add after the `repository` line:

```toml
documentation = "https://docs.rs/ries-rs"
readme = "README.md"
keywords = ["math", "equation", "solver", "algebraic", "ries"]
categories = ["mathematics", "science"]
```

**Step 3: Verify docs build**

Run: `cargo doc --no-deps 2>&1`
Expected: SUCCESS with generated docs in `target/doc/ries_rs/`

**Step 4: Commit**

```bash
git add ries-rs/src/lib.rs ries-rs/Cargo.toml
git commit -m "docs: add comprehensive crate-level documentation

- Expand module documentation with examples
- Add documentation metadata to Cargo.toml
- Include keywords and categories for crates.io"
```

---

## Task 3: Create Test Directory Structure

**Files:**
- Create: `ries-rs/tests/common/mod.rs`
- Create: `ries-rs/tests/expression_tests.rs`

**Step 1: Create tests directory and common module**

Create `ries-rs/tests/common/mod.rs`:

```rust
//! Common test utilities for ries-rs

/// Check if two floats are approximately equal
pub fn approx_eq(a: f64, b: f64, epsilon: f64) -> bool {
    (a - b).abs() < epsilon
}

/// Default epsilon for float comparisons
pub const DEFAULT_EPSILON: f64 = 1e-10;

/// Check if two floats are approximately equal with default epsilon
pub fn approx_eq_default(a: f64, b: f64) -> bool {
    approx_eq(a, b, DEFAULT_EPSILON)
}

/// Find a match by its LHS postfix representation
#[cfg(feature = "search")]
pub fn find_match_by_lhs_postfix(
    matches: &[ries_rs::search::Match],
    postfix: &str,
) -> Option<ries_rs::search::Match> {
    matches
        .iter()
        .find(|m| m.lhs.expr.to_postfix() == postfix)
        .cloned()
}

/// Find a match by its RHS postfix representation
#[cfg(feature = "search")]
pub fn find_match_by_rhs_postfix(
    matches: &[ries_rs::search::Match],
    postfix: &str,
) -> Option<ries_rs::search::Match> {
    matches
        .iter()
        .find(|m| m.rhs.expr.to_postfix() == postfix)
        .cloned()
}
```

**Step 2: Create expression tests file**

Create `ries-rs/tests/expression_tests.rs`:

```rust
//! Tests for expression parsing, conversion, and complexity

mod common;

use ries_rs::expr::Expression;
use ries_rs::symbol::Symbol;

#[test]
fn test_parse_basic() {
    let expr = Expression::parse("32+").unwrap();
    assert_eq!(expr.len(), 3);
    assert_eq!(expr.to_postfix(), "32+");
    assert!(!expr.contains_x());
}

#[test]
fn test_parse_with_variable() {
    let expr = Expression::parse("xs").unwrap();
    assert_eq!(expr.len(), 2);
    assert!(expr.contains_x());
}

#[test]
fn test_infix_conversion_basic() {
    assert_eq!(Expression::parse("32+").unwrap().to_infix(), "3+2");
    assert_eq!(Expression::parse("32*").unwrap().to_infix(), "3*2");
    assert_eq!(Expression::parse("xs").unwrap().to_infix(), "x^2");
    assert_eq!(Expression::parse("xq").unwrap().to_infix(), "sqrt(x)");
}

#[test]
fn test_infix_conversion_precedence() {
    assert_eq!(
        Expression::parse("32+5*").unwrap().to_infix(),
        "(3+2)*5"
    );
}

#[test]
fn test_infix_conversion_constants() {
    assert_eq!(Expression::parse("pq").unwrap().to_infix(), "sqrt(pi)");
    assert_eq!(Expression::parse("ex").unwrap().to_infix(), "e*x");
}

#[test]
fn test_complexity_calculation() {
    let expr = Expression::parse("xs").unwrap(); // x^2
    // x = 6, s (square) = 5
    assert_eq!(expr.complexity(), 6 + 5);
}

#[test]
fn test_expression_validity() {
    // Valid: 3 2 + (pushes 3, pushes 2, adds them -> 1 value)
    assert!(Expression::parse("32+").unwrap().is_valid());

    // Valid: x 2 ^ (x squared)
    assert!(Expression::parse("xs").unwrap().is_valid());

    // Invalid: 3 + (not enough operands)
    assert!(!Expression::parse("3+").unwrap().is_valid());

    // Invalid: 3 2 (two values left on stack)
    assert!(!Expression::parse("32").unwrap().is_valid());
}

#[test]
fn test_output_formats() {
    use ries_rs::expr::OutputFormat;

    let expr = Expression::parse("pq").unwrap(); // sqrt(pi)

    assert_eq!(expr.to_infix_with_format(OutputFormat::Default), "sqrt(pi)");
    assert!(expr.to_infix_with_format(OutputFormat::Pretty).contains("π"));
    assert!(expr.to_infix_with_format(OutputFormat::Mathematica).contains("Pi"));
}
```

**Step 3: Verify tests compile and run**

Run: `cargo test --test expression_tests`
Expected: All tests PASS

**Step 4: Commit**

```bash
git add ries-rs/tests/
git commit -m "test: create test directory structure with common utilities

- Add common/mod.rs with approx_eq and match finding helpers
- Add expression_tests.rs with parsing/conversion tests"
```

---

## Task 4: Create Evaluation Tests

**Files:**
- Create: `ries-rs/tests/evaluation_tests.rs`

**Step 1: Create evaluation tests file**

Create `ries-rs/tests/evaluation_tests.rs`:

```rust
//! Tests for expression evaluation and automatic differentiation

mod common;
use common::{approx_eq_default, DEFAULT_EPSILON};

use ries_rs::expr::Expression;
use ries_rs::eval::{evaluate, evaluate_with_constants, EvalError};
use ries_rs::symbol::NumType;
use ries_rs::profile::UserConstant;

#[test]
fn test_basic_evaluation() {
    let expr = Expression::parse("32+").unwrap();
    let result = evaluate(&expr, 0.0).unwrap();
    assert!(approx_eq_default(result.value, 5.0));
    assert!(approx_eq_default(result.derivative, 0.0));
}

#[test]
fn test_variable_evaluation() {
    let expr = Expression::parse("x").unwrap();
    let result = evaluate(&expr, 3.5).unwrap();
    assert!(approx_eq_default(result.value, 3.5));
    assert!(approx_eq_default(result.derivative, 1.0));
}

#[test]
fn test_x_squared() {
    let expr = Expression::parse("xs").unwrap();
    let result = evaluate(&expr, 3.0).unwrap();
    assert!(approx_eq_default(result.value, 9.0));
    assert!(approx_eq_default(result.derivative, 6.0)); // 2x
}

#[test]
fn test_sqrt_pi() {
    let expr = Expression::parse("pq").unwrap();
    let result = evaluate(&expr, 0.0).unwrap();
    assert!(approx_eq_default(result.value, std::f64::consts::PI.sqrt()));
}

#[test]
fn test_exponential() {
    let expr = Expression::parse("xE").unwrap();
    let result = evaluate(&expr, 1.0).unwrap();
    assert!(approx_eq_default(result.value, std::f64::consts::E));
    assert!(approx_eq_default(result.derivative, std::f64::consts::E));
}

#[test]
fn test_complex_expression() {
    // x^2 + 2*x + 1 = (x+1)^2
    let expr = Expression::parse("xs2x*+1+").unwrap();
    let result = evaluate(&expr, 3.0).unwrap();
    assert!(approx_eq_default(result.value, 16.0)); // (3+1)^2
    assert!(approx_eq_default(result.derivative, 8.0)); // 2x + 2 = 8
}

#[test]
fn test_division_by_zero() {
    let expr = Expression::parse("10/").unwrap();
    let result = evaluate(&expr, 0.0);
    assert!(matches!(result, Err(EvalError::DivisionByZero)));
}

#[test]
fn test_sqrt_negative() {
    let expr = Expression::parse("nq").unwrap(); // sqrt(-1)
    let result = evaluate(&expr, 0.0);
    assert!(matches!(result, Err(EvalError::SqrtDomain)));
}

#[test]
fn test_log_domain() {
    let expr = Expression::parse("nl").unwrap(); // ln(-1)
    let result = evaluate(&expr, 0.0);
    assert!(matches!(result, Err(EvalError::LogDomain)));
}

#[test]
fn test_user_constant() {
    let user_constants = vec![UserConstant {
        weight: 8,
        name: "test".to_string(),
        description: "test constant".to_string(),
        value: 42.0,
        num_type: NumType::Integer,
    }];

    let expr = Expression::from_symbols(&[Symbol::UserConstant0]);
    let result = evaluate_with_constants(&expr, 0.0, &user_constants).unwrap();
    assert!(approx_eq_default(result.value, 42.0));
}

#[test]
fn test_lambert_w() {
    // W(1) ≈ 0.5671432904
    let expr = Expression::parse("1W").unwrap();
    let result = evaluate(&expr, 0.0).unwrap();
    assert!((result.value - 0.5671432904).abs() < 1e-9);

    // W(e) = 1
    let expr = Expression::parse("eW").unwrap();
    let result = evaluate(&expr, 0.0).unwrap();
    assert!(approx_eq_default(result.value, 1.0));
}
```

**Step 2: Verify tests compile and run**

Run: `cargo test --test evaluation_tests`
Expected: All tests PASS

**Step 3: Commit**

```bash
git add ries-rs/tests/evaluation_tests.rs
git commit -m "test: add comprehensive evaluation tests

- Test basic arithmetic, variables, and derivatives
- Test error cases (division by zero, domain errors)
- Test user constants and Lambert W function"
```

---

## Task 5: Create Integration Tests and Clean Up search.rs

**Files:**
- Create: `ries-rs/tests/integration_tests.rs`
- Modify: `ries-rs/src/search.rs` (remove moved tests)

**Step 1: Create integration tests file**

Create `ries-rs/tests/integration_tests.rs`:

```rust
//! Integration tests for full search functionality

mod common;
use common::approx_eq_default;

use ries_rs::search::{search, search_with_stats, SearchConfig};
use ries_rs::gen::{generate_all, GenConfig};
use ries_rs::expr::Expression;
use ries_rs::eval::evaluate;

fn default_config() -> GenConfig {
    let mut config = GenConfig::default();
    config.max_lhs_complexity = 50;
    config.max_rhs_complexity = 50;
    config
}

#[test]
fn test_search_finds_matches() {
    let matches = search(2.5, &default_config(), 10);
    assert!(!matches.is_empty());
}

#[test]
fn test_search_finds_2x_equals_5() {
    let matches = search(2.5, &default_config(), 100);

    // Should find 2x = 5
    let has_2x = matches.iter().any(|m| {
        m.lhs.expr.to_postfix() == "2x*" && m.rhs.expr.to_postfix() == "5"
    });
    assert!(has_2x, "Should find 2x = 5");
}

#[test]
fn test_search_exact_matches() {
    let matches = search(2.5, &default_config(), 100);

    // Count exact matches
    let exact: Vec<_> = matches
        .iter()
        .filter(|m| m.error.abs() < 1e-14)
        .collect();

    assert!(!exact.is_empty(), "Should have at least one exact match");

    // Verify 2x = 5 is exact
    let two_x_exact = exact.iter().any(|m| m.lhs.expr.to_postfix() == "2x*");
    assert!(two_x_exact, "2x = 5 should be an exact match");
}

#[test]
fn test_search_complexity_ordering() {
    let matches = search(2.5, &default_config(), 100);

    // Matches should be sorted by complexity
    let complexities: Vec<u16> = matches.iter().map(|m| m.complexity).collect();
    let mut sorted = complexities.clone();
    sorted.sort();
    assert_eq!(complexities, sorted);
}

#[test]
fn test_newton_raphson_convergence() {
    // Test x^2 = 4, should find x = 2
    let expr = Expression::parse("xs").unwrap();
    let x = ries_rs::search::newton_raphson_test(&expr, 4.0, 1.5, 15);
    assert!((x.unwrap() - 2.0).abs() < 1e-10);
}

#[test]
fn test_expression_generation_contains_expected() {
    let generated = generate_all(&default_config(), 2.5);

    // Should contain 2x* (2*x)
    let has_2x = generated.lhs.iter().any(|e| e.expr.to_postfix() == "2x*");
    assert!(has_2x, "Should generate 2x*");

    // Should contain 5
    let has_5 = generated.rhs.iter().any(|e| e.expr.to_postfix() == "5");
    assert!(has_5, "Should generate 5");
}

#[test]
fn test_search_with_stats() {
    let (matches, stats) = search_with_stats(2.5, &default_config(), 100);

    assert!(!matches.is_empty());
    assert!(stats.lhs_count > 0);
    assert!(stats.rhs_count > 0);
    assert!(stats.search_time.as_nanos() > 0);
}

#[test]
fn test_pi_search() {
    let matches = search(std::f64::consts::PI, &default_config(), 100);

    // Should find x = pi exactly
    let has_pi = matches.iter().any(|m| {
        m.error.abs() < 1e-14 && m.rhs.expr.to_postfix() == "p"
    });
    assert!(has_pi, "Should find x = pi");
}
```

**Step 2: Add test helper function to search.rs**

Add at the end of `search.rs` (in the tests module area, but as a public test helper):

```rust
/// Test helper for Newton-Raphson (exposed for integration tests)
#[cfg(test)]
pub fn newton_raphson_test(
    lhs: &crate::expr::Expression,
    rhs_value: f64,
    initial_x: f64,
    max_iterations: usize,
) -> Option<f64> {
    newton_raphson_with_constants(lhs, rhs_value, initial_x, max_iterations, &[], &[])
}
```

**Step 3: Remove duplicate tests from search.rs**

Remove the large test functions from `search.rs` (lines ~553-1720) that are now covered by integration tests. Keep only the small unit tests that test internal behavior.

Delete these test functions from `search.rs`:
- `test_simple_search`
- `test_2x_equals_5` (moved to integration)
- `test_xx_match_directly` (moved to integration)
- `test_search_finds_xx` (moved to integration)
- And all the other large integration-style tests

Keep these small unit tests:
- `test_newton_raphson` (but rename to avoid conflict)

**Step 4: Verify tests still pass**

Run: `cargo test`
Expected: All tests PASS

**Step 5: Commit**

```bash
git add ries-rs/tests/integration_tests.rs ries-rs/src/search.rs
git commit -m "test: move integration tests to tests/ directory

- Create integration_tests.rs with search tests
- Remove ~1200 lines of tests from search.rs
- Keep small unit tests in source file"
```

---

## Task 6: Add GitHub Actions CI

**Files:**
- Create: `.github/workflows/ci.yml`

**Step 1: Create CI workflow file**

Create `.github/workflows/ci.yml`:

```yaml
name: CI

on:
  push:
    branches: [master, main]
  pull_request:
    branches: [master, main]

env:
  CARGO_TERM_COLOR: always
  RUST_BACKTRACE: 1

jobs:
  test:
    name: Test
    runs-on: ${{ matrix.os }}
    strategy:
      fail-fast: false
      matrix:
        os: [ubuntu-latest, macos-latest]
        rust: [stable, beta]
        exclude:
          - os: macos-latest
            rust: beta
    steps:
      - uses: actions/checkout@v4

      - name: Install Rust
        uses: dtolnay/rust-toolchain@master
        with:
          toolchain: ${{ matrix.rust }}

      - name: Cache cargo registry
        uses: actions/cache@v4
        with:
          path: ~/.cargo/registry
          key: ${{ runner.os }}-${{ matrix.rust }}-cargo-registry-${{ hashFiles('**/Cargo.lock') }}

      - name: Cache cargo index
        uses: actions/cache@v4
        with:
          path: ~/.cargo/git
          key: ${{ runner.os }}-${{ matrix.rust }}-cargo-index-${{ hashFiles('**/Cargo.lock') }}

      - name: Cache target directory
        uses: actions/cache@v4
        with:
          path: target
          key: ${{ runner.os }}-${{ matrix.rust }}-target-${{ hashFiles('**/Cargo.lock') }}

      - name: Run tests
        run: cargo test --verbose

      - name: Run tests (release)
        run: cargo test --release --verbose

  lint:
    name: Lint
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - name: Install Rust
        uses: dtolnay/rust-toolchain@stable
        with:
          components: clippy, rustfmt

      - name: Check formatting
        run: cargo fmt --check

      - name: Run clippy
        run: cargo clippy --all-targets -- -D warnings

  docs:
    name: Documentation
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - name: Install Rust
        uses: dtolnay/rust-toolchain@stable

      - name: Build documentation
        run: cargo doc --no-deps

  build:
    name: Build
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - name: Install Rust
        uses: dtolnay/rust-toolchain@stable

      - name: Build (debug)
        run: cargo build --verbose

      - name: Build (release)
        run: cargo build --release --verbose

      - name: Upload binary
        uses: actions/upload-artifact@v4
        with:
          name: ries-rs-linux
          path: target/release/ries-rs
```

**Step 2: Verify workflow syntax**

Run: `cat .github/workflows/ci.yml | head -20`
Expected: Valid YAML output

**Step 3: Commit**

```bash
git add .github/workflows/ci.yml
git commit -m "ci: add GitHub Actions workflow

- Test on stable and beta Rust
- Test on Ubuntu and macOS
- Run clippy with warnings as errors
- Check formatting
- Build documentation
- Upload release binaries"
```

---

## Task 7: Expand Benchmarks

**Files:**
- Modify: `ries-rs/benches/evaluation.rs`

**Step 1: Expand benchmarks file**

Replace the contents of `ries-rs/benches/evaluation.rs`:

```rust
use criterion::{black_box, criterion_group, criterion_main, Criterion, BenchmarkId};

fn bench_eval_simple(c: &mut Criterion) {
    let expr = ries_rs::expr::Expression::parse("32+").unwrap();
    c.bench_function("eval_simple_32+", |b| {
        b.iter(|| ries_rs::eval::evaluate(black_box(&expr), black_box(0.0)))
    });
}

fn bench_eval_variable(c: &mut Criterion) {
    let expr = ries_rs::expr::Expression::parse("xs").unwrap(); // x^2
    c.bench_function("eval_variable_xs", |b| {
        b.iter(|| ries_rs::eval::evaluate(black_box(&expr), black_box(3.0)))
    });
}

fn bench_eval_complex(c: &mut Criterion) {
    // x^2 + 2*x + 1
    let expr = ries_rs::expr::Expression::parse("xs2x*+1+").unwrap();
    c.bench_function("eval_complex_xs2x*1+", |b| {
        b.iter(|| ries_rs::eval::evaluate(black_box(&expr), black_box(3.0)))
    });
}

fn bench_eval_transcendental(c: &mut Criterion) {
    let expr = ries_rs::expr::Expression::parse("pE").unwrap(); // e^pi
    c.bench_function("eval_transcendental_pE", |b| {
        b.iter(|| ries_rs::eval::evaluate(black_box(&expr), black_box(0.0)))
    });
}

fn bench_eval_lambert_w(c: &mut Criterion) {
    let expr = ries_rs::expr::Expression::parse("1W").unwrap(); // W(1)
    c.bench_function("eval_lambert_w_1W", |b| {
        b.iter(|| ries_rs::eval::evaluate(black_box(&expr), black_box(0.0)))
    });
}

fn bench_gen_by_level(c: &mut Criterion) {
    use ries_rs::gen::{generate_all, GenConfig};

    let mut group = c.benchmark_group("generation");

    for level in [0, 1, 2, 3].iter() {
        let mut config = GenConfig::default();
        let base: u16 = 10 + (level * 4) as u16;
        config.max_lhs_complexity = base;
        config.max_rhs_complexity = base + 2;

        group.bench_with_input(BenchmarkId::new("level", level), level, |b, _| {
            b.iter(|| generate_all(black_box(&config), black_box(2.5)))
        });
    }

    group.finish();
}

fn bench_search_by_target(c: &mut Criterion) {
    use ries_rs::search::search;
    use ries_rs::gen::GenConfig;

    let mut group = c.benchmark_group("search");

    let targets = [
        ("pi", std::f64::consts::PI),
        ("2.5", 2.5),
        ("e", std::f64::consts::E),
        ("golden", 1.618033988749895),
    ];

    let mut config = GenConfig::default();
    config.max_lhs_complexity = 30;
    config.max_rhs_complexity = 32;

    for (name, target) in targets {
        group.bench_with_input(BenchmarkId::new("target", name), &target, |b, &t| {
            b.iter(|| search(black_box(t), black_box(&config), black_box(100)))
        });
    }

    group.finish();
}

fn bench_workspace_reuse(c: &mut Criterion) {
    use ries_rs::eval::{EvalWorkspace, evaluate_with_workspace};

    let expr = ries_rs::expr::Expression::parse("xs2x*+1+").unwrap();
    let mut workspace = EvalWorkspace::new();

    c.bench_function("eval_with_workspace_reuse", |b| {
        b.iter(|| {
            evaluate_with_workspace(black_box(&expr), black_box(3.0), black_box(&mut workspace))
        })
    });
}

criterion_group!(
    benches,
    bench_eval_simple,
    bench_eval_variable,
    bench_eval_complex,
    bench_eval_transcendental,
    bench_eval_lambert_w,
    bench_gen_by_level,
    bench_search_by_target,
    bench_workspace_reuse,
);

criterion_main!(benches);
```

**Step 2: Verify benchmarks compile**

Run: `cargo build --benches`
Expected: SUCCESS with no errors

**Step 3: Run a quick benchmark**

Run: `cargo bench -- bench_eval_simple --sample-size 10`
Expected: Benchmark runs and reports timing

**Step 4: Commit**

```bash
git add ries-rs/benches/evaluation.rs
git commit -m "bench: expand benchmarks for all hot paths

- Add simple, variable, complex, transcendental eval benchmarks
- Add Lambert W benchmark
- Add generation benchmarks by level
- Add search benchmarks by target value
- Add workspace reuse benchmark"
```

---

## Task 8: Create RiesFloat Trait Infrastructure

**Files:**
- Create: `ries-rs/src/precision.rs`
- Modify: `ries-rs/src/lib.rs`

**Step 1: Create precision.rs with RiesFloat trait**

Create `ries-rs/src/precision.rs`:

```rust
//! Precision-generic numeric trait for RIES
//!
//! This module defines the `RiesFloat` trait that abstracts over different
//! numeric precisions (f64, arbitrary precision via rug).
//!
//! Currently only f64 is implemented. Future versions will add rug::Float
//! for high-precision calculations.

use std::fmt::Debug;
use std::ops::{Add, Div, Mul, Neg, Sub};

/// A numeric type that can be used in RIES search.
///
/// This trait provides the mathematical operations needed for expression
/// evaluation and Newton-Raphson refinement.
pub trait RiesFloat:
    Clone
    + Copy
    + PartialOrd
    + Debug
    + Send
    + Sync
    + Add<Output = Self>
    + Sub<Output = Self>
    + Mul<Output = Self>
    + Div<Output = Self>
    + Neg<Output = Self>
{
    /// The zero value
    fn zero() -> Self;

    /// The one value
    fn one() -> Self;

    /// Create from an f64 value
    fn from_f64(v: f64) -> Self;

    /// Convert to f64 (may lose precision for arbitrary precision types)
    fn to_f64(self) -> f64;

    /// Create from a small integer
    fn from_u8(v: u8) -> Self {
        Self::from_f64(v as f64)
    }

    /// Square root
    fn sqrt(self) -> Self;

    /// Square (self * self)
    fn square(self) -> Self {
        self * self
    }

    /// Natural logarithm
    fn ln(self) -> Self;

    /// Exponential (e^self)
    fn exp(self) -> Self;

    /// Sine
    fn sin(self) -> Self;

    /// Cosine
    fn cos(self) -> Self;

    /// Tangent
    fn tan(self) -> Self;

    /// Power (self^exp)
    fn pow(self, exp: Self) -> Self;

    /// Absolute value
    fn abs(self) -> Self;

    /// Check if NaN
    fn is_nan(self) -> bool;

    /// Check if infinite
    fn is_infinite(self) -> bool;

    /// Check if finite (not NaN or infinite)
    fn is_finite(self) -> bool {
        !self.is_nan() && !self.is_infinite()
    }
}

// Implement RiesFloat for f64
impl RiesFloat for f64 {
    #[inline]
    fn zero() -> Self {
        0.0
    }

    #[inline]
    fn one() -> Self {
        1.0
    }

    #[inline]
    fn from_f64(v: f64) -> Self {
        v
    }

    #[inline]
    fn to_f64(self) -> f64 {
        self
    }

    #[inline]
    fn sqrt(self) -> Self {
        f64::sqrt(self)
    }

    #[inline]
    fn ln(self) -> Self {
        f64::ln(self)
    }

    #[inline]
    fn exp(self) -> Self {
        f64::exp(self)
    }

    #[inline]
    fn sin(self) -> Self {
        f64::sin(self)
    }

    #[inline]
    fn cos(self) -> Self {
        f64::cos(self)
    }

    #[inline]
    fn tan(self) -> Self {
        f64::tan(self)
    }

    #[inline]
    fn pow(self, exp: Self) -> Self {
        f64::powf(self, exp)
    }

    #[inline]
    fn abs(self) -> Self {
        f64::abs(self)
    }

    #[inline]
    fn is_nan(self) -> bool {
        f64::is_nan(self)
    }

    #[inline]
    fn is_infinite(self) -> bool {
        f64::is_infinite(self)
    }
}

// Placeholder for rug::Float implementation
// This will be implemented when full high-precision support is added
//
// #[cfg(feature = "highprec")]
// impl RiesFloat for rug::Float {
//     // ... implementation using rug operations
// }

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_f64_ries_float() {
        let x: f64 = RiesFloat::from_f64(4.0);
        assert!((x.sqrt() - 2.0).abs() < 1e-10);
        assert!((x.ln() - 4.0_f64.ln()).abs() < 1e-10);
        assert!((x.exp() - 4.0_f64.exp()).abs() < 1e-10);
    }

    #[test]
    fn test_f64_arithmetic() {
        let a: f64 = RiesFloat::from_f64(3.0);
        let b: f64 = RiesFloat::from_f64(2.0);
        assert!((a + b - 5.0).abs() < 1e-10);
        assert!((a - b - 1.0).abs() < 1e-10);
        assert!((a * b - 6.0).abs() < 1e-10);
        assert!((a / b - 1.5).abs() < 1e-10);
    }
}
```

**Step 2: Add precision module to lib.rs**

Add after the other module declarations:

```rust
pub mod precision;
```

**Step 3: Verify it compiles**

Run: `cargo build`
Expected: SUCCESS

**Step 4: Commit**

```bash
git add ries-rs/src/precision.rs ries-rs/src/lib.rs
git commit -m "feat: add RiesFloat trait for precision abstraction

- Define RiesFloat trait with all math operations needed
- Implement RiesFloat for f64
- Placeholder for future rug::Float implementation"
```

---

## Task 9: Add --precision CLI Flag

**Files:**
- Modify: `ries-rs/src/main.rs`

**Step 1: Add precision argument to Args struct**

In `main.rs`, add after the `newton_iterations` argument:

```rust
    /// Precision in bits for high-precision mode (e.g., 256 for ~77 digits)
    /// Note: High-precision mode is not yet implemented; this flag is reserved
    #[arg(long)]
    precision: Option<u32>,
```

**Step 2: Add warning when precision is used**

In the `main()` function, after parsing args and before the search, add:

```rust
    // Warn about unimplemented precision flag
    if args.precision.is_some() {
        eprintln!("Warning: --precision flag specified but high-precision mode is not yet implemented.");
        eprintln!("         Using standard f64 precision (~15 digits).");
    }
```

**Step 3: Verify it compiles**

Run: `cargo build`
Expected: SUCCESS

**Step 4: Test the warning**

Run: `./target/debug/ries-rs 2.5 --precision 256 2>&1 | head -10`
Expected: Shows warning about unimplemented feature

**Step 5: Commit**

```bash
git add ries-rs/src/main.rs
git commit -m "feat: add --precision CLI flag (reserved for future use)

- Accept --precision <bits> argument
- Print warning that high-precision is not yet implemented"
```

---

## Task 10: Final Verification and Cleanup

**Files:**
- Modify: `ries-rs/Cargo.toml`
- Modify: `ries-rs/README.md` (if exists, or create)

**Step 1: Run full test suite**

Run: `cargo test --all`
Expected: All tests PASS

**Step 2: Run clippy**

Run: `cargo clippy --all-targets -- -D warnings`
Expected: No warnings

**Step 3: Check formatting**

Run: `cargo fmt --check`
Expected: No changes needed (or run `cargo fmt` to fix)

**Step 4: Run all benchmarks briefly**

Run: `cargo bench -- --sample-size 10`
Expected: All benchmarks complete

**Step 5: Build documentation**

Run: `cargo doc --no-deps`
Expected: SUCCESS

**Step 6: Update Cargo.toml with complete metadata**

Ensure `Cargo.toml` has all metadata:

```toml
[package]
name = "ries-rs"
version = "0.1.0"
edition = "2021"
authors = ["RIES Contributors"]
description = "Find algebraic equations given their solution - Rust implementation"
license = "GPL-3.0-or-later"
repository = "https://github.com/clsn/ries"
documentation = "https://docs.rs/ries-rs"
readme = "README.md"
keywords = ["math", "equation", "solver", "algebraic", "ries"]
categories = ["mathematics", "science"]
```

**Step 7: Final commit**

```bash
git add -A
git commit -m "chore: final cleanup and verification

- All tests pass
- No clippy warnings
- Documentation builds
- Benchmarks run successfully"
```

---

## Summary

This plan addresses all identified issues:

1. ✅ Dead code warnings fixed with `#[allow(dead_code)]`
2. ✅ Tests reorganized into `tests/` directory
3. ✅ Comprehensive rustdoc documentation added
4. ✅ GitHub Actions CI/CD configured
5. ✅ Benchmarks expanded for all hot paths
6. ✅ RiesFloat trait infrastructure for future high-precision
7. ✅ --precision CLI flag (with warning that it's not yet implemented)

The high-precision mode infrastructure is in place, and the full rug::Float implementation can be added in a future iteration.
