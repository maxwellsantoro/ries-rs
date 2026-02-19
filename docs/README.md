# Documentation Map

This directory is split into current reference docs and historical implementation plans.

## Current Reference

- `ARCHITECTURE.md`: System architecture including streaming vs batch generation trade-offs
- `COMPLEXITY.md`: Symbol weight model and ranking rationale used by `ries-rs`
- `PERFORMANCE.md`: Benchmarking, profiling, and performance guidance

## Historical Plans

- `plans/`: Dated design/implementation plans used during parity and quality work

These plan files are retained for project history and decision traceability. They are not the source of truth for current behavior.

## Source-of-Truth Rule

For runtime behavior, trust these in order:

1. CLI/runtime code in `src/`
2. Regression tests in `tests/`
3. High-level summaries (`README.md`, `PARITY_REMAINING_REPORT.md`)
