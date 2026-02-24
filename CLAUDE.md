# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## About This Project

RIES-RS is a Rust implementation of Robert Munafo's RIES (RILYBOT Inverse Equation Solver). Given a numeric target, it searches for algebraic equations whose solution is that target (e.g., given π, find equations like `x = π`, `x-3 = 1/7`).

## Commands

```bash
# Build
cargo build --release

# Run
./target/release/ries-rs 3.141592653589793

# Test all
cargo test

# Run a specific test file
cargo test --test integration_tests
cargo test --test property_tests
cargo test --test cli_regression_tests

# Run a single test by name
cargo test --test integration_tests test_name

# Lint (CI requires zero warnings)
cargo clippy --all-targets -- -D warnings

# Format
cargo fmt

# Benchmarks
cargo bench

# Python bindings — MUST use maturin, not cargo build (from ries-py/)
cd ries-py && maturin develop --release
cargo check --manifest-path ries-py/Cargo.toml  # type-check only

# WASM (requires nightly; uses -Z build-std to avoid reference-types vs wasm-bindgen CLI mismatch)
npm run build
# Browser demo: build then serve repo root, open /web/
# Threaded WASM (nightly, SharedArrayBuffer, COOP/COEP required):
npm run build:threads
```

## Architecture

### Core Pipeline

1. **Generation** (`gen.rs`): Enumerates all valid postfix expressions up to a complexity limit. Produces two sets: **LHS** (expressions containing variable `x`) and **RHS** (constant expressions). Supports both batch (`generate_all`) and streaming (`generate_streaming`) modes.

2. **Evaluation** (`eval.rs`): Evaluates postfix expressions using a stack machine with **forward-mode automatic differentiation** — each value on the stack carries both its value and its derivative w.r.t. `x`. The `EvalWorkspace` struct is reused across calls to avoid heap allocations in hot loops.

3. **Matching/Search** (`search.rs`): For each LHS-RHS pair, uses **Newton-Raphson** to solve `LHS(x) = RHS`. Results are collected in a bounded priority pool (`pool.rs`). Supports parallel search via Rayon.

4. **Ranking** (`pool.rs`, `metrics.rs`): Matches are ranked by error, then either parity-style (matching the original RIES behavior) or complexity-style (simpler expressions first).

### Key Types

- **`Symbol`** (`symbol.rs`): Single-byte enum (values are ASCII chars like `p`=π, `e`=e, `+`, `-`, `q`=sqrt). Each symbol has a `Seft` (stack effect type): `A`=constant/push-1, `B`=unary pop-1-push-1, `C`=binary pop-2-push-1. Also tracks `NumType` (Integer → Rational → Algebraic → ... → Transcendental).

- **`Expression`** (`expr.rs`): `SmallVec` of `Symbol`s in postfix order. Max length 21 (matching original C RIES). Carries a precomputed `NumType` and complexity score.

- **`GenConfig`** (`gen.rs`): Controls which symbols are available, complexity limits, and generation options. This is the primary configuration object threaded through search.

- **`SearchConfig`** (`search.rs`): Controls Newton convergence tolerance, match distance thresholds, ranking mode, and early exit criteria.

### Module Map

```
src/
├── lib.rs          — Public API re-exports; three API levels (high/mid/low)
├── main.rs         — CLI binary entry
├── cli/
│   ├── args.rs     — clap argument definitions
│   ├── config_builder.rs — Builds GenConfig/SearchConfig from CLI args
│   ├── legacy.rs   — Normalizes legacy -p/-l/-i/-S/-E flags
│   ├── output.rs   — Display formatting (default/pretty/Mathematica/SymPy)
│   ├── diagnostics.rs — -D flag handling
│   └── search_runner.rs — Orchestrates the full search+output flow
├── symbol.rs       — Symbol enum, Seft, NumType (authoritative weight source)
├── expr.rs         — Expression type, OutputFormat
├── eval.rs         — Stack evaluator with automatic differentiation
├── gen.rs          — Expression enumeration (batch + streaming)
├── search.rs       — Newton-Raphson matching, SearchStats, Match
├── pool.rs         — Bounded TopKPool with parity/complexity ranking modes
├── metrics.rs      — Match scoring and categorization
├── fast_match.rs   — Fast path: exact matches against known constants
├── symbol_table.rs — Runtime symbol table (supports user-defined constants)
├── profile.rs      — Profile file loading (~/.ries, --profile flag)
├── udf.rs          — User-defined functions (--define flag)
├── presets.rs      — Named domain presets (physics, number-theory, etc.)
├── thresholds.rs   — All numeric constants (tolerances, scale factors)
├── stability.rs    — Numerical stability helpers
├── report.rs       — Report mode: categorized match output
├── manifest.rs     — Reproducibility manifest (--emit-manifest)
├── pslq.rs         — PSLQ integer relation detection
├── highprec_verify.rs — High-precision verification (highprec feature)
├── precision.rs    — HighPrec type abstraction (highprec feature only)
└── wasm.rs         — WASM bindings (wasm feature only)
```

## Feature Flags

| Flag | Default | Description |
|------|---------|-------------|
| `parallel` | ✓ | Multi-threaded via Rayon |
| `highprec` | ✗ | Arbitrary precision (requires GMP/MPFR) |
| `wasm` | ✗ | wasm-bindgen bindings |
| `wasm-threads` | ✗ | WASM + Rayon via wasm-bindgen-rayon (nightly, atomics, `initThreadPool`) |

Python bindings live in the separate `ries-py/` crate (PyO3 + maturin).

## Parity Tracking

This project tracks compatibility with two references: Robert Munafo's original `ries` and the `clsn/ries` fork. See `docs/PARITY_STATUS.md` for the current status summary and `docs/archive/parity/2026-02-18-parity-remaining-report.md` for historical detail. In `--classic` mode, default ranking is parity-style; use `--complexity-ranking` to switch.

## Commit Convention

Follow conventional commits: `feat:`, `fix:`, `docs:`, `refactor:`, `chore:`.
