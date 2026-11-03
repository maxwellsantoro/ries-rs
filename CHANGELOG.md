# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [2.0.3] - 2026-11-03

### Fixed
- Solved equations now use the real inverse branch nearest the reported root.
  Sine, cosine, tangent, and even roots no longer print the principal value
  when the search found a different real branch. Odd powers still use their
  single real root.
- Large period shifts work. Any finite integer, including magnitudes past
  `2^63`, is a valid shift. If that shift cannot be represented, the solved
  form is omitted.
- `sinpi`, `cospi`, and `tanpi` use the correct parity at large magnitudes.
  Finite values at `2^52` and above are integers, and finite values at `2^53`
  and above are even, so `tanpi` of `2^62` is 0 rather than a pole.
- An exact match now requires both sides to agree at the typed target, with
  the solved `x` within a few ulps of that target. A nearby root is no longer
  labeled exact.
- Flushed `x` terms are skipped at any magnitude. When the `x` contribution
  has disappeared into a constant, and a probe shows the expression is not a
  true identity, that candidate is left out.
- Newton keeps a refined root only when the residual is small. A tiny step
  that leaves the two sides apart is rejected. Steep roots with a small
  relative residual are still kept.

### Changed
- Coverage now runs on LLVM. The coverage job no longer uses tarpaulin's
  ptrace engine, which crashed while dropping an expression after a passing
  manifest test.

## [2.0.2] - 2026-06-27

### Fixed
- Turbo now mirrors serial generation semantics before parallel matching:
  small batch searches use the same LHS deduplication as serial, while
  streaming-sized searches preserve the raw LHS stream. This fixes a
  level-3 rank-one parity miss where `--turbo` could discard the serial best
  match for `2.506314` before matching.
- Added regressions covering both turbo's small batch-generation parity path
  and the expensive level-3 flat-exact `2.506314` case.

### Changed
- Documented the refined turbo contract: canonical generation semantics plus
  parallel match/Newton work, with the lower-ranked tail still allowed to differ
  from serial.

## [2.0.1] - 2026-06-27

### Fixed
- Turbo and parallel-generation searches now fall back to the canonical serial
  pipeline for order-dependent `--stop-at-exact` and `--stop-below` searches,
  preventing per-worker early exits from violating rank-one parity
- Turbo CLI results are canonically sorted before display so the documented
  best match is always presented at rank one

## [2.0.0] - 2026-06-26

### Added
- Turbo mode (`--turbo`) parallelizes matching and Newton refinement across
  available cores while preserving the same single best match as serial search
- Shared `MatchSummary` conversion for consistent Rust, Python, and WASM match
  representations
- Widest candidate-window diagnostics in JSON output and search stats, recording the
  LHS expression that produced the maximum RHS scan window
- Level-3 benchmark baseline artifacts dated 2026-03-20
- Supported Rust-side and end-to-end Python binding test workflows using `uv`

### Changed
- **Breaking:** `SearchStats::record_candidate_window` now takes the LHS
  `EvaluatedExpr` plus window width (was width only) so widest-window diagnostics
  can be attributed to a specific expression
- Python wheels use the CPython stable ABI for Python 3.8 and newer instead of
  being tied to the single interpreter version used to build them
- Adaptive search radius is capped by the pool strict-gate coarse-error envelope,
  matching `would_accept_strict` value-space bounds; batch and streaming paths now
  share the same radius logic
- Parallel generation OOM preflight uses count-only streaming instead of cloning
  full expression sets, reducing peak RSS during large runs
- `sinpi`/`cospi`/`tanpi` at default π scale snap arguments within `1e-12` of
  integers or half-integers to exact values and derivatives, removing pathological
  near-singular artifacts (e.g. `1Sxn^S`) but also shifting which equations rank
  highly; refreshed level-3 baselines show fewer Newton calls and a lower Newton
  success rate (~92% → ~73%) alongside far fewer candidate pairs scanned
  (~66M → ~8M)
- Optional widest-window JSON fields are omitted when no window has been recorded
  (`#[serde(skip_serializing_if = "Option::is_none")]`)
- Python and WASM APIs reject non-finite targets consistently and document their
  bounded level scale separately from the CLI complexity scale

### Fixed
- PSLQ relation detection, duplicate/scalar relation handling, and rational
  fast-match classification
- Adaptive target-count exponent overflow and profile parsing/merge error handling
- Adaptive-growth behavior and classic streaming result ordering
- Expression deduplication during generation no longer clones when replacing a
  duplicate with a simpler expression in the same quantized bucket
- Trig argument snapping guards `f64`→`i64` conversion against out-of-range values

## [1.1.1] - 2026-03-18

### Added
- A direct live-demo link in the README and on the GitHub repository homepage

### Changed
- Refreshed GitHub Actions workflow dependencies to current Node 24-compatible
  releases to remove deprecation warnings from CI and release automation
- Updated public-facing README copy to make the landing page and standalone demo
  entrypoints explicit

## [1.1.0] - 2026-03-18

### Added
- Automated release-integrity checks for cross-package version sync, canonical
  homepage metadata, release notes presence, and README image assets
- Public-facing v1.1.0 release notes centered on the canonical landing page and
  live demo handoff

### Changed
- Pointed package and citation homepage metadata at the canonical project page
  on `maxwellsantoro.com`
- Clarified release ownership in `RELEASING.md`: this repo verifies release
  artifacts, while the website repo verifies the deployed demo experience
- Kept the crate-level rustdoc command-line example contiguous by moving the
  test-only crate attribute out of the middle of the docs

### Fixed
- Aligned Zenodo/DOI wording across the README, release notes, and release
  process docs so the repo no longer overclaims archival status
- Added an automated guard for the README screenshot asset referenced at the top
  of the project page

## [1.0.1] - 2026-03-16

### Changed
- Automated the tag-driven release workflow to publish to crates.io and PyPI
  before creating the GitHub release
- Added Python source distribution (`sdist`) packaging to the release pipeline
- Made release publish steps rerun-safe when a tagged version is already present
  on crates.io or PyPI

### Fixed
- Resolved `maturin sdist` packaging for `ries-py` by using a package-local
  PyPI readme and explicit Python package metadata

## [1.0.0] - 2026-03-14

### Added
- PSLQ integer relation detection (`--pslq`, `--pslq-extended`, `--pslq-max-coeff`)
- WebAssembly bindings for browser and Node.js usage
- Python bindings with PyO3/maturin
- High-precision verification using arbitrary precision arithmetic
- Stability ladder for impostor detection
- Domain presets for targeted searches (analytic-nt, physics, etc.)
- Run manifest for reproducibility (`--emit-manifest`)
- Deterministic mode for reproducible results (`--deterministic`)
- CITATION.cff for academic citation

## [0.1.0] - 2026-02-18

### Added
- Initial release
- Core equation search with parallel execution
- Automatic differentiation for Newton-Raphson refinement
- User-defined constants and functions via profiles
- Multiple output formats (default, pretty, Mathematica, SymPy)
- Complexity scoring for equation ranking
- Classic mode for compatibility with original RIES
- CLI with comprehensive options

### References
- Based on [RIES by Robert Munafo](https://mrob.com/pub/ries/)
- Compatible with [clsn/ries fork](https://github.com/clsn/ries)
