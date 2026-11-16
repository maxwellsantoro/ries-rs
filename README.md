# ries-rs

Turn a mysterious decimal into your next mathematical conjecture with `ries-rs`,
an equation-discovery engine you can explore in your browser or build into your
research workflow.

[![CI](https://github.com/maxwellsantoro/ries-rs/actions/workflows/ci.yml/badge.svg?branch=main)](https://github.com/maxwellsantoro/ries-rs/actions/workflows/ci.yml)
[![Coverage](https://github.com/maxwellsantoro/ries-rs/actions/workflows/coverage.yml/badge.svg?branch=main)](https://github.com/maxwellsantoro/ries-rs/actions/workflows/coverage.yml)
[![Live Demo](https://img.shields.io/badge/live-demo-0f766e)](https://maxwellsantoro.com/projects/ries-rs/app/)
[![Crates.io](https://img.shields.io/crates/v/ries.svg)](https://crates.io/crates/ries)
[![PyPI](https://img.shields.io/pypi/v/ries-rs.svg?cacheSeconds=300)](https://pypi.org/project/ries-rs/)
[![DOI](https://zenodo.org/badge/1123876688.svg)](https://doi.org/10.5281/zenodo.19101924)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

`ries-rs` is an inverse equation solver that searches for concise mathematical
explanations of numerical values. Inspired by Robert Munafo’s RIES, it combines
expression generation, numerical evaluation, root refinement, and ranking to find
candidate equations involving arithmetic, roots, familiar constants, and
transcendental functions. Its shared Rust engine powers a command-line tool, a
Rust library, Python bindings, and a browser interface, with custom symbols,
reproducible searches, and structured output for further analysis. It gives
researchers, programmers, and curious mathematicians a practical way to turn
numerical observations into testable mathematical ideas.

The historical acronym is RIES, for "RILYBOT Inverse Equation Solver". This
repository aims to be a modern, documented, reproducible reference
implementation rather than a historical clone.

> **Name map:** crates.io crate `ries`; installed CLI `ries-rs`; PyPI package
> `ries-rs`; Python import `ries_rs`.

Project page: [maxwellsantoro.com/projects/ries-rs](https://maxwellsantoro.com/projects/ries-rs)  
Live demo: [maxwellsantoro.com/projects/ries-rs/app/](https://maxwellsantoro.com/projects/ries-rs/app/)

![RIES-RS web UI](docs/assets/web-ui.png)

## What’s new in v3.0.0

- **Precision-aware discovery:** preserves your decimal input and prioritizes the
  simplest equation that explains its precision. For `1.41421356`, discovery
  selects `x = sqrt(2)` and retains `x^2 = 2` as supporting evidence.
- **One workflow across interfaces:** Rust, CLI, Python, and WASM return
  structured hypotheses, supporting equations, resolved settings, and explicit
  verification statuses. Existing raw `search()` APIs remain available.
- **More reliable search:** fixes missed expressions, stopping order, impossible
  composed-trig roots, decimal-boundary handling, and symbol-cost arithmetic.
- **Better browser results and exports:** results stay attached to their
  completed search; mathematical rendering and CAS exports are corrected.
- **Independent checks:** supported polynomial equations can receive exact
  rational root bounds, with qualification tools and full release CI.

See the [v3.0.0 release notes](docs/releases/v3.0.0.md) for details and Rust API
migration guidance, or try the [discovery workflow](docs/DISCOVERY_WORKFLOW.md).

## Benefits over original RIES

[Robert Munafo’s original RIES](https://mrob.com/pub/ries/) established the core
equation-search model, including profiles, symbol customization, and restricted
mathematical classes. `ries-rs` builds on that foundation with a broader toolkit:

| Benefit | What you gain |
| --- | --- |
| Work across environments | CLI, Rust library, Python bindings, and browser/Node WASM packages powered by one engine. |
| Respect decimal precision | Discovery prioritizes concise explanations consistent with the digits you supplied. |
| Reproduce research | Deterministic ordering, structured JSON, run manifests, and exact-bit replay checks. |
| Connect analysis tools | Structured hypotheses and supporting equations, plus LaTeX, SymPy, and Mathematica exports. |
| Choose execution strategies | Parallel generation, parallel turbo refinement, and streaming options for larger searches. |
| Inspect numerical evidence | Optional high-precision reevaluation, explicit unavailable-check statuses, and independent root certificates for a restricted polynomial grammar. |
| Integrate and distribute | Memory-safe Rust, MIT licensing, regression-tested APIs, cross-platform CI, prebuilt executables, and Python wheels. |

Historical ordering can differ, and speed depends on the workload. Numerical
matches suggest conjectures; they do not establish symbolic identities. See
[compatibility status](docs/PARITY_STATUS.md) and the
[search model](docs/SEARCH_MODEL.md) for the current contracts.

## Who It's For

- Researchers who want reproducible constant-recognition runs with structured output and run manifests
- Programmers who want equation search embedded in Rust, Python, or browser workflows
- People comparing modern behavior against historical RIES baselines with explicit parity notes
- Educators and demo authors who want the same engine behind a CLI, Python module, and live browser demo

## Install

### CLI

Use `cargo install ries --locked` to install the `ries-rs` executable.

```bash
cargo install ries --locked
```

Prebuilt Linux, macOS, and Windows archives are attached to each
[GitHub release](https://github.com/maxwellsantoro/ries-rs/releases).

For unreleased development builds:

```bash
cargo install --git https://github.com/maxwellsantoro/ries-rs --locked

# Or, from a local checkout:
cargo install --path . --locked
```

### Python Bindings

```bash
pip install ries-rs
```

For local source development of the bindings:

```bash
uv venv
source .venv/bin/activate
uv pip install maturin
cd ries-py
maturin develop --release
```

For binding checks, see [the validation guide](docs/VALIDATION.md).

### Web App / WASM

GitHub releases include a `ries-rs-wasm.tar.gz` artifact with the generated WASM
packages. To build the browser bundle locally, install the JS dependencies and
the nightly Rust toolchain first:

```bash
npm ci
rustup toolchain install nightly --profile minimal --component rust-src --target wasm32-unknown-unknown
npm run build:web:site
```

Deploy the contents of `dist/web-site/` to a path such as
`https://example.com/projects/ries-rs/`.

The current local WASM build scripts use nightly `wasm-pack` via `-Z build-std`.

For the public deployment, use the canonical landing page at
`https://maxwellsantoro.com/projects/ries-rs` and the standalone app at
`https://maxwellsantoro.com/projects/ries-rs/app/`.

Detailed setup guides:

- [Python bindings](docs/PYTHON_BINDINGS.md)
- [WASM bindings](docs/WASM_BINDINGS.md)
- [Web UI hosting](web/README.md)

## Quick Start

For a rounded decimal, use precision-aware discovery. It selects a minimum-cost
precision-resolving tier before limiting the results, groups safely recognizable
presentations, and preserves the original literal and resolved settings. From a
repository checkout, export a run and apply the independent polynomial checker:

```bash
ries-rs --discover --deterministic --json 1.41421356 > run.json
python3 scripts/verify_discovery.py run.json --output selected.json
python3 scripts/verify_discovery.py selected.json --verify
```

The first hypothesis is `x = sqrt(2)`, with `x^2 = 2` retained as a supporting
equation. The external verifier can independently bound that polynomial root;
named constants and unsupported expressions report unavailable checks. Numerical
agreement does not prove the intended constant equals the equation's root.

Discovery uses API levels 0–5 and may return fewer hypotheses than requested when
only a small tier resolves the input precision. `--discover --json` exports the
versioned `ries-discovery-v1` envelope; it does not change ordinary CLI JSON or
the existing manifest schema. Use ordinary search for custom profiles and the
broader CLI configuration. See [the search contract](docs/SEARCH_MODEL.md) and
[the evidence workflow](docs/DISCOVERY_WORKFLOW.md).

The same additive workflow is available as `ries_rs.discover("1.41421356")` in
Python and `discover("1.41421356")` in JavaScript/WASM. Existing float-based
`search()` calls retain their raw ordering. The browser defaults to discovery and
keeps explicit raw complexity/parity modes.

Basic search:

```bash
ries-rs 3.141592653589793
```

Example output:

```text
                       x = pi                       ('exact' match) {29}
                      -x = -pi                      ('exact' match) {43}
                     1/x = 1/pi                     ('exact' match) {43}
```

An **“exact” match is a numerical classification**, not a symbolic proof that
the intended mathematical constant satisfies the equation. Search uses the
parsed `f64` target. Optional high-precision reevaluation corroborates that
numerical result; it cannot recover decimal digits lost during parsing.
Completeness depends on the grammar, budgets, numerical gates, and pruning.
The [qualification tools](docs/QUALIFICATION.md) provide independent known-answer
checks and an experimental exact rational root-certificate workflow.

For a rounded input such as `ries-rs 3.14159265`, the report's Best
approximations category prefers simpler equations once they match the precision
you typed, so `x = pi` comes before more elaborate fits. Extra trailing zeros
request finer precision. The displayed errors retain their computed values;
this preference does not mark rounded inputs as exact.

Classic-style output:

```bash
ries-rs --classic 2.5063
```

Deterministic machine-readable output:

```bash
ries-rs 3.141592653589793 --deterministic --json --emit-manifest run.json
```

Turbo mode can refine wide candidate windows in parallel.

```bash
ries-rs 2.506314 -l3 --turbo
```

`--turbo` refines wide RHS candidate windows in parallel and admits the results
in serial order. Materialized serial and turbo searches return the same ranked
matches. Turbo can materialize larger searches before switching to streaming.

Search preserves distinct left-hand sides by default. For a materialized search,
`--lhs-value-dedup` can reduce matching work by merging left-hand sides with the
same value and derivative at the target. It can change which equations and
roots are returned. See the [search model](docs/SEARCH_MODEL.md) for the measured
tradeoff and streaming fallback behavior.

For the authoritative option list:

```bash
ries-rs --help
```

### Search with a custom profile

The [Fano–Heawood example profile](examples/fano-heawood.ries) gives the search
names and weights for the Heawood graph’s 14 vertices and 21 edges. From the
repository root, search for a rounded value of π/21 to see `x = pi/edges`.

```bash
ries-rs --profile examples/fano-heawood.ries --one-sided --deterministic \
  --report=false -l0 -n1 0.14959965017094254
```

## Project Scope

`ries-rs` is intended to be a disciplined, modern reference implementation:

- Faithful reimplementation of the core RIES search model
- Deterministic and documented execution modes
- Memory-safe Rust implementation with optional parallel generation and a
  parallel `--turbo` match phase
- Structured output for automation (`--json`, `--emit-manifest`)
- CLI, Rust library, Python bindings, and WebAssembly builds
- Modular presets, profiles, and extension points

Not primary goals of this repository:

- Automated proof discovery or treating numerical matches as established theorems
- Turning the bundled PSLQ integer-relation mode into a separate research platform
- Experimental search branches outside the core RIES model

## Performance

Performance claims are tracked conservatively with separate benchmark artifacts
for end-to-end CLI runs and generation-only scaling:

- End-to-end CLI baseline:
  [`docs/benchmarks/2026-03-20-level3-baseline.md`](docs/benchmarks/2026-03-20-level3-baseline.md)
  records a March 20 workload at its captured source and environment, with about
  `8.1M` candidate pairs and near break-even sequential/parallel times. These
  measurements predate later search changes and are not a performance claim for
  the current candidate. The default `--parallel` path parallelizes generation;
  the matching/Newton phase stays serial to preserve byte-identical results.
- `--turbo` parallelizes Newton refinement for wide RHS candidate windows. It
  uses the same traversal, adaptive gates, and result pool as serial matching.
  See [the search model](docs/SEARCH_MODEL.md#turbo-contract) for the contract.
- Generation-only scaling:
  [`docs/benchmarks/2026-02-25-generation-parallel-scaling.md`](docs/benchmarks/2026-02-25-generation-parallel-scaling.md)
  reports `3.18x` median speedup for parallel generation.

Raw benchmark artifacts live under `docs/benchmarks/artifacts/`.

## Compatibility

`ries-rs` tracks behavior against two historical baselines while keeping a
Rust-native engine and build surface:

1. The original RIES by Robert Munafo
2. The `clsn/ries` fork with additional compatibility-oriented CLI behavior

Current status in brief:

- Core equation search and classic-style output flow are implemented
- Legacy CLI semantics and diagnostic channels are supported substantially more
  completely than in early versions
- The Rust-native engine keeps modern build and integration advantages, while
  edge-case ordering or complexity scores can still differ from historical
  binaries on some targets

See [`docs/PARITY_STATUS.md`](docs/PARITY_STATUS.md) for the detailed status and
historical notes.

## How It Works

1. Enumerate valid postfix expressions up to the current complexity limit
2. Check fast-path exact matches against well-known constants when possible
3. Generate left-hand-side and right-hand-side expression candidates
4. Use Newton refinement to solve `LHS(x) = RHS`
5. Filter, deduplicate, and refine candidate equations
6. Rank matches by exactness, then error (exact matches rank by simplicity, not
   sub-tolerance residual), then parity-style or complexity-style order

## Documentation

- [Roadmap and parallel work packets](docs/ROADMAP.md)
- [Documentation map](docs/README.md)
- [Search model](docs/SEARCH_MODEL.md)
- [Complexity and weights](docs/COMPLEXITY.md)
- [Architecture overview](docs/ARCHITECTURE.md)
- [Performance notes and benchmarks](docs/PERFORMANCE.md)
- [Parity and compatibility status](docs/PARITY_STATUS.md)
- [Python bindings](docs/PYTHON_BINDINGS.md)
- [WASM bindings](docs/WASM_BINDINGS.md)
- [Web UI build and hosting](web/README.md)

## Contributing

If you want to improve the engine, docs, packaging, or release surfaces, start
with [CONTRIBUTING.md](CONTRIBUTING.md) for setup and verification expectations.

## Additional Interfaces

### Python

The Python bindings expose `ries_rs.search()` and typed match objects through
PyO3. See [docs/PYTHON_BINDINGS.md](docs/PYTHON_BINDINGS.md) for PyPI install,
source development, API details, and troubleshooting.

### WebAssembly

The WASM build supports browser, Node.js, bundler, and static-site workflows.
See [docs/WASM_BINDINGS.md](docs/WASM_BINDINGS.md) for the JS/TS API and
[web/README.md](web/README.md) for the browser UI and static hosting flow.

### PSLQ

The CLI includes PSLQ integer-relation detection via `--pslq`,
`--pslq-extended`, and `--pslq-max-coeff`. This is part of the shipped tool,
but it is not the primary focus of the repository.

## How to Cite

If you use `ries-rs` in academic work, cite the project version you used.
`CITATION.cff` is the canonical metadata source.

```bibtex
@software{ries-rs2026,
  author       = {Santoro, Maxwell},
  title        = {ries-rs: A Rust Implementation of the RIES Inverse Equation Solver},
  year         = {2026},
  version      = {3.0.0},
  url          = {https://maxwellsantoro.com/projects/ries-rs},
  license      = {MIT},
  note         = {Features parallel search, deterministic mode, and run manifest for reproducibility}
}
```

Zenodo concept DOI for the project and its archived releases:
`10.5281/zenodo.19101924`

The latest verified version DOI is for `v1.1.1`:
`10.5281/zenodo.19101925`

No version DOI is recorded for `v3.0.0` until its Zenodo archive has completed.

Use the concept DOI when you want a stable project-level reference that resolves
to the latest archived release. Use the version DOI when you need to cite the
specific archived release you used.

For reproducible research runs, prefer `--deterministic` together with
`--emit-manifest`.

When `--emit-manifest` is requested, failure to serialize or write the manifest
prints an error and exits nonzero. A parent directory must already exist, and
the destination must be a writable file path.

From a repository checkout, replay a self-contained CLI run with
`python3 tools/replay_manifest.py run.json --bin ries-rs`. The tool checks the
recorded settings and results. Runs that rely on profile files may require
manual reproduction.

## License

MIT License. See [`LICENSE`](LICENSE).

## References

- [Original RIES](https://mrob.com/pub/ries/) by Robert Munafo
- [RIES Documentation](https://mrob.com/pub/ries/ries.html)
- [`clsn/ries` fork](https://github.com/clsn/ries)
- Stoutemyer, D.R. (2024). "Computing with No Machine Constants, Only
  Constructive Axioms". arXiv:2402.03304

## Strict configuration and resource-sensitive runs

Use `--strict-config` in reproducible experiments to fail with exit code 2 on
invalid custom constants/functions, malformed symbol weights/names, unknown symbol
set codes, invalid memory hints, or unavailable precision options. The default
compatibility mode retains warnings for malformed custom definitions. Replay
manifests preserve the strict flag; CLI JSON also preserves the original target
literal separately from its `f64` value.

`--max-memory` selects a strategy and does not enforce an RSS ceiling.
The [Linux resource supervisor](docs/QUALIFICATION.md#enforced-resource-limits-on-linux)
adds an OS-enforced address-space budget and timeout. An aborted run is incomplete.

When `--match-all-digits` is set, the original decimal must survive conversion to
`f64` within half its last typed decimal place. Inputs such as
`1.000000000000000000000000000001` are rejected instead of searching for exactly
1. Nonzero decimals that underflow to zero (for example `1e-400`) are rejected
in every decimal-input search mode. Exact representable values with many trailing
zeros remain valid. This validation does not change the engine to arbitrary precision.

A requested `--precision` now fails if verification is unavailable. Use
`--allow-unverified` explicitly to continue and obtain per-result `unavailable`
status in JSON. This covers missing feature support, at most 53 bits, and
unsupported high-precision operators.

Adaptive searches grow their own budgets: JSON `max_lhs_complexity` and
`max_rhs_complexity` report the actual final bounds, while
`requested_max_lhs_complexity` and `requested_max_rhs_complexity` retain the level
mapping. Manifests record actual bounds and requested bounds in replay metadata.
Adaptive and one-sided modes reject streaming, turbo, and memory-hint flags that
they cannot honor; memory hints still do not impose a hard RSS ceiling.

`--pslq --match-all-digits` is explicitly unsupported and returns a configuration
error before results. Decimal validation still applies to PSLQ targets, but a
PSLQ residual tolerance is not a general root-distance guarantee. Use ordinary
numerical search for the match-all-digits result contract.
