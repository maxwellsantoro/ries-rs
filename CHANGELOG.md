# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- Widest candidate-window diagnostics in JSON output and search stats, recording the
  LHS expression that produced the maximum RHS scan window
- Level-3 benchmark baseline artifacts dated 2026-03-20

### Changed
- **Breaking:** `SearchStats::record_candidate_window` now takes the LHS
  `EvaluatedExpr` plus window width (was width only) so widest-window diagnostics
  can be attributed to a specific expression
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

### Fixed
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
