# Architecture

This document describes the current structure of `ries-rs` as a project and as
an engine.

## Runtime Surfaces

The repository ships the same search engine through four main surfaces:

- Rust CLI: `src/main.rs`
- Rust library: `src/lib.rs`
- Python bindings: `ries-py/`
- WebAssembly + browser UI: `src/wasm.rs`, `web/`, and `package.json`

All four surfaces ultimately feed into the same core generation, evaluation,
ranking, and reporting logic in `src/`.

## Codebase Layout

### Core engine

- `src/expr.rs`: postfix expression representation and infix rendering
- `src/eval.rs`: numerical evaluation and derivatives
- `src/gen.rs`: expression generation
- `src/search.rs`: search orchestration and match discovery
- `src/pool.rs`: bounded match collection, ranking, and ordering
- `src/solver.rs`: solve-for-x presentation helpers and canonicalization
- `src/symbol.rs` and `src/symbol_table.rs`: built-in symbols plus profile overrides
- `src/profile.rs`, `src/presets.rs`, `src/udf.rs`: user customization surfaces

### CLI layer

- `src/main.rs`: binary entry point
- `src/cli/`: argument parsing, config building, manifest/output/diagnostics

### Reporting and analysis

- `src/report.rs`: categorized report output
- `src/metrics.rs`: elegance/interestingness/stability-related metrics
- `src/stability.rs`: impostor-detection and stability analysis
- `src/manifest.rs`: reproducibility manifest generation
- `src/pslq.rs`: integer-relation mode

### Bindings and packaging

- `src/wasm.rs`: wasm-bindgen API
- `ries-py/src/lib.rs`: PyO3 API
- `web/index.html`: browser UI
- `scripts/`: helper scripts for web bundling and local profiling

## Search Pipeline

At a high level, all runtime surfaces execute the same pipeline:

1. Build an active symbol set from defaults, presets, profiles, and feature flags.
2. Generate valid postfix expressions under complexity and symbol constraints.
3. Split candidates into LHS expressions containing `x` and RHS constant-only expressions.
4. Match LHS and RHS numerically and refine candidates with Newton iteration when enabled.
5. Deduplicate and rank matches.
6. Format results for the caller: CLI text, JSON, Python objects, or WASM objects.

## Generation Modes

The engine supports both batch and streaming generation in `src/gen.rs`.

| Mode | Strengths | Tradeoffs | Typical use |
|------|-----------|-----------|-------------|
| Batch | simple API, built-in dedupe, easy post-processing | higher memory use | default CLI/library search paths |
| Streaming | lower peak memory, early exit possible | caller-managed handling/dedupe | high-complexity or memory-sensitive flows |

The rest of the engine is written so either mode can feed the same search and
ranking logic.

## Layering Rules

The repository is intentionally layered:

- `src/` owns runtime behavior.
- `tests/` owns regression coverage and compatibility expectations.
- `README.md`, `docs/`, and binding-specific docs explain the public surfaces.
- `docs/archive/` retains historical implementation notes but is not a source of
  truth for current behavior.

When docs and code diverge, code and tests win.

## Determinism and Ranking

Ranking and ordering are centralized rather than duplicated across frontends:

- canonical match ordering lives in `src/pool.rs`
- complexity weights live in `src/symbol.rs` and `src/symbol_table.rs`
- deterministic sorting is part of the search contract, not a frontend concern

That is why CLI, Python, and WASM all expose the same underlying match concepts
even when they differ in presentation or supported options.

## Build and Release Artifacts

Release outputs are split by surface:

- crates.io package for the Rust crate/binary
- PyPI package from `ries-py/`
- GitHub release archives for native binaries
- GitHub release WASM tarball containing `pkg`, `pkg-node`, and `pkg-bundler`
- optional static-site bundle built locally into `dist/web-site/`

See `RELEASING.md` for the maintainer checklist and `web/README.md` for the
browser bundle flow.

## Related Documentation

- `SEARCH_MODEL.md`: formal search semantics
- `COMPLEXITY.md`: weight model and simplicity scoring
- `PERFORMANCE.md`: benchmark policy and profiling workflow
- `PYTHON_BINDINGS.md`: Python-facing API details
- `WASM_BINDINGS.md`: JS/WASM-facing API details
