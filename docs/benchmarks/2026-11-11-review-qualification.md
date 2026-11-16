# November 11, 2026 candidate qualification

The [follow-up review](2026-11-11-followup-review.md) supersedes this candidate's boundary contracts and test counts. This earlier performance record is retained.

This is local development evidence for an unpublished 3.0.0 candidate, not a
release qualification for a tagged commit. The full release CI must run at the
final tag. The baseline was captured after agent build/test jobs finished; no
system-wide load isolation or universal speedup is claimed.

## Correctness observations

- 120 runs: ten fixed cases × four requested modes × three repeats.
- Every run agreed on ordered top-8 equation identities, roots, errors, and exact classifications.
- No missing known answers in the four polynomial fixtures; the threshold tie kept `x = b` first.
- Four historical first-equation polynomial checks passed; equivalent spellings/cost differences remain recorded.
- Exact rational root certificates were available for 216 result occurrences; 228 were explicitly unavailable. Repeats are not independent discoveries.
- The seeded small-grammar Rust oracle varies grammar, weights, and budgets independently of production evaluation and Newton refinement.

## Level-3 resource observations

Target `3.301`, CLI level 3, top 8, maximum root distance 0.01, default full grammar.
Times are median process wall time over three runs, including startup. RSS is the
CLI process peak, not a configured limit.

| Requested mode | Effective matcher | Median wall time (s) | Median peak RSS (MiB) |
|---|---|---:|---:|
| serial | streaming | 4.106 | 216.8 |
| streaming | streaming | 3.779 | 176.0 |
| parallel | streaming | 4.148 | 176.0 |
| turbo | turbo | 1.929 | 473.4 |

The widest observed window in the streaming paths was 508,342 RHS entries for
LHS `xxE^T`, with derivative about 7.06e17. The first turbo repeat had a window
of 508,249 entries for the same LHS. Candidate counts differ across matcher
implementations despite equal retained top-8 results on this corpus. No narrower
candidate gate was introduced.

## Validation recorded

- Default nextest: 565 passed, 32 skipped.
- Highprec nextest: 589 passed, 32 skipped.
- Clippy default/no-default/highprec and formatting: passed.
- Browser: 12 passed; desktop 1280×720 and mobile 390×844, static CSS, live search, text/cards, console health, and no KaTeX errors in generated cards.
- Python: 2 Rust helper tests and 6 installed-wheel integration tests passed.
- Tool checks: 8 passed and 1 Linux-only skip on macOS; all 7 dedicated tool tests passed in Linux, including actual allocation-limit and timeout cases.
- WASM browser/Node/bundler builds, package-entry checks, WASM tests, and 7 doctests passed.
- Verified Cargo package and Python sdist builds passed.
- Release metadata integrity passed at 3.0.0. Workflow YAML and publication dependency paths were checked locally; hosted GitHub Actions were not run.

## Evidence

- [Machine-readable summary](artifacts/2026-11-11-review-summary.json)
- [Raw JSON, manifests, certificates, baseline source snapshot, and validation logs](artifacts/2026-11-11-review-candidate.tar.gz)
- Archive SHA-256: `768e3b13c7d86bf4258b3b0ac531c6b07895d7180d87ed9f0937cc2f3e1a035b`
- Release binary SHA-256: `bebbb54423dc4506c728bf17ab6f48ee68b6383cb030c03435d21b83c18eb33f`

The archive retains all source files named in the baseline source-hash map. A
later strict-replay regression and whitelist addition are included in the final
tracked patch; they do not alter the measured search engine. Run manifests from
the corpus use an explicit empty profile; reproduce those searches with the corpus
runner rather than passing those manifests to the self-contained replay tool.

See [QUALIFICATION.md](../QUALIFICATION.md) for commands, certificate trust
boundaries, and the Linux address-space supervisor.
