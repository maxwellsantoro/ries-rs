# AGENTS.md

`ries-rs` searches for compact equations near a supplied numerical target. One
Rust engine powers the CLI (`ries-rs`, crate `ries`), Rust library, Python
(`ries_rs`), and WASM/browser interfaces. Read only the references needed for the
task; start with the relevant packet in [docs/ROADMAP.md](docs/ROADMAP.md).

## Authority and orientation

- Runtime: `src/`; regression contracts: `tests/`. When docs disagree with them,
  inspect the discrepancy and correct the docs rather than assuming new behavior.
- Current behavior: [docs/SEARCH_MODEL.md](docs/SEARCH_MODEL.md); code map:
  [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md); other docs:
  [docs/README.md](docs/README.md).
- Planned work and dependencies: [docs/ROADMAP.md](docs/ROADMAP.md). Planned APIs
  and acceptance criteria are not implemented contracts.
- Check selection and reuse: [docs/VALIDATION.md](docs/VALIDATION.md). Release
  requirements: [RELEASING.md](RELEASING.md).
- `docs/archive/`, versioned release notes, and benchmark artifacts are historical
  evidence. Preserve them; do not load them as current instructions. `CLAUDE.md`
  is a symlink to this file, not a second policy to maintain.

## Coordinator and workers

For roadmap implementation, use a coordinator and independent workers with the
exclusive file ownership and dependency order in the roadmap. Use the project
[ries-delivery skill](.agents/skills/ries-delivery/SKILL.md) for packet dispatch,
handoff, and integration. Small single-file tasks do not need a team.

The coordinator records the starting dirty tree, preserves prior work, freezes
shared interfaces before adapters, and assigns each shared file one owner.
Workers do not revert another worker's changes or edit outside their ownership
without coordinating. Report focused test evidence and unresolved limitations;
the coordinator owns the integrated checks. Reuse existing workers and completed
exploration rather than asking multiple agents to inspect the same subsystem.

## Validation loop

Use the change matrix in [docs/VALIDATION.md](docs/VALIDATION.md), not a blanket
full-suite run. Start with the regression that demonstrates the change; run the
affected integration checks once after the patch settles. Documentation-only
changes need cheap doc/integrity checks, not an engine rebuild.

Reuse passing evidence only when all relevant source, test, dependency, feature,
toolchain, environment, and artifact inputs are unchanged. Include untracked
inputs in the receipt. A Cargo cache is not a test receipt. Do not run concurrent
Cargo commands against the same target directory or each agent's full matrix.
Release candidates still require clean exact-commit CI and fresh installed
artifacts. Failures, cancellations, timeouts, and unavailable checks are explicit.

## Project constraints

- Determinism is a public contract: raw ordering lives in `src/pool.rs`, weights
  in `src/symbol.rs` / `src/symbol_table.rs`. Prefer explicit profiles and
  `--deterministic` with manifests for reproducible probes.
- Preserve the original decimal literal. A numerical “exact” match is not a
  symbolic proof; high-precision reevaluation cannot recover digits lost to f64
  parsing. Keep root existence, input agreement, and independent evidence distinct.
- New runtime behavior ships with a meaningful regression. Public changes update
  the matching reference docs; avoid copying the same contract into every guide.
- Local Python tooling uses `uv` and `maturin`; use the maintained test helpers.
  Public pip installation instructions are fine. `highprec` needs GMP/MPFR;
  browser WASM build scripts need Node and Rust nightly with rust-src/wasm32.
- Keep shipped PSLQ support scoped. Do not add operators, expand proof claims,
  change versions, or rewrite release metadata as incidental cleanup.

## Portfolio metadata

The registry entry is `RIES` in
`~/projects/registry/project_registry.json` (family `formal-math`). Update project
metadata there when it changes; do not substitute repo prose for registry updates.
Read that repository's guidance before modifying it.
