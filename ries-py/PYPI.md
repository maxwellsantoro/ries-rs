# ries-rs

Python bindings for [`ries`](https://crates.io/crates/ries), a Rust inverse
equation solver that turns a target number into compact algebraic equations.

Install the package from PyPI as `ries-rs`, then import it in Python as
`ries_rs`.

The Python package uses the same core search engine as the CLI and WASM builds,
including the same presets and typed match objects. For deterministic mode,
JSON manifests, or the full compatibility-oriented CLI surface, use the main
`ries-rs` executable.

## Install

```bash
pip install ries-rs
```

## Quick Start

```python
import ries_rs

results = ries_rs.search(1.6487212707, level=5, max_matches=3)
for match in results:
    print(f"{match.lhs} = {match.rhs}")
```

```text
x^2 = e
x = sqrt(e)
ln(x) = 1/2
```

## More Examples

Use a preset to expose domain-specific constants:

```python
import ries_rs

results = ries_rs.search(1.64493406685, preset="analytic-nt", level=2, max_matches=5)
for match in results[:5]:
    print(match.solve_for_x or f"{match.lhs} = {match.rhs}")
```

Export structured results for notebooks or downstream tooling:

```python
import json
import ries_rs

payload = [match.to_dict() for match in ries_rs.search(1.618033988749895, max_matches=3)]
print(json.dumps(payload, indent=2))
```

## What You Get

- `ries_rs.search(...)` for equation search
- `ries_rs.list_presets()` for available domain presets
- `ries_rs.version()` for runtime version checks
- `PyMatch` objects with `to_dict()` for serialization

## Project Links

- Repository: <https://github.com/maxwellsantoro/ries-rs>
- Main README: <https://github.com/maxwellsantoro/ries-rs/blob/main/README.md>
- Python bindings docs: <https://github.com/maxwellsantoro/ries-rs/blob/main/docs/PYTHON_BINDINGS.md>
- Live demo: <https://maxwellsantoro.com/projects/ries-rs/app/>
