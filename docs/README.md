# Documentation Map

This repository keeps current reference documentation separate from historical
implementation notes.

## Current Reference

- `../README.md`: project overview, install paths, and primary usage examples
- `ARCHITECTURE.md`: codebase layout, runtime surfaces, and search pipeline
- `SEARCH_MODEL.md`: formal search behavior, ranking, and determinism contract
- `COMPLEXITY.md`: symbol weights and complexity rationale
- `PERFORMANCE.md`: benchmarking policy, artifact sources, and profiling workflow
- `PARITY_STATUS.md`: current compatibility summary relative to historical RIES baselines
- `PYTHON_BINDINGS.md`: Python package install, API surface, and troubleshooting
- `WASM_BINDINGS.md`: JS/WASM package API, build targets, and deployment notes
- `../web/README.md`: browser UI, static-site bundle, and Playwright smoke flow
- `../tests/README.md`: test suite layout and test commands
- `../RELEASING.md`: maintainer release checklist and artifact verification
- `benchmarks/`: reproducible benchmark reports and raw benchmark artifacts
- `releases/`: versioned release notes used for GitHub releases

## Historical Material

- `archive/plans/`: selected archived design and implementation plans with ongoing technical value
- `archive/parity/`: archived parity gap reports and compatibility investigations
- `archive/artifacts/`: archived generated/debug artifacts

Archived material is useful for project history, not for current behavior.

## Source-of-Truth Rule

For runtime behavior, trust these in order:

1. `src/`
2. `tests/`
3. current public docs (`README.md`, files listed above)

For published install and release surfaces, trust these in order:

1. crates.io for the Rust package (`ries`)
2. PyPI for the Python package (`ries-rs`)
3. GitHub Releases for native binaries and WASM artifacts
