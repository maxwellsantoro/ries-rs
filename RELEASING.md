# Releasing

Release-process notes and templates for `ries-rs`.

## Next release: 3.0.0

The current unreleased batch must ship as 3.0.0. The public
`ExpressionConstraintOptions::user_function_constraints` field breaks exhaustive
Rust struct literals, and the corrected stop contract changes first results.
Callers should add `..Default::default()` where appropriate, or explicitly supply
body summaries using `UserFunctionConstraints::from_function` when restrictions
must inspect user functions. Do not publish this batch under a 2.x version.

During release preparation, update all versioned surfaces listed below together,
including both lockfiles and release notes. Metadata is staged at 3.0.0 and
remains unpublished. Set `CITATION.cff` `date-released` to the actual publication
date when releasing and rerun preflight. Preparation does not authorize tagging
or publishing.

## Maintainer Checklist

### Preflight (Local)

Before tagging a release:

1. Run the automated integrity preflight:
   - `uv run --no-project --with tomli python3 scripts/check_release_integrity.py`
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

The integrity check covers the versioned release metadata surfaces that have to
move together:

- `README.md`
- `CHANGELOG.md`
- `CITATION.cff`
- `Cargo.toml`
- `package.json`
- `ries-py/Cargo.toml`
- `ries-py/pyproject.toml`
- `docs/releases/vX.Y.Z.md`

### CI/Workflow Preconditions

- The release workflow calls the full reusable CI workflow at the tagged commit. Every artifact build depends on that successful run through `validate-tag`; publication cannot bypass it. CI includes built browser smoke tests and retained matcher/historical qualification evidence.
- Before tagging, run `python3 scripts/qualify_search.py --output output/qualification --repeats 3` with a release binary, inspect the missing-equation and certificate results, and follow [QUALIFICATION.md](docs/QUALIFICATION.md). A local dirty-tree baseline is not release qualification.
- Release automation is tag-driven via `push` tags matching `v*` in `.github/workflows/release.yml`.
- CI coverage for release-related surfaces is defined in `.github/workflows/ci.yml` (release integrity, Rust checks/tests, WASM tests, Python bindings crate check).
- The historical root-accuracy smoke job downloads and verifies the pinned original RIES source; download or checksum failure fails CI. It checks root accuracy, not expression or ranking parity.
- GitHub release publishing now assumes both registry publishing paths are configured:
  - crates.io via repository secret `CARGO_REGISTRY_TOKEN`
  - PyPI via repository secret `PYPI_API_TOKEN`
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
  - 3 Python stable-ABI wheels (Linux x86_64, macOS aarch64, Windows x86_64)
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

### Website Handoff

This repository owns release-artifact correctness. It does not own the final
deployment validation for `https://maxwellsantoro.com/projects/ries-rs/app/`.

After a WASM release or release-candidate build is ready:

1. Import the updated static bundle into the website repository
2. Run the website repository's demo-asset normalization step
3. Run the website repository's landing-page and live-demo smoke checks
4. Only then treat the public web demo as release-ready

Keep the artifact gate here focused on CLI archives, WASM packages, and Python
distributions. Keep landing-page metadata, app-shell metadata, and deployed
demo correctness in the website repository.

### Post-Release

1. Record any verified Zenodo DOI status for the released version
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

Record a DOI here only after Zenodo archival has actually completed and the DOI
has been verified in project metadata.

- `v0.1.0`: no Zenodo DOI recorded in this repository
- `v1.0.0`: no Zenodo DOI recorded in this repository
- `v1.0.1`: no Zenodo DOI recorded in this repository
- `v1.1.0`: no Zenodo DOI recorded in this repository
- `v1.1.1`: version DOI `10.5281/zenodo.19101925`; concept DOI `10.5281/zenodo.19101924`
- `v2.0.0`: no Zenodo DOI recorded in this repository
- `v2.0.1`: no Zenodo DOI recorded in this repository
- `v2.0.2`: no Zenodo DOI recorded in this repository
- `v2.0.3`: no Zenodo DOI recorded in this repository
- `v2.1.0`: no Zenodo DOI recorded in this repository

## Immutable release-candidate review

Commit the candidate before final validation. Record both its commit and Git tree
SHA, run qualification from a clean checkout, and require each recorded source
hash to equal `git show <candidate>:<path>`. Retain evidence outside that checkout
or in a separate evidence commit that explicitly names the source commit; do not
claim an evidence file can identify its own containing commit.

`python3 scripts/test_release_consumers.py --output /outside/checkout/consumers`
requires a clean commit and tests the installed CLI built from the Cargo package,
an installed wheel, an installed npm tarball in Node and in the built browser UI,
and manifest creation/exact replay including explicit unavailable verification.
CI runs it at the checked-out SHA and retains a commit/tree-bound receipt. The
browser smoke suite uses the npm-installed WASM for the built-site path.
No RC review step tags or publishes. Obtain a fresh independent review of that
immutable source and inspect hosted CI before authorizing a release.

Consumer validation requires a fresh output directory and distinct explicit Cargo
target directories for packaging, CLI installation, Python, and WASM. Shared
same-version outputs are not accepted as artifact provenance. Failed receipts
are retained; reruns use a new directory.
