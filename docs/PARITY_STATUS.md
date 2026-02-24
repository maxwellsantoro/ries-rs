# Parity Status

Current parity/compatibility status for `ries-rs` relative to the original RIES and the `clsn/ries` fork.

## Current State (2026-02-24)

- No known mandatory parity blockers remain from the tracked P0/P1/P2 parity work.
- `--classic` mode defaults to parity-style ranking; `--complexity-ranking` remains available.
- Remaining work is optional quality enhancement (for example, extending `-s` solve-for-x support to more operator families).

## References

- Historical detailed gap report (2026-02-18): `docs/archive/parity/2026-02-18-parity-remaining-report.md`
- Comparison helper script: `tests/compare_with_original.sh`
- Regression coverage: `tests/cli_regression_tests.rs` and `tests/cli/`

## Source of Truth

For current behavior, prefer code and tests over historical parity reports:

1. `src/`
2. `tests/`
3. `README.md`
