# Validation: check the changed surface once

This is the local validation decision guide. CI jobs in
[ci.yml](../.github/workflows/ci.yml) remain the executable gate; release-candidate
requirements remain in [RELEASING.md](../RELEASING.md). The
[roadmap](ROADMAP.md) assigns packet ownership and integration responsibility.

## Fast development loop

1. Record the starting working tree so pre-existing user changes remain distinct
   from the current patch. Identify affected runtime surfaces and regression
   expectations before choosing checks.
2. Run the smallest meaningful regression for the behavior being changed. A
   worker owns focused tests; the coordinator owns the integrated surface checks.
3. After focused checks pass, run the applicable rows below once on the integrated
   state. Do not rerun every suite when only docs, comments, or unrelated files
   changed. Do not treat a build cache as a record that tests passed.
4. Record commands, results/skips, source/configuration identity, and evidence
   location. Reuse valid evidence; rerun checks whose inputs changed or whose
   previous outcome was failed, incomplete, or uncertain.

Use Cargo's normal incremental cache for successive checks in one checkout.
Serialize Cargo commands sharing a target directory; concurrent workers use
separate worktrees/target directories only when they actually need simultaneous
builds. Do not spawn a full build per agent by default. Reuse an installed Python
extension or built WASM package for test-only/UI-only iteration when its native
inputs are unchanged. Tests must load the artifact just validated, with its path
and hash recorded when provenance matters.

## Local check matrix

Commands assume the repository root. Choose relevant rows; this is not a command
list to run in full for every patch. `nextest` does not run Rust doctests, so the
Rust row includes them separately.

| Changed surface | Focused iteration | Before integration is accepted |
| --- | --- | --- |
| Prose, roadmap, agent guidance, project skills | `python3 scripts/check_docs.py` | Same link check; `python3 scripts/check_release_integrity.py` for public/release docs. No Rust/WASM rebuild for prose alone. |
| CI routing or doc validator | The corresponding `tests/test_ci_scope.py` / `tests/test_docs_check.py` via unittest discovery | Full cheap Python tool suite; workflow syntax/expression lint if available; inspect full/non-PR and skipped-job behavior. Hosted CI still must validate the workflow. |
| Core Rust / CLI / ranking / profiles / shared output | `cargo test --locked --test numeric_contract <filter>` or the relevant library/integration test | Format; default/no-default/highprec Clippy; default + highprec nextest; Rust doctests; affected binding/browser checks and independent qualification for numerical/search changes. |
| Python adapter | `./scripts/test_ries_py_rust.sh <filter>` or pytest against a current installed wheel | Binding Rust formatting (`cargo fmt --manifest-path ries-py/Cargo.toml -- --check`) and both binding helpers; update Python docs. Core changes beneath Python also select the core row. |
| WASM adapter / generated package interface | Focused WASM tests and package entry-point check against freshly built targets | WASM Node tests, relevant web/Node/bundler package builds, package-entry checks, browser integration. A native `--features wasm` build alone does not execute wasm32 code. |
| Browser HTML/JS/styles | Focused Playwright regression against current WASM | Browser smoke including built-site path and generated CSS consistency. Rebuild WASM only if Rust/features/toolchain/build inputs changed. |
| Numeric evaluation, root validity, generation, pool ordering, decimal input | Targeted positive/negative counterexamples and relevant Rust tests | Core row plus fixed matcher qualification, decimal oracle, and independent checks for supported grammar. Preserve raw artifacts and explicit unavailable statuses. |
| CAS serializers | Expression/solver and browser clipboard regressions as applicable | Execute exported expressions with pinned SymPy; real browser test for JS serializers. Mathematica output needs a kernel check when one is available; string tests do not establish kernel semantics. |
| Qualification/replay/resource tools | `python3 -m unittest discover -s tests -p 'test_*py'` | Relevant independent corpus/oracle; actual Linux resource-limit tests on Linux. No timing study merely because a test changed. |
| Packaging, dependencies, release metadata/build scripts | Integrity and focused consumer/package test | Applicable surface rows; fresh installed-consumer validation for an immutable candidate. Lockfile changes invalidate affected dependency evidence. |

### Commands behind the matrix

Cheap documentation/tooling checks (no engine compilation):

```bash
python3 scripts/check_docs.py
python3 scripts/check_release_integrity.py
python3 -m unittest discover -s tests -p 'test_*py'
```

Core integrated checks:

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --locked -- -D warnings
cargo clippy --all-targets --no-default-features --locked -- -D warnings
cargo clippy --all-targets --features highprec --locked -- -D warnings
cargo nextest run --tests --locked
cargo nextest run --tests --features highprec --locked
cargo test --doc --locked
```

Without nextest, substitute `cargo test --tests --locked` (and the corresponding
feature flags). Run a no-default test build when changing feature gates, serial
fallbacks, or parallel-only imports; the no-default Clippy build in the core row is a
compile/lint check, not execution coverage. `highprec` requires GMP/MPFR.

Python uses `uv` and `maturin`; public installation can use pip, but local
validation should use the maintained helpers:

```bash
./scripts/test_ries_py_rust.sh
./scripts/test_ries_py_python.sh -q
```

The second helper builds and installs a wheel in an isolated environment. For
additional Python-test edits against that unchanged extension, run pytest directly
with that environment's interpreter; another wheel build adds no evidence. A
binding/core/feature/dependency/toolchain change requires rebuilding it.

Browser iteration with an existing current WASM build and built-site bundle:

```bash
npm run build:css
bash scripts/build_web_site.sh
npm run test:web:smoke
```

Run `npm ci --no-fund --no-audit` when dependencies or environment setup changed,
and install Chromium once per environment. Use
`npm run test:web:smoke:build` for a fresh web/WASM build. The root package scripts
require the `nightly` toolchain alias with rust-src and wasm32 installed. A dated
nightly can be used explicitly with equivalent build flags; record which one.
Keep Playwright workers bounded; parallel search-heavy browser tests compete for
CPU/memory and can make timings and failures misleading.

Independent numerical and serialization checks:

```bash
cargo build --bin ries-rs --locked
uv run --no-project --with sympy==1.14.0 python scripts/check_cas_exports.py --binary target/debug/ries-rs
python3 scripts/check_decimal_boundary.py --binary target/debug/ries-rs --output output/decimal-boundary.json
cargo build --release --locked
python3 scripts/qualify_search.py --output output/qualification
python3 scripts/qualify_discovery.py --binary target/release/ries-rs --output output/discovery
```

See [QUALIFICATION.md](QUALIFICATION.md) for corpus scope, historical differential,
certificates, and resource enforcement. One qualification repeat checks the
contract; repeated timing measurements belong to an explicitly requested
performance study. [PERFORMANCE.md](PERFORMANCE.md) defines benchmark evidence.

## Evidence reuse and invalidation

A short receipt in the task record or `output/` is enough for local development:

```text
check: numeric_contract / reciprocal_trig_levels
command: cargo test --locked --test numeric_contract reciprocal_trig_levels
source: HEAD + patch identity (including relevant untracked source/fixtures)
inputs: Rust sources/tests, Cargo manifests/locks, compiler/target/features,
        relevant environment/profile/options, external fixture/artifact hashes
result: passed; skips: none; log: output/<task>/numeric-contract.log
```

A full relevant input set includes transitive shared-core dependencies, not just
the edited test file. Record `rustc -Vv`, feature/build flags, and artifact identity
for compiled checks. A diff hash alone omits untracked files, toolchains, and
external inputs. If the dependency set is uncertain, broaden it and rerun rather
than claiming reuse. Do not implement a silent persistent test-success cache.

Examples:

- Editing the roadmap after a passing engine suite requires doc checks, not the
  same engine suite again. Editing `src/eval.rs` invalidates native and binding
  runtime evidence that depends on evaluation.
- Editing a browser view requires browser tests and refreshing the static bundle;
  it does not require recompiling an unchanged WASM binary. Editing `src/wasm.rs`
  requires a new WASM build and affected package/browser tests.
- A new regression test must run. A focused filter passing does not establish
  that an entire suite passed. Report skipped/platform-only checks explicitly.
- Compiler, lockfile, flags, custom profile, grammar, generated code, or external
  fixture changes invalidate affected evidence even when source files did not
  change. Cancellation, timeouts, and partial output never count as success.
- Security advisories and hosted/deployed state are time-sensitive. Earlier
  results do not establish current status. CI performs fresh selected audits;
  final release CI and installed artifacts are checked at the exact candidate.

## CI selection and required checks

[scripts/ci_scope.py](../scripts/ci_scope.py) selects lanes for the complete PR
diff against its merge base. Paths are NUL-delimited; renames include both source
removal and destination addition. Unknown paths, unreadable metadata, or a failed
diff select the full matrix. Unit tests cover mixed changes, missing metadata,
renames, unusual filenames, and aggregate-gate failures.

- Every PR runs release metadata integrity, active-doc local-link validation, and
  the cheap Python tool tests. Documentation-only changes skip native/binding/
  browser builds. The doc checker excludes historical archives/release notes/
  benchmark evidence and does not validate remote URLs or heading anchors.
- Browser-only changes select browser checks; package metadata/build changes also
  select installed-consumer checks. Python
  changes select binding, feature, and installed-consumer checks. Qualification
  tools select qualification. Core Rust, CI, dependencies, and unclassified paths
  select broadly. The classifier is authoritative for exact path categories.
- Main/master pushes, manual dispatches, and reusable release calls use all lanes.
  Coverage uses the same Rust-lane decision; its 50% floor is unchanged. Benchmarks
  remain on demand, so ordinary changes do not trigger repeated timing studies.
- Superseded PR runs are cancelled. Release and branch runs are not cancelled by
  this policy. An intentional skipped lane is different from a failed,
  cancelled, missing, or unexpectedly skipped selected lane.

Require both always-run checks, **CI result** and **Coverage result**, in repository
branch protection. CI result fails if scope/integrity fails or any selected job
fails, is cancelled, or is unexpectedly skipped. Coverage result independently
requires a successful scope decision and successful selected coverage; an
intentionally unselected coverage job may be skipped. Coverage remains a separate
workflow and is not included in CI result. Existing individual job names remain
available. Branch-protection configuration is external to this repository and is
not changed by editing the workflows.

## Final release gate

Local receipt reuse accelerates development; it does not replace exact-commit
release validation. Run the full reusable CI and fresh installed-consumer checks
from a clean immutable candidate, retain source/artifact-bound evidence, and
follow [RELEASING.md](../RELEASING.md). Never reuse same-version wheel/npm/Cargo
outputs as provenance for a different candidate. Deployment validation belongs
to the website repository; no local result proves the live demo is current.
