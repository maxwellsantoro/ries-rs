# Python Bindings

The Python package exposes the `ries-rs` engine through PyO3 as the module
`ries_rs`.

## Install

Published package:

```bash
pip install ries-rs
```

Local development build:

```bash
pip install maturin
cd ries-py
maturin develop --release
```

Build distribution artifacts:

```bash
cd ries-py
maturin build --release --locked
maturin sdist --out dist
```

Rust-only verification without building a wheel:

```bash
cargo check --manifest-path ries-py/Cargo.toml --locked
```

Rust-side binding tests without building the extension-module flavor:

```bash
./scripts/test_ries_py_rust.sh
```

End-to-end Python integration tests:

```bash
./scripts/test_ries_py_python.sh -q
```

CI note:

- GitHub Actions pins Python 3.11 for the `ries-py` Rust-side test lane.
- GitHub Actions also runs an end-to-end Python integration lane via
  `./scripts/test_ries_py_python.sh -q`.
- Local machines may use newer interpreters; the helper script keeps that path
  usable by enabling PyO3 stable-ABI forward compatibility when needed.

## Module API

The module exports:

- `search(...)`
- `list_presets()`
- `version()`
- `PyMatch`

## Quick Start

```python
import ries_rs

print(ries_rs.version())
print(ries_rs.list_presets())

results = ries_rs.search(1.6487212707, level=5, max_matches=3)
for match in results[:3]:
    print(match)
```

## `search()` Parameters

| Parameter | Type | Default | Notes |
|-----------|------|---------|-------|
| `target` | `float` | required | numeric target value |
| `level` | `int` | `2` | accepted range: `0..=5` |
| `max_matches` | `int` | `16` | hard-capped at `10000` |
| `preset` | `str \| None` | `None` | validated against `list_presets()` |
| `parallel` | `bool` | `True` | falls back to sequential if the extension was built without the `parallel` feature |

Notes:

- The Python API currently exposes the lighter library-level complexity mapping,
  not the CLI's heavier `-l/--level` mapping.
- The Python API currently uses complexity-first ranking and does not expose the
  CLI's broader flag surface.

## `PyMatch` Properties

| Property | Type | Description |
|----------|------|-------------|
| `lhs` | `str` | left-hand side in infix form |
| `rhs` | `str` | right-hand side in infix form |
| `lhs_postfix` | `str` | left-hand side in postfix form |
| `rhs_postfix` | `str` | right-hand side in postfix form |
| `solve_for_x` | `str \| None` | solve-for-x rendering when the equation can be rearranged analytically |
| `solve_for_x_postfix` | `str \| None` | postfix form of `solve_for_x` |
| `canonical_key` | `str` | canonicalized equation key used for dedupe/reporting |
| `x_value` | `float` | solved numeric value for `x` |
| `error` | `float` | `x_value - target` |
| `complexity` | `int` | total complexity score |
| `operator_count` | `int` | total operator count across both sides |
| `tree_depth` | `int` | maximum tree depth across both sides |
| `is_exact` | `bool` | whether the match is within exact-match tolerance |

## `PyMatch` Methods

| Method | Returns | Description |
|--------|---------|-------------|
| `to_dict()` | `dict` | materialize a serializable Python dictionary |
| `__repr__()` | `str` | concise developer-oriented representation |
| `__str__()` | `str` | formatted equation string with error/complexity |

## Examples

Use a preset:

```python
import ries_rs

results = ries_rs.search(1.64493406685, preset="analytic-nt", level=2)
for match in results[:5]:
    print(match.solve_for_x or f"{match.lhs} = {match.rhs}")
```

Export results:

```python
import json
import ries_rs

payload = [match.to_dict() for match in ries_rs.search(1.618033988749895)]
print(json.dumps(payload, indent=2))
```

## Troubleshooting

**Import/build problems**

- Reinstall with `pip install --force-reinstall ries-rs`, or rebuild with
  `maturin develop --release`.
- Make sure the Python environment used for import matches the one used for
  `maturin`.
- On Linux you may need `python3-dev`; on macOS, the Xcode command-line tools.
- For Rust-side `ries-py` tests, use `./scripts/test_ries_py_rust.sh` instead
  of plain `cargo test --manifest-path ries-py/Cargo.toml`. The helper disables
  the `extension-module` feature and sets PyO3 interpreter environment
  variables consistently.
- If your local interpreter is newer than the PyO3 version in this repository
  supports, the helper enables `PYO3_USE_ABI3_FORWARD_COMPATIBILITY=1` so the
  Rust-only test path can still run against the stable ABI.

**Unexpected preset error**

- Call `ries_rs.list_presets()` and use one of those keys exactly.

**Performance is slower than the CLI**

- Keep `parallel=True` unless you specifically need single-threaded behavior.
- The Python API currently exposes a smaller configuration surface than the CLI,
  so use the CLI when you need deterministic mode, JSON manifests, or parity
  flags.
