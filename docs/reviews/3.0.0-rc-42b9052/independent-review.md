# Independent release-candidate agent review

Final candidate: `42b90521e0808b467e58f9e4c4bbb33d4d0f1b3f`.

This is a separate read-only agent review. It is not external human acceptance, hosted CI acceptance, or authorization to publish. No source checkout, Git state, installed artifacts, tags, PRs, or publication state was changed by this reviewer. Scratch sources, binaries, and execution logs are under `/tmp` outside the source checkouts.

## Reviewed source

- Core candidate `c94d781e58565e0f275655150748df5f112d4165`, inspected in the immutable clean checkout `/tmp/ries-c94d781-review`; applicable AGENTS.md read.
- Isolation-only follow-up `ac6265bb19a82ddc98cab314b7674c40121d6da4`, inspected through committed Git objects from `/Users/maxwell/projects/math/ries-rs`.
- Final lockfile/cache follow-up `517e269d388b45e994a4eb48c9f1b50661a357ce`, inspected through committed Git objects from the same repository.

## Scope and conclusions

No additional release-blocking findings within the inspected scope.

Source inspection covered `src/main.rs` target validation before PSLQ and expression-mode dispatch, positional/legacy target literals and `--at`; `src/target.rs` exact decimal half-last-place comparison, signed inputs, exponent handling, subnormal decomposition, and finite/underflow checks; `src/cli/manifest.rs` original literal preservation and replay invocation construction; `tools/replay_manifest.py` argument allowlist, target/literal consistency, exact result f64 bits including signed zero, and explicit tolerant comparison; `scripts/algebraic_certificate.py` rational parsing and arithmetic preflight, polynomial restrictions, independent root isolation and verification; and `scripts/test_release_consumers.py` plus installed-consumer CI wiring and packaged browser WASM selection.

The parent independently discovered a stale installed CLI when an ambient same-version shared Cargo target directory was used. This review does not minimize that original validation gap. The `ac6265b` delta addresses it with fresh empty output enforcement and distinct explicit external packaging/CLI/Python/WASM target directories. I inspected that delta and found no further blocker within it.

The `517e269` delta restores the `is-binary-path` constraint, resolved version, and tarball URL to 2.1.0, retaining its integrity hash, and gives consumer `npm ci` a fresh cache beneath its fresh output directory. I inspected these changes and the matching release guidance. A committed-file structural execution confirmed the corrected transitive fields agree and package/lockfile root metadata remain version 3.0.0. I did not independently query registry integrity or execute cold-cache npm installation; the parent's reported successful cold-cache install is not counted here as my execution evidence.

## Actual execution evidence

1. `PYTHONDONTWRITEBYTECODE=1 python3 tests/test_review_tools.py` against the immutable core checkout: unittest reported 9 tests run, OK, with 1 Linux-only test skipped on macOS. Re-run log: `/tmp/independent-rc-review-tools.log`.
2. Compiled `src/target.rs` directly with rustc into external artifacts; no Cargo target directory was used. Harness source: `/tmp/independent-rc-target.rs`; executable: `/tmp/independent-rc-target`; additional directly compiled rlib: `/tmp/independent-rc-target.rlib`.
3. Compared the Rust validator with Python Fraction exact arithmetic for 15,005 fixed-seed generated and boundary decimal literals, including subnormal and maximum-finite cases: zero mismatches. Reproducible oracle source: `/tmp/independent-rc-oracle.py`; execution log: `/tmp/independent-rc-oracle.log`. The harness includes the immutable original checkout's `src/target.rs` by absolute path, so that checkout is required to recompile it.
4. Confirmed the original checkout HEAD was c94d781 and `git status --porcelain` was empty after review execution.
5. Executed committed-file JSON checks for final package/lockfile root version consistency and corrected `is-binary-path` fields; all assertions passed. This simple check's output was recorded in the agent tool transcript, not a separate log.

## Limits and remaining gates

This was targeted review, not exhaustive proof or a full repository audit. The reviewer did not run the complete Rust feature matrix, end-to-end installed-consumer pipeline, browser suite, Python wheel installation, or hosted CI. The standalone arithmetic oracle and Python tests do not establish those surfaces. The original core remains unchanged by the two reviewed follow-ups. Successful complete installed-consumer execution and hosted CI for the final SHA remain necessary before publication. No claim that these final-SHA gates have passed is made here.

## Final independent finding and repair review

After the initial no-additional-findings assessment, a targeted follow-up found a P2 contract gap in the previous candidate: `src/cli/args.rs` promised that match-all-digits requires matches to agree with target precision and `docs/SEARCH_MODEL.md` promised half-last-place maximum root error, without a PSLQ exception, while `src/cli/runtime.rs` retained fixed PSLQ residual tolerance 1e-10. The representable example `ries-rs 1.000000000000001 --match-all-digits --pslq --pslq-max-coeff 10` could therefore accept the x-1 relation despite its root distance exceeding the requested 5e-16. The fresh finding was established by source/documentation inspection; I did not independently execute that installed-CLI command.

I independently inspected the complete committed delta `42b90521e0808b467e58f9e4c4bbb33d4d0f1b3f` through `git show` read-only. It rejects match-all-digits in the PSLQ branch before the PSLQ header/results, after main's shared decimal target validation. It explicitly documents the unsupported combination in help, README, search-model/qualification documentation, and changelog. The CLI regression uses the valid representable counterexample and requires exit code 2, empty stdout, and the specific unsupported-combination diagnostic. The installed-consumer smoke includes the same valid target in its rejection cases.

No additional release-blocking findings within this final delta's inspected scope. The repair directly closes the reported P2 through explicit rejection rather than a false residual-to-root-distance promise. Numerical-search behavior is unchanged by this delta. This final repair review is source inspection only: I have not executed the new Rust regression or the final-SHA installed-consumer pipeline or hosted CI. Earlier actual 15,005-case oracle results and Python tool-test results retain exactly the original c94 source scope described above. All source checkouts remain unchanged by this reviewer.
