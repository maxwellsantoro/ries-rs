# ries-rs

Python bindings for [`ries`](https://crates.io/crates/ries), a Rust implementation of the
RIES inverse equation solver.

`ries-rs` searches for algebraic equations that have a given target value as a
solution, with support for deterministic runs, structured output, and the same
core search engine used by the CLI and WASM builds.

## Install

```bash
pip install ries-rs
```

## Example

```python
import ries_rs

results = ries_rs.search(3.141592653589793, max_matches=3)
for match in results:
    print(match.lhs, "=", match.rhs)
```

## Project Links

- Repository: <https://github.com/maxwellsantoro/ries-rs>
- Python bindings docs: <https://github.com/maxwellsantoro/ries-rs/blob/main/docs/PYTHON_BINDINGS.md>
