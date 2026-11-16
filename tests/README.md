# Tests

## Layout

- `cli_regression_tests.rs` and `cli/`: CLI behavior and compatibility coverage
- `integration_tests.rs`, `search_tests.rs`, `evaluation_tests.rs`, `expression_tests.rs`, `profile_tests.rs`: engine integration coverage
- `property_tests.rs`: property-based checks
- `wasm_tests.rs` and `test_expr_wasm.rs`: WASM-oriented Rust tests
- `web-smoke.spec.ts`: Playwright smoke test for the browser UI and static bundle
- `compare_with_original.sh` and `validate_parity.py`: compare reported root accuracy with original RIES
- `test_ci_validation.py`: regression tests for parity and benchmark baseline validation

## Choosing checks

Use [VALIDATION.md](../docs/VALIDATION.md) for commands and the change-to-check
matrix. Start with the failing regression or affected test target. Run a full
Rust/feature/binding suite at the integration boundary when that surface changed,
rather than repeating it in each agent's loop.

- `numeric_contract.rs`: numerical acceptance, roots, and boundary regressions
- `stop_contract.rs`: independent small-grammar oracle and stopping behavior
- `tests/corpus/search-contract.json`: fixed matcher qualification inputs
- `test_*.py`: independent tooling and CI-policy tests

[QUALIFICATION.md](../docs/QUALIFICATION.md) explains what the independent
oracles and certificates establish and where they are unavailable. Product
hypothesis-quality evaluation is planned in [ROADMAP.md](../docs/ROADMAP.md);
passing the existing matcher corpus is not a discovery-quality score.

## Original-RIES Comparison

`compare_with_original.sh` expects executable original RIES and release `ries-rs`
binaries. Set `RIES_ORIGINAL_BIN` or pass the original binary as argument 4;
set `RIES_RS_BIN` to override `target/release/ries-rs`. Both must report an
equation whose root is within 1e-8 of the target. CI downloads the author's
2026-05-06 source and verifies its SHA-256 before compiling it. The upstream
URL is mutable, so a changed or unavailable source fails the job and requires
a reviewed update to the pin.

Run `python3 tests/test_ci_validation.py` to test the validation gates. To
compare benchmarks, dispatch the Benchmarks workflow with a baseline name,
the prior workflow run ID, and its `criterion-report-<sha>` artifact name.
A dispatch with all three fields empty saves a `main` baseline. Artifacts expire
after 90 days; comparisons require an available artifact containing that named
baseline.

For an artifact round trip with one quick case per suite, enable `smoke` in
both dispatches and select the `criterion-report-<sha>-smoke` artifact.
