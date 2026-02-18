# Test Organization

## Integration Test Layout

- `cli_regression_tests.rs`: CLI test harness and shared helpers
- `cli/`: CLI regression test modules split by topic
- `search_tests.rs`, `evaluation_tests.rs`, `expression_tests.rs`, `profile_tests.rs`: subsystem integration tests
- `property_tests.rs`: property-based tests

## Comparison Tooling

- `compare_with_original.sh`: side-by-side output comparison against original `ries`
- Set `RIES_ORIGINAL_BIN` or pass original binary path as argument 4

## Running

```bash
cargo test
```
