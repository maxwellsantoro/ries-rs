# Documentation Map

This directory is split into current reference docs, active planning notes, and archived historical material.

## Current Reference

- `ARCHITECTURE.md`: System architecture including streaming vs batch generation trade-offs
- `COMPLEXITY.md`: Symbol weight model and ranking rationale used by `ries-rs`
- `PERFORMANCE.md`: Benchmarking, profiling, and performance guidance
- `PARITY_STATUS.md`: Current parity/compatibility status summary and links to detailed history

## Active Plans

- `plans/`: Dated design/implementation plans that are still in progress or recently active (created as needed; may be absent)

## Archive

- `archive/plans/`: Historical design/implementation plans retained for traceability
- `archive/parity/`: Historical parity gap reports and detailed compatibility writeups
- `archive/artifacts/`: Archived generated/debug artifacts (sample manifests, screenshots, one-off debug scripts)

Archived files are retained for project history and decision traceability. They are not the source of truth for current behavior.

## Source-of-Truth Rule

For runtime behavior, trust these in order:

1. CLI/runtime code in `src/`
2. Regression tests in `tests/`
3. High-level summaries (`README.md`, `PARITY_STATUS.md`)
