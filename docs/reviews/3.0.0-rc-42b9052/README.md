# 3.0.0 committed release-candidate evidence

Source commit: **`42b90521e0808b467e58f9e4c4bbb33d4d0f1b3f`**  
Source Git tree: `1b0ab177d1e4e651b0ec60ae4b203e0f381a5b4b`  
Review branch: `codex/3.0.0-rc-review` / [draft PR #18](https://github.com/maxwellsantoro/ries-rs/pull/18)

**Publication remains on hold.** No tag, merge, registry upload, or release was
performed. This evidence-only commit names the separate immutable source commit;
it does not claim to attest to its own containing commit.

## Boundary fixes

- Decimal validation precedes PSLQ and expression evaluation, including `--at`.
- Original decimal spelling is stored in manifests and replayed exactly, with
  the parsed f64 recorded separately. Exact replay requires identical root/error
  bits; tolerant comparison requires an explicit, reported absolute tolerance.
- Certificate arithmetic is conservatively bounded at 4096 numerator/denominator
  bits, including checks before multiplication and untrusted rational parsing.
- PSLQ rejects match-all-digits because its residual criterion does not establish
  the numerical search's last-digit root-distance bound.
- Installed-consumer builds use fresh separate Cargo targets and a fresh npm
  cache; the transitive npm resolution corrupted by broad version replacement
  was restored and verified without disabling integrity checks.

## Exact-source and artifact checks

All **165 source hashes** in local qualification and all
recorded hosted source hashes were checked against `git show 42b90521e0808b467e58f9e4c4bbb33d4d0f1b3f:<path>`.
The Cargo package's VCS metadata names this SHA, and packaged Rust sources match
those blobs. The local installed CLI hash equals the binary hash in qualification.
The receipt records hashes of the CLI, installed wheel, npm tarball and browser
WASM; the built-site browser test loads the npm-installed WASM.

Local qualification used a clean detached checkout. The hosted qualification job
writes untracked output under its checkout, so its global `working_tree_clean`
flag is false; its tracked diff is empty and every recorded source hash matches
the candidate. This is disclosed rather than presenting it as a clean-worktree
receipt. The installed-consumer jobs separately require clean checkouts.

## Validation

- [Hosted full CI](https://github.com/maxwellsantoro/ries-rs/actions/runs/43105978252): **success**, at the source SHA above; Linux, macOS,
  Windows, highprec, Clippy feature configurations, Python, WASM, browser, audit,
  independent qualification, and installed consumers all passed.
- [Hosted coverage](https://github.com/maxwellsantoro/ries-rs/actions/runs/43105900475): **success**, same source SHA.
- Local minimal-feature nextest: **564 passed, 31 skipped**.
- Local matcher corpus: **126 runs passed** across applicable modes and weights,
  including actual per-side budget checks. Hosted corpus: **42 runs passed**.
- Exact-decimal acceptance oracle: **119 cases passed**, explicitly including
  `9007199254740993`, true zero versus underflow, signed zero, huge exponents,
  trailing-zero precision, and the 4096-character resource limit.
- Packaged CLI installation, installed wheel tests (6), installed npm/Node search,
  npm WASM browser tests (13), and exact manifest replay with
  `--allow-unverified`: **passed locally and in hosted CI**.
- [Independent agent review](independent-review.md): no additional blockers within
  its stated scope after reviewing the final delta. It independently compared the
  unchanged decimal validator source against **15,005 exact Fraction oracle cases**
  with zero mismatches. It did not independently run the full feature matrix or
  hosted/artifact checks, and is not external human approval or symbolic proof.

The corpus runs record timing/resource observations but ran alongside other
validation work. They are conformance evidence, not an isolated performance
baseline or a universal completeness claim. Earlier development archives remain
historical evidence and are not substituted for this source-bound run.

## Receipts and archive

- [Machine-readable summary](summary.json)
- [Source binding](source-binding.json)
- [Local installed-consumer receipt](local-consumer-receipt.json)
- [Hosted installed-consumer receipt](hosted-consumer-receipt.json)
- [Hosted CI details](hosted-ci.json) / [coverage details](hosted-coverage.json)
- [Raw qualification, consumer logs, independent oracle and source snapshot](validation-evidence.tar.gz)

Archive SHA-256: `7ca278db5ea617373bfea76e7fc56542dec1505ea27cb64ea0ce34d24373ffcd`.

The archive preserves earlier failed receipts too: a stale same-version installed
CLI from a shared target directory, and the npm lockfile integrity failure. They
are labeled historical failures and were not accepted as validation of this source.
Hosted artifacts retain the actual packaged CLI, wheel and npm artifacts for the
workflow's configured retention period; no release publication is implied.
