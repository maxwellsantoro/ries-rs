# Review Recommendations Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Address the prioritized recommendations from the comprehensive code review to improve code quality, documentation, and test coverage.

**Architecture:** Incremental refactoring with frequent commits. Each phase focuses on a single concern, maintaining backward compatibility. The plan follows TDD where applicable and ensures all tests pass after each commit.

**Tech Stack:** Rust, cargo, clap, proptest, cargo-tarpaulin

---

## Phase 1: High Priority - CONTRIBUTING.md (Quick Win)

### Task 1.1: Create CONTRIBUTING.md

**Files:**
- Create: `CONTRIBUTING.md`

**Step 1: Create the contributing guide**

Create a comprehensive CONTRIBUTING.md that covers:
- Development environment setup
- Building and testing instructions
- PR process and code style guidelines
- Feature flag documentation

```markdown
# Contributing to RIES-RS

Thank you for your interest in contributing to RIES-RS! This document provides guidelines and instructions for contributing.

## Development Setup

### Prerequisites

- Rust 1.70 or later (install via [rustup](https://rustup.rs/))
- For `highprec` feature: GMP and MPFR libraries
  - Ubuntu/Debian: `sudo apt-get install libgmp-dev libmpfr-dev`
  - macOS: `brew install gmp mpfr`
  - Windows: Use MSYS2 or WSL

### Building

```bash
# Standard build
cargo build

# With all optional features
cargo build --all-features

# Python bindings (requires maturin)
cargo build --features python
```

### Testing

```bash
# Run all tests
cargo test

# Run tests with highprec feature
cargo test --features highprec

# Run specific test file
cargo test --test integration_tests

# Run property tests only
cargo test --test property_tests
```

## Code Style

We use standard Rust conventions:

```bash
# Format code
cargo fmt

# Check for linting issues
cargo clippy --all-targets -- -D warnings
```

### Key Conventions

1. **Documentation**: All public APIs must have rustdoc comments
2. **Error handling**: Use `thiserror` for error types
3. **Testing**: New features require unit tests; mathematical features require property tests
4. **Commits**: Follow conventional commit format (`feat:`, `fix:`, `docs:`, `refactor:`)

## Project Structure

```
src/
├── lib.rs          # Library entry point, re-exports
├── main.rs         # CLI binary entry
├── cli/            # CLI argument parsing and output
├── expr.rs         # Expression representation
├── eval.rs         # Expression evaluation with AD
├── gen.rs          # Expression generation
├── search.rs       # Search and matching
├── symbol.rs       # Symbol definitions
└── thresholds.rs   # Named constants
```

## Feature Flags

| Flag | Description | Dependencies |
|------|-------------|--------------|
| `parallel` | Multi-threaded search (default) | rayon |
| `highprec` | Arbitrary precision arithmetic | rug, GMP, MPFR |
| `python` | Python bindings via PyO3 | pyo3 |
| `wasm` | WebAssembly bindings | wasm-bindgen |

## Pull Request Process

1. Fork the repository and create a feature branch
2. Make your changes with appropriate tests
3. Ensure all CI checks pass:
   - `cargo fmt -- --check`
   - `cargo clippy --all-targets -- -D warnings`
   - `cargo test --tests`
4. Update documentation if changing public APIs
5. Submit PR with a clear description of changes

## Running Benchmarks

```bash
cargo bench
```

## Questions?

Open an issue for bugs, feature requests, or questions about the codebase.
```

**Step 2: Commit**

```bash
git add CONTRIBUTING.md
git commit -m "docs: add CONTRIBUTING.md with development guidelines"
```

---

## Phase 2: High Priority - Coverage Thresholds

### Task 2.1: Add coverage threshold to CI

**Files:**
- Modify: `.github/workflows/coverage.yml:22`

**Step 1: Update coverage workflow to fail on threshold**

Modify the "Generate coverage" step to include a threshold:

```yaml
      - name: Generate coverage
        run: cargo tarpaulin --locked --out Xml --output-dir ./coverage --timeout 300 --fail-under 70
```

**Step 2: Add coverage summary job**

Add after the coverage job:

```yaml
      - name: Report coverage summary
        run: |
          echo "## Coverage Report" >> $GITHUB_STEP_SUMMARY
          echo "Coverage report generated and uploaded as artifact." >> $GITHUB_STEP_SUMMARY
```

**Step 3: Commit**

```bash
git add .github/workflows/coverage.yml
git commit -m "ci: add 70% coverage threshold to CI"
```

---

## Phase 3: High Priority - main.rs Refactoring

### Task 3.1: Extract search orchestration to cli/search_runner.rs

**Files:**
- Create: `src/cli/search_runner.rs`
- Modify: `src/cli/mod.rs`
- Modify: `src/main.rs`

**Step 1: Create search_runner.rs with run_search function**

Extract the core search logic from main() into a dedicated module:

```rust
//! Search execution and orchestration
//!
//! This module handles the coordination of search execution,
//! including configuration building and result processing.

use crate::gen::GenConfig;
use crate::profile::Profile;
use crate::search::{Match, SearchConfig, SearchStats};
use crate::symbol::NumType;
use std::time::Instant;

/// Result of a search operation
pub struct SearchResult {
    pub matches: Vec<Match>,
    pub stats: SearchStats,
    pub elapsed: std::time::Duration,
}

/// Orchestrates the search process
pub fn run_search(
    target: f64,
    gen_config: GenConfig,
    search_config: SearchConfig,
) -> SearchResult {
    let start = Instant::now();
    let (matches, stats) = crate::search::search_with_stats(&gen_config, &search_config, target);
    let elapsed = start.elapsed();

    SearchResult {
        matches,
        stats,
        elapsed,
    }
}
```

**Step 2: Update cli/mod.rs to include new module**

Add to `src/cli/mod.rs`:

```rust
pub mod search_runner;
pub use search_runner::{run_search, SearchResult};
```

**Step 3: Update main.rs to use new module**

Replace the direct search call in main() with:

```rust
let result = cli::run_search(resolved_target.unwrap(), gen_config, search_config);
```

**Step 4: Run tests to verify**

```bash
cargo test
```

**Step 5: Commit**

```bash
git add src/cli/search_runner.rs src/cli/mod.rs src/main.rs
git commit -m "refactor(cli): extract search orchestration to search_runner module"
```

### Task 3.2: Extract config building to cli/config_builder.rs

**Files:**
- Create: `src/cli/config_builder.rs`
- Modify: `src/cli/mod.rs`
- Modify: `src/main.rs`

**Step 1: Create config_builder.rs**

Extract `build_gen_config()` and `build_search_config()` functions:

```rust
//! Configuration builders from CLI arguments
//!
//! Converts parsed CLI arguments into runtime configuration structs.

use crate::gen::GenConfig;
use crate::search::SearchConfig;
use crate::profile::{Profile, UserConstant};
use crate::udf::UserFunction;
use crate::symbol::NumType;

/// Build GenConfig from CLI arguments
pub fn build_gen_config(
    max_lhs_complexity: u32,
    max_rhs_complexity: u32,
    min_type: NumType,
    exclude: Option<&str>,
    enable: Option<&str>,
    only_symbols: Option<&str>,
    exclude_rhs: Option<&str>,
    enable_rhs: Option<&str>,
    only_symbols_rhs: Option<&str>,
    op_limits: Option<&str>,
    op_limits_rhs: Option<&str>,
    user_constants: Vec<UserConstant>,
    user_functions: Vec<UserFunction>,
    show_pruned_arith: bool,
) -> Result<GenConfig, String> {
    // ... (move existing implementation from main.rs)
}

/// Build SearchConfig from CLI arguments
pub fn build_search_config(/* args */) -> Result<SearchConfig, String> {
    // ... (move existing implementation from main.rs)
}
```

**Step 2: Update cli/mod.rs**

```rust
pub mod config_builder;
pub use config_builder::{build_gen_config, build_search_config};
```

**Step 3: Update imports in main.rs**

Change the import to use the new module:

```rust
use cli::{build_gen_config, build_search_config, ...};
```

**Step 4: Run tests to verify**

```bash
cargo test
```

**Step 5: Commit**

```bash
git add src/cli/config_builder.rs src/cli/mod.rs src/main.rs
git commit -m "refactor(cli): extract config building to config_builder module"
```

### Task 3.3: Extract legacy argument handling to cli/legacy.rs

**Files:**
- Create: `src/cli/legacy.rs`
- Modify: `src/cli/mod.rs`
- Modify: `src/main.rs`

**Step 1: Create legacy.rs**

Extract the legacy argument normalization logic:

```rust
//! Legacy argument handling for backward compatibility
//!
//! Handles quirky legacy behaviors from original RIES:
//! - `-p 2.5` means target 2.5, not profile "2.5"
//! - `-l 2.5` means liouvillian + target 2.5
//! - `-E 2.5` means enable-all + target 2.5

/// Normalized arguments after legacy handling
pub struct NormalizedArgs {
    pub target: Option<f64>,
    pub profile: Option<String>,
    pub enable: Option<String>,
    pub level: f32,
    pub liouvillian: bool,
}

/// Normalize legacy argument semantics
pub fn normalize_legacy_args(
    profile_arg: Option<&str>,
    enable_arg: Option<&str>,
    level_arg: &str,
    explicit_target: Option<f64>,
) -> NormalizedArgs {
    // ... (move existing implementation from main.rs)
}
```

**Step 2: Update cli/mod.rs**

```rust
pub mod legacy;
pub use legacy::{normalize_legacy_args, NormalizedArgs};
```

**Step 3: Run tests to verify**

```bash
cargo test
```

**Step 4: Commit**

```bash
git add src/cli/legacy.rs src/cli/mod.rs src/main.rs
git commit -m "refactor(cli): extract legacy argument handling to legacy module"
```

---

## Phase 4: Medium Priority - WASM Tests

### Task 4.1: Create WASM test infrastructure

**Files:**
- Create: `tests/wasm_tests.rs`
- Modify: `Cargo.toml`

**Step 1: Add wasm-bindgen-test dependency**

Add to `Cargo.toml` dev-dependencies:

```toml
wasm-bindgen-test = "0.3"
```

**Step 2: Create basic WASM tests**

```rust
//! WASM binding tests
//!
//! Tests for WebAssembly bindings. Run with:
//! ```bash
//! wasm-pack test --node
//! ```

#![cfg(all(feature = "wasm", target_arch = "wasm32"))]

use wasm_bindgen_test::*;

#[wasm_bindgen_test]
fn test_wasm_basic_search() {
    // Test that basic search works through WASM bindings
    let result = ries_rs::search(2.0, &ries_rs::GenConfig::default(), 5);
    assert!(!result.is_empty());
}

#[wasm_bindgen_test]
fn test_wasm_expression_formatting() {
    use ries_rs::{Expression, OutputFormat};

    let expr = Expression::parse("x2*").unwrap();
    let formatted = expr.format(&OutputFormat::Default);
    assert!(formatted.contains("x"));
}
```

**Step 3: Add CI job for WASM tests**

Add to `.github/workflows/ci.yml`:

```yaml
  test-wasm:
    name: Test (WASM)
    runs-on: ubuntu-latest
    steps:
      - name: Checkout
        uses: actions/checkout@v4

      - name: Setup Rust
        uses: dtolnay/rust-toolchain@stable
        with:
          targets: wasm32-unknown-unknown

      - name: Install wasm-pack
        uses: jetli/wasm-pack-action@v0.4.0

      - name: Run WASM tests
        run: wasm-pack test --node --features wasm
```

**Step 4: Commit**

```bash
git add tests/wasm_tests.rs Cargo.toml .github/workflows/ci.yml
git commit -m "test(wasm): add WASM binding tests with CI job"
```

---

## Phase 5: Medium Priority - Config Documentation

### Task 5.1: Add rustdoc to GenConfig

**Files:**
- Modify: `src/gen.rs`

**Step 1: Document GenConfig fields**

Add documentation to the GenConfig struct:

```rust
/// Configuration for expression generation
///
/// Controls which symbols are available, complexity limits,
/// and various generation options.
///
/// # Example
///
/// ```rust
/// use ries_rs::gen::GenConfig;
///
/// let config = GenConfig {
///     max_lhs_complexity: 50,
///     max_rhs_complexity: 30,
///     ..GenConfig::default()
/// };
/// ```
#[derive(Clone, Debug)]
pub struct GenConfig {
    /// Maximum complexity score for left-hand-side expressions.
    /// Higher values allow more complex equations but take longer to search.
    /// Default: 52 (complexity level 2)
    pub max_lhs_complexity: u32,

    /// Maximum complexity score for right-hand-side expressions.
    /// Default: 32
    pub max_rhs_complexity: u32,

    /// Minimum numeric type required for expressions.
    /// Use to restrict to algebraic, rational, or integer results.
    /// Default: NumType::Real
    pub min_num_type: NumType,

    /// Maximum number of symbols in an expression.
    /// Default: 15
    pub max_length: usize,

    /// Whether to generate LHS expressions (containing x).
    /// Default: true
    pub generate_lhs: bool,

    /// Whether to generate RHS expressions (not containing x).
    /// Default: true
    pub generate_rhs: bool,

    // ... (continue for all fields)
}
```

**Step 2: Commit**

```bash
git add src/gen.rs
git commit -m "docs(gen): add comprehensive rustdoc to GenConfig"
```

### Task 5.2: Add rustdoc to SearchConfig

**Files:**
- Modify: `src/search.rs`

**Step 1: Document SearchConfig fields**

```rust
/// Configuration for the search process
///
/// Controls matching thresholds, result limits, and search behavior.
///
/// # Example
///
/// ```rust
/// use ries_rs::search::SearchConfig;
///
/// let config = SearchConfig {
///     tolerance: 1e-10,
///     max_matches: 20,
///     ..SearchConfig::default()
/// };
/// ```
#[derive(Clone, Debug)]
pub struct SearchConfig {
    /// Tolerance for considering a match exact.
    /// Expressions within this distance from the target are marked as exact matches.
    /// Default: 1e-14
    pub tolerance: f64,

    /// Maximum number of matches to return.
    /// Default: 100
    pub max_matches: usize,

    // ... (continue for all fields)
}
```

**Step 2: Commit**

```bash
git add src/search.rs
git commit -m "docs(search): add comprehensive rustdoc to SearchConfig"
```

---

## Phase 6: Medium Priority - Code Deduplication

### Task 6.1: Unify streaming and batch generation

**Files:**
- Modify: `src/gen.rs`

**Step 1: Create shared validation helper**

Extract common expression validation logic:

```rust
/// Check if an evaluated expression meets generation criteria
#[inline]
fn should_include_expression(
    result: &crate::eval::EvalResult,
    config: &GenConfig,
    complexity: u32,
    contains_x: bool,
) -> bool {
    result.value.is_finite()
        && result.value.abs() <= MAX_GENERATED_VALUE
        && result.num_type >= config.min_num_type
        && if contains_x {
            config.generate_lhs && complexity <= config.max_lhs_complexity
        } else {
            config.generate_rhs && complexity <= config.max_rhs_complexity
        }
}
```

**Step 2: Create shared symbol iteration helper**

```rust
/// Iterate over valid symbols for the current expression state
fn iter_valid_symbols<'a>(
    config: &'a GenConfig,
    current: &Expression,
    stack_depth: usize,
    max_complexity: u32,
) -> impl Iterator<Item = (Symbol, u32)> + 'a {
    config
        .constants
        .iter()
        .copied()
        .chain(config.unary_ops.iter().copied())
        .chain(config.binary_ops.iter().copied())
        .filter(move |&sym| {
            !exceeds_symbol_limit(config, current, sym)
                && !would_exceed_complexity(config, current, sym, max_complexity)
        })
        .map(move |sym| (sym, config.symbol_table.weight(sym)))
}
```

**Step 3: Refactor both generation functions to use shared helpers**

Update `generate_recursive` and `generate_recursive_streaming` to use the new helpers.

**Step 4: Run tests to verify no regression**

```bash
cargo test
```

**Step 5: Commit**

```bash
git add src/gen.rs
git commit -m "refactor(gen): unify validation logic between streaming and batch generation"
```

---

## Phase 7: Low Priority - Examples Directory

### Task 7.1: Create examples directory with API demos

**Files:**
- Create: `examples/basic_search.rs`
- Create: `examples/custom_config.rs`
- Create: `examples/streaming.rs`

**Step 1: Create basic_search.rs example**

```rust
//! Basic search example
//!
//! Run with: cargo run --example basic_search

use ries_rs::{search, GenConfig};

fn main() {
    let target = std::env::args().nth(1).unwrap_or_else(|| "2.5".to_string());
    let target: f64 = target.parse().expect("Please provide a valid number");

    println!("Searching for equations where x = {}", target);
    println!("{:-<60}", "");

    let config = GenConfig::default();
    let matches = search(target, &config, 20);

    for (i, m) in matches.iter().enumerate() {
        println!(
            "{:3}: {} = {}  (distance: {:.2e}, complexity: {})",
            i + 1,
            m.lhs.expr.format(&ries_rs::OutputFormat::Pretty),
            m.rhs.expr.format(&ries_rs::OutputFormat::Pretty),
            m.distance,
            m.total_complexity()
        );
    }

    println!("{:-<60}", "");
    println!("Found {} matches", matches.len());
}
```

**Step 2: Create custom_config.rs example**

```rust
//! Custom configuration example
//!
//! Demonstrates how to customize search parameters.
//! Run with: cargo run --example custom_config

use ries_rs::{search, gen::GenConfig, search::SearchConfig, symbol::NumType};

fn main() {
    let target = 3.14159265358979;

    // Configuration for algebraic-only search
    let gen_config = GenConfig {
        max_lhs_complexity: 40,
        max_rhs_complexity: 25,
        min_num_type: NumType::Algebraic,
        ..GenConfig::default()
    };

    let search_config = SearchConfig {
        tolerance: 1e-10,
        max_matches: 10,
        ..SearchConfig::default()
    };

    println!("Searching for algebraic equations where x = π");
    println!("{:-<60}", "");

    let matches = search(target, &gen_config, 10);

    for m in &matches {
        println!(
            "{} = {}",
            m.lhs.expr.format(&ries_rs::OutputFormat::Pretty),
            m.rhs.expr.format(&ries_rs::OutputFormat::Pretty)
        );
    }
}
```

**Step 3: Commit**

```bash
git add examples/
git commit -m "docs: add examples directory with API usage demos"
```

---

## Phase 8: Low Priority - Security Audit

### Task 8.1: Add cargo audit to CI

**Files:**
- Modify: `.github/workflows/ci.yml`

**Step 1: Add security audit job**

Add to `.github/workflows/ci.yml`:

```yaml
  audit:
    name: Security Audit
    runs-on: ubuntu-latest
    steps:
      - name: Checkout
        uses: actions/checkout@v4

      - name: Setup Rust
        uses: dtolnay/rust-toolchain@stable

      - name: Install cargo-audit
        uses: taiki-e/install-action@cargo-audit

      - name: Run security audit
        run: cargo audit
```

**Step 2: Commit**

```bash
git add .github/workflows/ci.yml
git commit -m "ci: add cargo audit security check"
```

---

## Summary

| Phase | Priority | Estimated Tasks | Key Deliverable |
|-------|----------|-----------------|-----------------|
| 1 | High | 1 | CONTRIBUTING.md |
| 2 | High | 1 | Coverage threshold enforcement |
| 3 | High | 3 | main.rs refactored to ~500 lines |
| 4 | Medium | 1 | WASM tests passing in CI |
| 5 | Medium | 2 | Config types fully documented |
| 6 | Medium | 1 | Reduced code duplication in gen.rs |
| 7 | Low | 1 | Examples directory |
| 8 | Low | 1 | Security audit in CI |

**Total: 11 tasks across 8 phases**
