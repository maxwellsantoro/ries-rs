# RIES-RS Code Quality Improvements Design

**Date:** 2026-02-16
**Status:** Approved
**Approach:** Incremental Polish

## Overview

Address code quality issues and implement high-precision support for ries-rs, following the incremental polish approach to deliver value at each step.

## Goals

1. Eliminate all compiler warnings
2. Improve code organization and maintainability
3. Add comprehensive documentation
4. Enable CI/CD for quality assurance
5. Expand benchmark coverage
6. Lay groundwork for high-precision mode

## Design Sections

### Section 1: Dead Code Warnings & Rustdocs

**Problem:** Several public API functions show dead code warnings because they're intended for library consumers.

**Solution:**
- Add `#[allow(dead_code)]` to public API functions not used internally
- Add comprehensive rustdoc documentation to all public items
- Enable rustdoc generation in Cargo.toml

**Files Changed:**
- `src/eval.rs` - Add `#[allow(dead_code)]` + docs
- `src/search.rs` - Add `#[allow(dead_code)]` + docs
- `src/expr.rs` - Add `#[allow(dead_code)]` + docs
- `src/lib.rs` - Expand crate-level docs
- `Cargo.toml` - Add documentation metadata

### Section 2: Test Reorganization

**Problem:** `search.rs` contains ~1200 lines of tests mixed with implementation.

**Solution:**
Create `tests/` directory structure:
```
tests/
├── integration_tests.rs    # High-level search tests
├── expression_tests.rs     # Expression parsing/conversion tests
├── evaluation_tests.rs     # Eval + derivative tests
└── common/
    └── mod.rs              # Shared test utilities
```

**Guidelines:**
- Small unit tests (5-20 lines) stay in `#[cfg(test)] mod tests` within source files
- Integration tests (larger, multi-module) go to `tests/`
- Shared utilities go in `tests/common/mod.rs`

**Files Changed:**
- Create `tests/` directory with 4 files
- Remove large test blocks from `src/search.rs`
- Keep small unit tests in place

### Section 3: CI/CD with GitHub Actions

**Problem:** No automated testing, building, or linting.

**Solution:** Create `.github/workflows/ci.yml` with:

- **test job:** Run on stable and beta Rust
- **lint job:** Clippy with warnings as errors + format checking
- **docs job:** Verify documentation builds

**Files Changed:**
- Create `.github/workflows/ci.yml`

### Section 4: Expanded Benchmarks

**Problem:** Current benchmarks are minimal.

**Solution:** Expand `benches/evaluation.rs` with:

1. Expression evaluation benchmarks (simple, complex, transcendental)
2. Newton-Raphson benchmarks (fast/slow convergence)
3. Expression generation benchmarks (by level)
4. Full search pipeline benchmarks

**Files Changed:**
- Expand `benches/evaluation.rs`

### Section 5: High-Precision Infrastructure

**Problem:** High-precision mode not implemented.

**Solution:**
Create `RiesFloat` trait in `src/precision.rs`:

```rust
pub trait RiesFloat: Clone + PartialOrd + Debug + Send + Sync {
    fn zero() -> Self;
    fn one() -> Self;
    fn from_f64(v: f64) -> Self;
    fn to_f64(&self) -> f64;

    // Arithmetic
    fn add(&self, other: &Self) -> Self;
    fn sub(&self, other: &Self) -> Self;
    fn mul(&self, other: &Self) -> Self;
    fn div(&self, other: &Self) -> Self;
    fn neg(&self) -> Self;

    // Math functions
    fn sqrt(&self) -> Self;
    fn square(&self) -> Self;
    fn ln(&self) -> Self;
    fn exp(&self) -> Self;
    fn sin(&self) -> Self;
    fn cos(&self) -> Self;
    fn tan(&self) -> Self;
    fn pow(&self, exp: &Self) -> Self;

    // Special checks
    fn is_nan(&self) -> bool;
    fn is_infinite(&self) -> bool;
    fn abs(&self) -> Self;
}
```

**Scope Limitation (for "nice to have"):**
- Implement trait + f64 impl
- Add infrastructure for rug (full implementation can be added later)
- CLI flag `--precision` accepts value but warns "not yet implemented"

**Files Changed:**
- Create `src/precision.rs`
- Modify `src/eval.rs`, `src/search.rs` for generics
- Update `src/main.rs` with `--precision` flag
- Update `Cargo.toml`

## Implementation Order

1. Dead code warnings & rustdocs (quick wins)
2. Test reorganization
3. CI/CD setup
4. Benchmark expansion
5. High-precision infrastructure

## Success Criteria

- Zero compiler warnings
- All tests pass in new structure
- CI passes on all jobs
- Benchmarks provide useful performance data
- High-precision trait implemented with f64 backend
