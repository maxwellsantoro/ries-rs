# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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
