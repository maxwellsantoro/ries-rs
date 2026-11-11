# November 11 follow-up review evidence

This uncommitted, unpublished 3.0.0 candidate supersedes the earlier candidate's
decimal, adaptive-bound, precision-fallback, and Python-artifact contracts. The
prior archive remains historical evidence. Hosted release CI must still run at
the final tagged commit; no local run proves universal search completeness.

## Reproduced boundaries and fixes

- The decimal `1.000000000000000000000000000001` with match-all-digits is rejected before search. Nonzero decimal underflow such as `1e-400` is rejected even without that flag. Exact representable values with long zero padding are accepted.
- An independent Python `Fraction`/`Decimal` oracle passed all 110 hand-picked and seeded cases against the release CLI. The Rust check uses exact decimal integer comparisons, not f64 subtraction or an optional highprec dependency.
- The original adaptive reproducer reports requested bounds 35/35 and actual bounds 52/52; its highest equation complexity is 104. Every returned side is checked against actual limits.
- Adaptive and one-sided searches reject streaming, turbo, and memory-hint combinations that would otherwise be ignored.
- Explicit precision requests fail when unavailable unless `--allow-unverified` is supplied. Fallback results report unavailable and false `is_verified`; self-contained manifest replay accepts and reproduces the explicit fallback flag.
- Python's Cargo library target is `ries_rs_py`; the wheel still imports as `ries_rs`. Wheel construction and installed tests passed without the former output filename collision warning. The Python documentation now describes its complexity-ranking comparator.
- A second stop-order fixture covers a cheaper nonqualifying user constant and a more expensive qualifying constant in the same error bucket.

## Validation

| Surface | Result |
|---|---|
| Default Rust nextest | 571 passed, 32 skipped |
| No-default-features nextest | 560 passed, 31 skipped |
| Highprec nextest | 595 passed, 32 skipped |
| Clippy default / no-default / highprec; formatting | passed |
| Real-browser suite, including the worker decimal rejection/recovery flow | 13 passed |
| WASM tests / doctests | 13 / 7 passed |
| Python installed wheel / Rust helper tests | 6 / 2 passed |
| Cargo package verification, Python sdist, release integrity | passed |
| Fixed corpus, including applicable adaptive modes | 126 runs passed |
| Independent exact decimal oracle | 110 cases passed |

The fixed corpus checks ordered equations, root errors, numerical exact
classifications, grammar membership, and each side's complexity against effective
symbol weights and reported bounds. The adaptive case compares its supported
serial and parallel paths. Four historical polynomial differential cases passed.

## Retained evidence

- [Machine-readable summary](artifacts/2026-11-11-followup-summary.json)
- [Raw outputs, manifests, certificates, source snapshot and validation logs](artifacts/2026-11-11-followup-evidence.tar.gz)
- Archive SHA-256: `d11da0b30ecb3d8e7604f76ba47b7d45460406b3199bb3d6fe238b536038ca40`
- Release CLI SHA-256: `dcd06d58b6d57c4e305afb33194e454353344b0dee5620c5a9fc8b634b3d28d0`

The archive retains 159 source files checked against the corpus run's
source hashes, plus Python packaging and replay-tool sources. It records a local
candidate rather than a published release or complete build attestation. Browser
checks used Playwright at 1280×720 and 390×844; the Browser plugin was unavailable.
