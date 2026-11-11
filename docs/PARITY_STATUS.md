# Parity Status

`ries-rs` tracks behavior against two historical baselines:

1. Robert Munafo's original RIES
2. the `clsn/ries` fork and its compatibility-oriented CLI behavior

## Current Summary

- Historical compatibility is covered by finite checks; it is not a claim of full search, expression, or ranking equivalence.
- `--classic` defaults to parity-style ranking; `--complexity-ranking` remains
  available when you want simpler-first ordering instead.
- Core search flow, classic-style output, and most compatibility flags are
  covered by regression tests.
- Exact output ordering and some complexity details can still differ from older
  implementations because the engine is Rust-native.
- CI runs a printed root-accuracy smoke test and a separate restricted polynomial
  differential. The differential records normalized equation identities and costs
  and independently bounds roots for four known-answer cases. Full historical
  top-k ranking remains outside these checks. See [QUALIFICATION.md](QUALIFICATION.md).

## References

- Historical detailed gap report:
  `docs/archive/parity/2026-02-18-parity-remaining-report.md`
- Comparison helper script: `tests/compare_with_original.sh`
- Regression coverage: `tests/cli_regression_tests.rs` and `tests/cli/`

## Source of Truth

For current behavior, prefer:

1. `src/`
2. `tests/`
3. current public docs
