# Contributing to RIES-RS

Start with [the roadmap](docs/ROADMAP.md) for current priorities and packet
ownership. [VALIDATION.md](docs/VALIDATION.md) is the authoritative guide to
change-scoped checks, evidence reuse, integration, and the full release gate.

## Development setup

- Rust stable via [rustup](https://rustup.rs/); CI pins its compiler in
  [ci.yml](.github/workflows/ci.yml).
- `highprec`: GMP and MPFR (`brew install gmp mpfr` on macOS;
  `sudo apt-get install libgmp-dev libmpfr-dev` on Ubuntu/Debian).
- Python: [`uv`](https://docs.astral.sh/uv/) and `maturin`. Use
  [the binding guide](docs/PYTHON_BINDINGS.md) for a local environment.
- Web/WASM: Node.js, Rust nightly, and `wasm-pack`; see
  [the web guide](web/README.md).

Build the native CLI with `cargo build --locked`. Build only the surface you are
working on; `--all-features` includes threaded WASM and is not a general native
build shortcut.

## Working on a packet

1. Read the relevant contract and identify the observable behavior to change.
2. Agree on exclusive file ownership and shared interfaces before parallel edits.
3. Add a regression that fails for the old behavior, implement the change, and run
   the focused checks selected by [VALIDATION.md](docs/VALIDATION.md).
4. Record changed files, exact checks and outcomes, reused evidence, and remaining
   limitations in the handoff. The coordinator runs the affected integration
   surfaces once after combining the packets.

Passing evidence remains useful only while its inputs remain unchanged. Reuse
build caches and valid check receipts; do not ask every worker to rerun the full
matrix or treat a stale artifact as a current build. Full release qualification
still runs on an immutable candidate under [RELEASING.md](RELEASING.md).

## Source and documentation map

Code and regression tests define current behavior. Future work belongs in
[ROADMAP.md](docs/ROADMAP.md); archived plans are historical context.

| Change | Primary reference to update when behavior changes |
|--------|---------------------------------------------------|
| CLI / search / numerical contract | [SEARCH_MODEL.md](docs/SEARCH_MODEL.md) |
| Symbol costs / generation budgets | [COMPLEXITY.md](docs/COMPLEXITY.md) |
| Module boundaries / shared interfaces | [ARCHITECTURE.md](docs/ARCHITECTURE.md) |
| Python API | [PYTHON_BINDINGS.md](docs/PYTHON_BINDINGS.md) |
| WASM API / browser | [WASM_BINDINGS.md](docs/WASM_BINDINGS.md), [web/README.md](web/README.md) |
| Qualification / independent evidence | [QUALIFICATION.md](docs/QUALIFICATION.md) |
| Check selection / CI | [VALIDATION.md](docs/VALIDATION.md) |
| Release artifacts / metadata | [RELEASING.md](RELEASING.md) |

Use [the documentation index](docs/README.md) for the complete map. Update the
README only when a change affects the overview or common usage. Keep published
release notes and benchmark evidence tied to their original versions; add new
evidence rather than rewriting history.

## Pull requests

Describe the problem, resulting behavior, relevant regression coverage, and the
checks actually run. Explain skipped or reused checks through their change scope
and evidence. Keep versioned release metadata synchronized and change it only
when preparing a release. See [RELEASING.md](RELEASING.md).

Coverage is an aggregate regression floor; it does not establish mathematical
correctness or coverage of every high-risk path.
