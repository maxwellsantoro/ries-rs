# Python Bindings

The Python package exposes the `ries-rs` engine through PyO3 as the module
`ries_rs`.

## Install

Published package:

```bash
pip install ries-rs
```

Published wheels use CPython's stable ABI (`abi3`) with a Python 3.9 minimum,
so one wheel per supported operating-system/architecture target works across
Python 3.9 and newer.

Local development build:

```bash
uv venv
source .venv/bin/activate
uv pip install maturin
cd ries-py
maturin develop --release --locked
```

Build distribution artifacts:

```bash
cd ries-py
maturin build --release --locked
maturin sdist --out dist
```

Use [VALIDATION.md](VALIDATION.md) to select Rust helper and installed-wheel
checks for your change. The helpers prefer `uv` and keep extension-module tests
separate from Rust-only binding tests. Published wheels use the stable ABI;
local helper scripts also handle newer interpreters through PyO3's forward
compatibility setting when needed.

## Module API

The module exports:

- `discover(...)`: precision-aware hypotheses and a serializable run record
- `search(...)`
- `list_presets()`
- `version()`
- `PyMatch`

## Quick Start

```python
import ries_rs

print(ries_rs.version())
print(ries_rs.list_presets())

run = ries_rs.discover("3.14159265", level=3, max_matches=3)
for hypothesis in run["hypotheses"]:
    print(hypothesis["representative"]["solve_for_x"])

# Existing typed raw-search API remains available.
results = ries_rs.search(1.6487212707, level=5, max_matches=3)
for match in results[:3]:
    print(match)
```

## `discover()` Parameters and Result

```python
run = ries_rs.discover(
    "1.41421356",
    level=3,
    max_matches=8,
    preset=None,
    parallel=False,
    verification_bits=None,
)
```

`target_literal` is required text so entered precision, trailing zeros, and
scientific notation survive. Passing a float raises `TypeError`; converting an
already rounded float to text cannot recover its original spelling. Optional
`level` is 0..5 (default 2), `max_matches` is 1..1000 (default 8), and `preset`
uses the names from `list_presets()`. Invalid decimal input, unavailable presets,
or unsupported option values raise `ValueError`. Unknown keywords raise
`TypeError`.

The result is a plain JSON-compatible `dict` with schema `ries-discovery-v1`:

- `target_literal`, `target`, and `engine_version` preserve the input and engine.
- `config` contains resolved precision, grammar/weights, budgets, selection
  policy, requested parallel/verification settings, and numerical controls.
- `stats` identifies the actual matcher and observed counts/times. A parallel
  request currently uses the serial stop-contract fallback.
- `hypotheses` contains a `representative` match dictionary, structural `key`,
  `supporting_equations` excluding the representative, `within_input_precision`,
  and separate numerical/high-precision/polynomial-certificate check statuses.

The shared policy finds the cheapest admitted equation resolving the entered
precision before limiting results. More costly alternatives are excluded at
that bound, so fewer hypotheses than `max_matches` can be returned. If none
resolves the precision, closest candidates are returned with
`within_input_precision: False`. Safe structural grouping preserves inverse
branches; numerical proximity alone is not equivalence. See
[SEARCH_MODEL.md](SEARCH_MODEL.md#additive-precision-aware-discovery) for the
selection and evidence contract.

`verification_bits` optionally requests higher-precision numerical corroboration
and is capped at 4096 bits. A wheel built without `highprec`, an unsupported
operator, or precision at most 53 bits reports `unavailable`; it never silently
reports verification success. With no request the status is `not_requested`.
The target and preset constants remain their parsed f64 values. Discovery does
not compute a polynomial certificate itself; the result explains the separate
restricted-grammar export/check path. None of these statuses is symbolic proof.

Both native search entry points release the GIL during computation. Export the
whole run with `json.dumps(run, allow_nan=False)`; retain source/artifact identity
externally when exact build provenance matters.

## `search()` Parameters

| Parameter | Type | Default | Notes |
|-----------|------|---------|-------|
| `target` | `float` | required | numeric target value |
| `level` | `int` | `2` | accepted range: `0..=5` |
| `max_matches` | `int` | `16` | hard-capped at `10000` |
| `preset` | `str \| None` | `None` | validated against `list_presets()` |
| `parallel` | `bool` | `True` | falls back to sequential if the extension was built without the `parallel` feature |

Notes:

- The Python API exposes `search::level_to_complexity` (`40 + 2*L` on each side),
  not the CLI's `-l/--level` mapping (`35 + 10*L`).
- Results use the complexity ranking comparator: exact matches precede inexact
  matches, inexact errors use eight-ULP buckets, then complexity and canonical
  postfix tie-breaks apply. Parity ranking is not exposed here. See
  [SEARCH_MODEL.md](SEARCH_MODEL.md) for the full ordering contract.
- The Python API does not expose the CLI's broader flag surface.
- `target` must be finite; `NaN` and `Infinity` raise `ValueError`.

The native search releases the Python GIL so other Python threads can run.

### API vs CLI level scale

The Python `level` parameter uses `search::level_to_complexity`, not the CLI
`-l/--level` mapping. The API budget is `40 + 2*L` on each side. The CLI uses
`35 + 10*L` on each side, so the two scales meet near the bottom and then the
CLI pulls ahead.

| Level | API `(max_lhs, max_rhs)` | CLI `(max_lhs, max_rhs)` |
|-------|--------------------------|--------------------------|
| 0 | (40, 40) | (35, 35) |
| 2 | (44, 44) | (55, 55) |
| 3 | (46, 46) | (65, 65) |
| 5 | (50, 50) | (85, 85) |

The raw `search()` entry point does not expose `GenConfig`, target literals,
custom profiles, run manifests, or precision-aware selection. Use `discover()`
for literal-based discovery and its resolved run record. To drive a full
CLI-equivalent reproducible run from Python, invoke the CLI with an explicit
decimal string, `--deterministic`, `--json`, and `--emit-manifest`, then parse its
JSON output. Rust callers can configure `GenConfig` directly; discovery does not
duplicate every advanced CLI option.

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
| `constants` | `list[tuple[str, float]]` | preset constant names and values in postfix slot order (character codes 128 onward) |
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

The extension's Cargo library target is `ries_rs_py`, distinct from the core
crate's `ries_rs` target. Maturin's `module-name = "ries_rs"` preserves the public
Python import and initialization symbol while avoiding shared-directory output
filename collisions.
