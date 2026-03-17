# Python Bindings

The Python bindings expose the `ries-rs` search engine through PyO3 and
`maturin`.

## Install

Install the published package from PyPI:

```bash
pip install ries-rs
```

## Source Development

From the repository root:

```bash
pip install maturin
cd ries-py
maturin develop --release
```

To build a wheel for distribution:

```bash
cd ries-py
maturin build --release --locked
maturin sdist --out dist
```

If you only want to verify the Rust side without building a wheel:

```bash
cargo check --manifest-path ries-py/Cargo.toml --locked
```

## Quick Start

```python
import ries_rs

results = ries_rs.search(3.1415926535)
for match in results:
    print(f"{match.lhs} = {match.rhs}  (error: {match.error:.2e})")
```

With options:

```python
import ries_rs

results = ries_rs.search(
    1.618033988,
    level=3,
    max_matches=20,
    preset="physics",
    parallel=True,
)
```

## `search()` Parameters

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `target` | float | required | Target value to search for |
| `level` | int | 2 | Search depth (0-5) |
| `max_matches` | int | 16 | Maximum matches to return |
| `preset` | str | `None` | Domain preset name |
| `parallel` | bool | `True` | Enable parallel search |

## `PyMatch` Properties

| Property | Type | Description |
|----------|------|-------------|
| `lhs` | str | Left-hand side expression (contains `x`) |
| `rhs` | str | Right-hand side expression (constants only) |
| `lhs_postfix` | str | Postfix representation of the LHS |
| `rhs_postfix` | str | Postfix representation of the RHS |
| `x_value` | float | Solved value of `x` |
| `error` | float | `x_value - target` |
| `complexity` | int | Complexity score (lower is simpler) |
| `is_exact` | bool | `True` if error < `1e-14` |

## `PyMatch` Methods

| Method | Returns | Description |
|--------|---------|-------------|
| `to_dict()` | dict | Convert the match to a Python dictionary |
| `__repr__()` | str | Developer-friendly representation |
| `__str__()` | str | Human-readable equation + error string |

## Examples

Find equations for common constants:

```python
import ries_rs

for match in ries_rs.search(3.141592653589793, level=2):
    print(match)

for match in ries_rs.search(2.718281828459045, level=3):
    print(match)
```

Use presets for targeted searches:

```python
import ries_rs

physics = ries_rs.search(137.035999, preset="physics", level=2)
number_theory = ries_rs.search(2.678938534707747, preset="number-theory")
```

Export results for downstream analysis:

```python
import json
import ries_rs

results = ries_rs.search(1.618033988749895)
payload = [match.to_dict() for match in results]
print(json.dumps(payload, indent=2))
```

## Troubleshooting

**ImportError: cannot import name `ries_rs`**

- Reinstall with `pip install --force-reinstall ries-rs`, or rebuild with
  `maturin develop --release`.
- Make sure you are using the same Python environment where `maturin` ran.

**Build fails with linking errors**

- Ubuntu/Debian: `sudo apt install python3-dev`
- macOS: `xcode-select --install`
- Windows: install Python from python.org

**Performance is slower than the CLI**

- Keep `parallel=True` unless you need deterministic single-threaded behavior.
- Reuse returned results instead of calling `search()` repeatedly in a tight loop.
