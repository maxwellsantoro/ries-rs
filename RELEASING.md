# Releasing

Release-process notes and templates for `ries-rs`.

## 3.0.0 release requirements

This batch ships as 3.0.0. The public
`ExpressionConstraintOptions::user_function_constraints` field breaks exhaustive
Rust struct literals, and the corrected stop contract changes first results.
Callers should add `..Default::default()` where appropriate, or explicitly supply
body summaries using `UserFunctionConstraints::from_function` when restrictions
must inspect user functions. Do not publish this batch under a 2.x version.

During release preparation, update all versioned surfaces listed below together,
including both lockfiles and release notes. Set `CITATION.cff` `date-released`
to the actual publication date when releasing and rerun preflight. Preparation
does not authorize tagging or publishing.

## Maintainer Checklist

### Candidate preparation

1. Update the release metadata below and run
   `python3 scripts/check_release_integrity.py` (Python 3.11+; use
   `uv run --no-project --with tomli python3 scripts/check_release_integrity.py`
   on older Python).
2. Finish the affected integration checks from
   [VALIDATION.md](docs/VALIDATION.md), then commit the release-ready source.
3. Validate that immutable candidate using the full release gate in
   [VALIDATION.md](docs/VALIDATION.md) and the consumer procedure below. Retain
   commit/tree-bound receipts and inspect hosted CI before tagging.

Workers should not repeat the whole release matrix while implementing packets.
The coordinator owns the final full gate; focused development receipts help
iteration but do not replace clean-candidate release evidence.

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
- [ci.yml](.github/workflows/ci.yml) selects affected lanes on pull requests and
  runs the full gate on main-branch pushes, manual runs, and reusable release
  calls. A PR's intentionally skipped lanes are not release qualification.
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

If a runner or workflow failure requires a repair after tagging, merge the
workflow repair into `main`, then dispatch the release workflow with the existing
tag, for example `gh workflow run release.yml --ref main -f tag=v3.0.0`. Every
checkout and the full reusable CI gate use the selected tag; the release notes,
version check, and GitHub release also use that tag. The workflow implementation
comes from `main`, so retain both the workflow revision and the source commit
from the installed-consumer receipt. Preserve the existing tag and monitor the
new run through publication. Linux Python 3.9 wheels use Ubuntu 24.04 because
`setup-python` does not provide that interpreter on Ubuntu 26.04.

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
- Python wheel (inside an isolated environment managed by `uv`):
  - `uv venv .venv-release-check`
  - `uv pip install --python .venv-release-check/bin/python <wheel-file.whl>`
  - `.venv-release-check/bin/python -c 'import ries_rs; print(len(ries_rs.search(3.14159)))'`

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

Update npm root metadata structurally (for example `npm version --no-git-tag-version --ignore-scripts`), never with a global version-string replacement in the lockfile. Consumer validation uses a fresh npm cache so tarball integrity is checked against the committed resolution.
