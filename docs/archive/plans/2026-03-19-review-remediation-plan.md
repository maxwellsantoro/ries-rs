# 2026-03-19 Review Remediation Plan

## Scope

This plan consolidates the issues raised in:

- `reviews/c.md`
- `reviews/cl.md`
- `reviews/g.md`
- `reviews/ge.md`

The review set contains a mix of confirmed correctness bugs, contract drift,
cross-surface duplication, and lower-priority performance or UX suggestions.
This document separates the immediate implementation lane from follow-up work.

## Confirmed Immediate Work

### 1. User-defined symbol filtering correctness

Problem:

- CLI symbol filtering applies allow/exclude sets to built-in symbols first.
- User constants and user functions are then appended afterward.
- This breaks the expected contract for restrictive filters such as
  `--only-symbols` and can also skew RHS-only filtering.

Resolution:

- Build the full symbol universe first, including user-defined constants and
  functions.
- Apply allow/exclude filtering once to that full set.
- Reuse the same filtered symbol pools for RHS-specific overrides.

Verification:

- Add unit tests for `--only-symbols` with user constants.
- Add unit tests for `--only-symbols` with user functions.
- Add unit tests for RHS-only allowlists with user symbols.
- Add combination tests for `--only-symbols`, `--exclude`, and `--enable`.

### 2. User symbol capacity must hard-error at 17+

Problem:

- The runtime only has fixed slots for 16 user constants and 16 user functions.
- Several code paths silently truncate after 16 entries.

Resolution:

- Enforce the 16-symbol cap centrally.
- Reject overflow during profile parsing and CLI function parsing.
- Ensure config builders for CLI, Python, and WASM all surface the same error.

Verification:

- Add tests for 16 accepted constants/functions.
- Add tests for 17 rejected constants/functions.
- Ensure the error message is stable and explicit.

### 3. Batch and streaming search paths must use the same candidate logic

Problem:

- Streaming search uses `calculate_adaptive_search_radius`.
- Batch search still uses a simpler threshold-based search radius.
- Both paths compute a linearized `x_delta`, but Newton still starts from the
  raw target rather than `target + x_delta`.

Resolution:

- Route batch search through the same adaptive radius function already used by
  streaming.
- Use `target + x_delta` as the Newton starting point in both paths.
- Keep candidate filtering and refinement semantics aligned between paths.

Verification:

- Add regression tests around shared radius behavior.
- Add regression tests for Newton seeding using cases that are sensitive to
  the initial guess.
- Add parity tests comparing batch and streaming results around the
  in-memory/streaming threshold.

### 4. Manifest/runtime/schema contract alignment

Problem:

- The schema requires numeric `stability`.
- The Rust type models it as `Option<f64>`.

Resolution:

- Keep the current `v1` schema contract and make runtime types match it.
- Add schema validation coverage for serialized manifests.

Verification:

- Add a manifest serialization test.
- Validate the produced JSON against `schema/run-manifest-v1.json`.

### 5. Level semantics and public docs cleanup

Problem:

- CLI and programmatic APIs intentionally use different level mappings.
- Some public docs still use old absolute equation-count claims that imply a
  shared meaning.

Resolution:

- Remove or rewrite the absolute equation-count claims.
- Keep the CLI/API distinction explicit in docs.

Verification:

- Update CLI help text and programmatic surface docs to align with the actual
  formulas.

## Immediate Refactoring Work

### 6. Shared `GenConfig` construction from profile

Problem:

- Python and WASM duplicate nearly identical `build_gen_config` logic.
- CLI has a third version with extra filtering.

Resolution:

- Add a shared core helper that builds a baseline `GenConfig` from a `Profile`.
- Have Python and WASM call the shared helper directly.
- Have CLI start from the shared helper and then apply CLI-specific filters and
  overrides.

Verification:

- Keep existing unit coverage for default config behavior.
- Add one cross-surface sanity test for presets flowing through user constants.

## Follow-up Work After Immediate Fixes

### 7. Instrument search bottlenecks before retuning heuristics

- Add counters for candidate window width, candidates tested per accepted
  match, Newton success rate, and pool acceptance ratio.
- Only revisit `accept_error` relaxation, tiered batch storage, or broader
  search heuristics once measurement exists.

Status:

- Initial instrumentation is now implemented in `SearchStats` and exposed
  through both CLI text stats and JSON output.
- Current counters cover candidate window total/max width, strict pre-Newton
  gate rejections, candidates per pool insertion, Newton success rate, and
  pool acceptance rate.
- Reproducible artifact capture now exists via
  `scripts/capture_search_benchmark.py`, which writes environment metadata,
  raw JSON outputs, and a generated Markdown summary for benchmark notes.
- Remaining work is analysis of those captured runs and any heuristic retuning
  that the new measurements justify.

### 8. Quantization follow-up

- Keep current quantization for now.
- Add collision-focused property tests near the 1e-8 resolution boundary.
- Remove duplicate `QUANTIZE_SCALE` definitions so there is one source of
  truth.

### 9. Browser responsiveness

- Move WASM search execution off the main UI thread.
- Preserve current result rendering and status messaging.

### 10. Tooling/ergonomics cleanup

- Make `scripts/profile_comparison.sh` portable across BSD and GNU `time`.
- Revisit `SearchStats::print()` if library output routing becomes important.
- Revisit `cdylib` build ergonomics if contributor friction remains high.

## Explicitly Deferred

The following do not block the immediate remediation lane:

- Determinism/parallel concerns in the CLI, because deterministic mode already
  disables parallel execution.
- A full PSLQ rewrite on arbitrary precision.
- Python streaming iterator support.
- Formatter deduplication in `expr.rs`.
- Macro cleanup for `Symbol` boilerplate.
- TLS-to-explicit-workspace refactors in evaluation hot paths.

## Implementation Order

1. Write the remediation plan into the repository.
2. Fix symbol filtering and user symbol capacity enforcement.
3. Extract shared `GenConfig` construction and update CLI/Python/WASM.
4. Align batch/streaming search radius and Newton seeding.
5. Align manifest runtime types with schema and clean up public docs.
6. Apply lower-risk UX/tooling fixes in parallel.
7. Run targeted tests and summarize remaining gaps.
