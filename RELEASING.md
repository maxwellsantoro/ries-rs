# Releasing

Release-process notes and templates for `ries-rs`.

## Maintainer Checklist

### Preflight (Local)

Before tagging a release:

1. Confirm docs and metadata are up to date (`README.md`, `CHANGELOG.md`, `CITATION.cff`, `Cargo.toml`, `package.json`, `ries-py/pyproject.toml`)
2. Run formatting and lint checks:
   - `cargo fmt --all -- --check`
   - `cargo clippy --all-targets --locked -- -D warnings`
   - `cargo clippy --all-targets --no-default-features --locked -- -D warnings`
   - `cargo clippy --all-targets --features highprec --locked -- -D warnings`
3. Run test checks you expect CI to enforce:
   - `cargo test --tests --locked`
   - `cargo check --manifest-path ries-py/Cargo.toml --locked`
   - `cargo build --features wasm --locked`
4. Sanity-check packaging:
   - `cargo package --allow-dirty --locked`
   - `cd ries-py && maturin sdist --out dist-check`
5. Review pending changes:
   - `git status --short`

### CI/Workflow Preconditions

- Release automation is tag-driven via `push` tags matching `v*` in `.github/workflows/release.yml`.
- CI coverage for release-related surfaces is defined in `.github/workflows/ci.yml` (Rust checks/tests, WASM tests, Python bindings crate check).
- The parity check job is optional when `ries-original/ries.c` is not vendored (it skips automatically).
- GitHub release publishing now assumes both registry publishing paths are configured:
  - crates.io via repository secret `CARGO_REGISTRY_TOKEN`
  - PyPI via Trusted Publishing for the GitHub environment named `pypi`
  - Registry publish steps are rerun-safe:
    - crates.io skips upload if the tagged `ries` version already exists
    - PyPI uses `skip-existing` for already-uploaded files

### Create the Release

1. Update `CHANGELOG.md` and commit all release-ready changes
2. Create and push an annotated tag:
   - `git tag -a vX.Y.Z -m "vX.Y.Z"`
   - `git push origin vX.Y.Z`
   - If present, the release workflow will use `docs/releases/vX.Y.Z.md` as the
     GitHub release body; otherwise it falls back to `.github/release-template.md`
3. Monitor the GitHub Actions release workflow:
   - `build-binaries` (Linux/macOS/Windows CLI artifacts)
   - `build-wasm` (`pkg`, `pkg-node`, `pkg-bundler`)
   - `build-python` (wheels from `ries-py/`)
   - `build-python-sdist` (source distribution from `ries-py/`)
   - `publish-crate` (crates.io upload)
   - `publish-python` (PyPI upload)
   - `create-release` (GitHub release publication after registry publishes succeed)

### Artifact Verification

After the workflow finishes, verify the GitHub release contains:

- CLI archives for Linux/macOS (x86_64 + aarch64 macOS) and Windows zip
- WASM tarball (`ries-rs-wasm.tar.gz`)
- Python wheels (`*.whl`)
- Python source distribution (`*.tar.gz`)

Spot-check at least one artifact per surface if possible:

- CLI binary runs and prints version/help
- WASM package loads in the `web/` demo or a Node import test
- Python wheel imports `ries_rs` and runs a minimal `search(...)`

### Release Gate (Go/No-Go)

Use this as the final pass/fail checklist before announcing a release.

Go only if all of the following are true:

- `CI` is green for the release commit/tag (at minimum: format, clippy, tests, WASM tests, audit, feature checks)
- GitHub release workflow completed successfully (`build-binaries`, `build-wasm`, `build-python`, `build-python-sdist`, `publish-crate`, `publish-python`, `create-release`)
- Expected artifact groups are present on the GitHub release:
  - 4 CLI artifacts (Linux x86_64, macOS x86_64, macOS aarch64, Windows x86_64 zip)
  - 1 WASM tarball (`ries-rs-wasm.tar.gz`)
  - >=1 Python wheel artifact set (platform-dependent wheel files)
  - 1 Python source distribution (`ries_rs-*.tar.gz`)
- Registry publishes completed and are externally visible:
  - crates.io package page for the tagged version
  - PyPI project page for the tagged version
- `CHANGELOG.md` and release notes describe the shipped version accurately
- No known P0 regressions discovered during smoke checks

No-go conditions (fix first):

- Any required CI/release job failed or was skipped unexpectedly
- Missing or misnamed artifacts in the GitHub release
- CLI binary fails basic startup (`--help` / version)
- Python wheel import fails (`import ries_rs`)
- WASM bundle cannot initialize in a basic smoke test

Suggested smoke checks (one per artifact surface):

- CLI (after extracting a release archive):
  - `./ries-rs --help`
  - `./ries-rs 3.141592653589793 -n 3`
- WASM (Node/package sanity):
  - `tar -tzf ries-rs-wasm.tar.gz | head`
  - `npm run test:web:smoke` (from a clean checkout with build prerequisites installed), or a minimal Node import check against `pkg-node/`
- Python wheel (inside a temporary venv):
  - `python -m venv .venv-release-check`
  - `. .venv-release-check/bin/activate`
  - `pip install <wheel-file.whl>`
  - `python -c "import ries_rs; print(len(ries_rs.search(3.14159)))"`

### Post-Release

1. Add the released version + DOI status to this file (or move DOI tracking to a dedicated release log if preferred)
2. Confirm `CHANGELOG.md` has a new `Unreleased` section for follow-up work
3. Verify docs/release notes links still point to current files (`docs/PARITY_STATUS.md`, `RELEASING.md`)

## Release Notes Template

When creating a release, include:

1. Summary: Brief description of the release
2. New Features: List of new capabilities
3. Bug Fixes: List of fixed issues
4. Breaking Changes: Any incompatible changes
5. Deprecations: Features scheduled for removal
6. Contributors: Acknowledge contributors

## DOI / Zenodo

After each release, Zenodo will assign a DOI. Record it here (or in project release notes/docs):

- `v0.1.0`: DOI pending assignment
- `v1.0.0`: DOI pending assignment
- `v1.0.1`: DOI pending assignment
