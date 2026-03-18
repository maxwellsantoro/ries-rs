# Tests

## Layout

- `cli_regression_tests.rs` and `cli/`: CLI behavior and compatibility coverage
- `integration_tests.rs`, `search_tests.rs`, `evaluation_tests.rs`, `expression_tests.rs`, `profile_tests.rs`: engine integration coverage
- `property_tests.rs`: property-based checks
- `wasm_tests.rs` and `test_expr_wasm.rs`: WASM-oriented Rust tests
- `web-smoke.spec.ts`: Playwright smoke test for the browser UI and static bundle
- `compare_with_original.sh`: optional side-by-side comparison against an original `ries` binary

## Common Commands

Fast local Rust regression pass:

```bash
cargo nextest run --tests --locked
```

Simple local fallback:

```bash
cargo test --tests --locked
```

High-precision feature coverage:

```bash
cargo nextest run --tests --features highprec --locked
```

Browser smoke path:

```bash
npm run test:web:smoke:build
```

## Original-RIES Comparison

`compare_with_original.sh` expects an original `ries` binary. Set
`RIES_ORIGINAL_BIN` or pass the original binary path as argument 4.
