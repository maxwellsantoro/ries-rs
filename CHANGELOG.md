# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [3.0.0] - 2026-11-12

Release decision: these changes target **3.0.0**, not a 2.x release, because
`ExpressionConstraintOptions` gains a required field and the stop contract
changes observable ordering. Package metadata is 3.0.0. Publishing still
follows `RELEASING.md`; this heading is the version note, not a tag.

### Fixed
- Exact labels no longer trust an f64 residual when a rounded reciprocal or trigonometric argument manufactured it. Candidates that agree in f64 are re-evaluated in double-double at the typed target and stay exact only when the real sides still agree to eight ulps. This drops `cospi(1/x) = cospi(1/8)` at `±1e-15`, `cospi(pi+x) = -cospi(1/8)` at `1e15`, and `e^sinpi(1/x) = 1` at `1e-12`. `2*x = 5`, `x = pi`, `x = sqrt(2)`, integer `sinpi`, and `x = ln(1)` stay exact.
- Parity ranking of exact rows is complexity-first, and each right-hand value keeps its simplest spelling. The website call `search(target, {level: 3, maxMatches: 50, rankingMode: 'parity'})` therefore leads with plain equations such as `2*x = 5`, `x = 5/2`, `x = pi`, and `x = 0`.
- Subnormal targets are no longer emptied by the near-zero left-hand prune, which used to drop `x` itself.
- `--stop-below` rejects NaN, infinities, and negative thresholds.
- Release builds use Rust 1.99.1, matching CI. The Windows archive is `ries-rs-windows-x86_64.zip`.
- Reject PSLQ with match-all-digits instead of silently substituting residual tolerance for a root-distance contract.
- Validate decimal targets before PSLQ/special-mode dispatch.
- Preserve original target spelling in manifests and replay arguments; exact replay now requires identical root/error bits, with explicit opt-in tolerance.
- Bound experimental certificate arithmetic before expensive integer operations.
- Reject precision-sensitive decimal targets whose conversion loses more than the last-digit tolerance; reject nonzero decimal underflow in CLI/WASM.
- Report actual adaptive bounds separately from requested bounds and reject ignored mode/memory combinations.
- Fail unavailable explicit verification requests unless `--allow-unverified` is supplied; retain unavailable JSON on explicit fallback.
- Give the Python extension a distinct Cargo artifact identity while preserving `import ries_rs`; document the actual comparator.
- Preserve the engine's designated stop winner through deterministic and turbo CLI postprocessing.
- Report unavailable high-precision verification with `is_verified = false`.
- Reject unsupported nested user-function calls during definition parsing.
- Group chained exponents in browser LaTeX output to preserve associativity and avoid KaTeX double-superscript errors.
- Synchronize complexity weights, level examples, and historical accuracy CI documentation with the implementation.

### Added
- Opt-in `--strict-config` validation, replay preservation, and original target literals in CLI JSON.
- Candidate-window histograms, a fixed cross-matcher qualification corpus, and a restricted historical equation differential with retained CI evidence.
- Experimental exact rational polynomial root certificates and a Linux address-space/time supervisor.
- Built browser CI coverage, mobile smoke coverage, compiled Tailwind CSS, and a full-CI release gate at the tagged commit.

### Changed
- Search now preserves structurally distinct left-hand sides by default. The
  same search can return a different first equation, ordered result list, or
  solved root. `--lhs-value-dedup` (Rust: `SearchConfig::lhs_value_dedup`) opts
  materialized searches into the former value-and-derivative heuristic. It can
  save matching work. Streaming always processes the generated LHS stream.
- CLI JSON and run manifests record the actual matcher, the requested LHS
  deduplication setting, and whether it was applied. The JSON `streaming` field
  now reflects the matcher used, including automatic fallback.
- `stop_at_exact` and `stop_below` (including `--classic`) follow one stop
  contract in every matcher. The simplest qualifying match bounds result
  complexity; results are the matches within that bound, ranked with a fresh
  pool and led by that match. Batch matching used to return the first
  qualifying match it scanned: for pi, `x = (-sqrt(pi))^2` (54) instead of
  `x = pi` (29). Classic output can differ. For example, 2.5 now returns
  `2 x = 5` (49) before `x = 5/2` (50).
- The CLI uses a fast match only when it provably ranks first: an exact
  `x = <atom>` below `non_atomic_equation_complexity_floor` that is the simplest
  atom meeting either stop condition (`stop_below` included), chosen from every
  enabled atom with `compare_matches`. Other targets run the search, which
  finds simpler equations such as `x = 1/2` spelled `2r` (35), not `1 2 /`
  (43). Classic searches that used to take the fast path can take longer.
- **Breaking:** `ExpressionConstraintOptions` has a new field,
  `user_function_constraints`. Struct literals without `..Default::default()`
  must add it.

### Added
- `UserFunctionConstraints`, `find_certified_fast_match`,
  `non_atomic_equation_complexity_floor`, `Symbol::ALL`,
  `Symbol::to_postfix_char`, and `Symbol::from_postfix_char`.

### Fixed
- Library RHS symbol filters are resolved before generation and rewrite pruning,
  so a filtered-out `sqrt` cannot suppress an admissible second-root spelling.
- Sine/cosine matches reject RHS values outside [-1, 1] before residual or Newton
  acceptance; a loose residual near an oscillatory maximum cannot create a root.

- RHS deduplication keeps distinct floating-point values, even when close
  values used to share a bucket. For bit-identical values, search selects a
  representative using the requested ranking after applying RHS symbol filters.
  Under a stop condition it keeps the simplest spelling, so the stop bound
  reflects it.
- Postfix output for user constants and functions parses back.
  `Expression::parse` decodes characters, not UTF-8 bytes. Symbol options no
  longer truncate a character above U+00FF onto an ASCII symbol (U+0170 was
  read as `p`), and `-S`/`-N` no longer split multibyte characters into bytes
  that alias user symbol slots.
- Rewrite pruning (`2 b v` as `sqrt(b)`, `log_e` as `ln`, `a+a` as `2*a`) fires
  only when the replacement is enabled on that side, no heavier, within its
  count limit, and not itself pruned. `-S x2v` now finds `x = 22v` for sqrt(2).
  `1/sqrt(ln a)` and `1/(ln a)^2` are no longer lost to a chain with the
  `1/ln(a)` heuristic.
- `(a^2)^2` and `sqrt(sqrt(a))` are pruned only when `^` or `v` and `4` are
  enabled, so restricted symbol sets can still express fourth powers and roots.
- Structural constraints classify user functions from their bodies, so
  `sinpi(sq(2))` passes `--rational-trig-args`, and a function whose body applies
  `sinpi` counts toward `--max-trig-cycles` and obeys the trig restriction.

## [2.1.0] - 2026-11-03

### Changed
- Library, Python, and WebAssembly search levels now use `40 + 2*level` on each side. Level 3 is 46/46 and level 5 is 50/50. The previous `10 + 4*level` / `12 + 4*level` scale could not express `2*x = 5`, `x = 5/2`, or `x = 6*7` even at level 5. The CLI `-l` scale is unchanged. `MAX_API_LEVEL` stays 5. The same programmatic level now changes coverage, latency, and ordering; callers relying on the old budgets should recalibrate.

### Fixed
- Searching for 0 no longer drops every left-hand side under `1e-4`. `x` is kept, so the exact equation `x = ln(1)` is found. `1-1` is the same value, but right-hand-side dedupe keeps `ln(1)` because it is cheaper (23 vs 25). Other targets still prune near-zero sides.

## [2.0.3] - 2026-11-03

### Fixed
- Solved equations now use the real inverse branch nearest the reported root.
  Sine, cosine, tangent, and even roots no longer print the principal value
  when the search found a different real branch. Odd powers still use their
  single real root.
- Large period shifts work. Any finite integer, including magnitudes past
  `2^63`, is a valid shift. If that shift cannot be represented, the solved
  form is omitted.
- `sinpi`, `cospi`, and `tanpi` use the correct parity at large magnitudes.
  Finite values at `2^52` and above are integers, and finite values at `2^53`
  and above are even, so `tanpi` of `2^62` is 0 rather than a pole.
- An exact match now requires both sides to agree at the typed target, with
  the solved `x` within a few ulps of that target. A nearby root is no longer
  labeled exact.
- Flushed `x` terms are skipped at any magnitude. When the `x` contribution
  has disappeared into a constant, and a probe shows the expression is not a
  true identity, that candidate is left out.
- Newton keeps a refined root only when the residual is small. A tiny step
  that leaves the two sides apart is rejected. Steep roots with a small
  relative residual are still kept.
- Clippy is clean.

### Changed
- Coverage now runs on LLVM. The coverage job no longer uses tarpaulin's
  ptrace engine, which crashed while dropping an expression after a passing
  manifest test.
- PyO3 was upgraded to 0.30.1. The Python API is unchanged.

### Security
- Updated `crossbeam-epoch`, `anyhow`, `rand`, and `pyo3`. These updates cover
  RUSTSEC-2026-0204, RUSTSEC-2026-0190, RUSTSEC-2026-0097, RUSTSEC-2026-0177,
  and RUSTSEC-2025-0020.

### Compatibility
- Python 3.8 support is dropped. The Python package now requires Python 3.9 or
  newer (`abi3-py39`) because PyO3 0.30 dropped 3.8.

## [2.0.2] - 2026-06-27

### Fixed
- Turbo now mirrors serial generation semantics before parallel matching:
  small batch searches use the same LHS deduplication as serial, while
  streaming-sized searches preserve the raw LHS stream. This fixes a
  level-3 rank-one parity miss where `--turbo` could discard the serial best
  match for `2.506314` before matching.
- Added regressions covering both turbo's small batch-generation parity path
  and the expensive level-3 flat-exact `2.506314` case.

### Changed
- Documented the refined turbo contract: canonical generation semantics plus
  parallel match/Newton work, with the lower-ranked tail still allowed to differ
  from serial.

## [2.0.1] - 2026-06-27

### Fixed
- Turbo and parallel-generation searches now fall back to the canonical serial
  pipeline for order-dependent `--stop-at-exact` and `--stop-below` searches,
  preventing per-worker early exits from violating rank-one parity
- Turbo CLI results are canonically sorted before display so the documented
  best match is always presented at rank one

## [2.0.0] - 2026-06-26

### Added
- Turbo mode (`--turbo`) parallelizes matching and Newton refinement across
  available cores while preserving the same single best match as serial search
- Shared `MatchSummary` conversion for consistent Rust, Python, and WASM match
  representations
- Widest candidate-window diagnostics in JSON output and search stats, recording the
  LHS expression that produced the maximum RHS scan window
- Level-3 benchmark baseline artifacts dated 2026-03-20
- Supported Rust-side and end-to-end Python binding test workflows using `uv`

### Changed
- **Breaking:** `SearchStats::record_candidate_window` now takes the LHS
  `EvaluatedExpr` plus window width (was width only) so widest-window diagnostics
  can be attributed to a specific expression
- Python wheels use the CPython stable ABI for Python 3.8 and newer instead of
  being tied to the single interpreter version used to build them
- Adaptive search radius is capped by the pool strict-gate coarse-error envelope,
  matching `would_accept_strict` value-space bounds; batch and streaming paths now
  share the same radius logic
- Parallel generation OOM preflight uses count-only streaming instead of cloning
  full expression sets, reducing peak RSS during large runs
- `sinpi`/`cospi`/`tanpi` at default π scale snap arguments within `1e-12` of
  integers or half-integers to exact values and derivatives, removing pathological
  near-singular artifacts (e.g. `1Sxn^S`) but also shifting which equations rank
  highly; refreshed level-3 baselines show fewer Newton calls and a lower Newton
  success rate (~92% → ~73%) alongside far fewer candidate pairs scanned
  (~66M → ~8M)
- Optional widest-window JSON fields are omitted when no window has been recorded
  (`#[serde(skip_serializing_if = "Option::is_none")]`)
- Python and WASM APIs reject non-finite targets consistently and document their
  bounded level scale separately from the CLI complexity scale

### Fixed
- PSLQ relation detection, duplicate/scalar relation handling, and rational
  fast-match classification
- Adaptive target-count exponent overflow and profile parsing/merge error handling
- Adaptive-growth behavior and classic streaming result ordering
- Expression deduplication during generation no longer clones when replacing a
  duplicate with a simpler expression in the same quantized bucket
- Trig argument snapping guards `f64`→`i64` conversion against out-of-range values

## [1.1.1] - 2026-03-18

### Added
- A direct live-demo link in the README and on the GitHub repository homepage

### Changed
- Refreshed GitHub Actions workflow dependencies to current Node 24-compatible
  releases to remove deprecation warnings from CI and release automation
- Updated public-facing README copy to make the landing page and standalone demo
  entrypoints explicit

## [1.1.0] - 2026-03-18

### Added
- Automated release-integrity checks for cross-package version sync, canonical
  homepage metadata, release notes presence, and README image assets
- Public-facing v1.1.0 release notes centered on the canonical landing page and
  live demo handoff

### Changed
- Pointed package and citation homepage metadata at the canonical project page
  on `maxwellsantoro.com`
- Clarified release ownership in `RELEASING.md`: this repo verifies release
  artifacts, while the website repo verifies the deployed demo experience
- Kept the crate-level rustdoc command-line example contiguous by moving the
  test-only crate attribute out of the middle of the docs

### Fixed
- Aligned Zenodo/DOI wording across the README, release notes, and release
  process docs so the repo no longer overclaims archival status
- Added an automated guard for the README screenshot asset referenced at the top
  of the project page

## [1.0.1] - 2026-03-16

### Changed
- Automated the tag-driven release workflow to publish to crates.io and PyPI
  before creating the GitHub release
- Added Python source distribution (`sdist`) packaging to the release pipeline
- Made release publish steps rerun-safe when a tagged version is already present
  on crates.io or PyPI

### Fixed
- Resolved `maturin sdist` packaging for `ries-py` by using a package-local
  PyPI readme and explicit Python package metadata

## [1.0.0] - 2026-03-14

### Added
- PSLQ integer relation detection (`--pslq`, `--pslq-extended`, `--pslq-max-coeff`)
- WebAssembly bindings for browser and Node.js usage
- Python bindings with PyO3/maturin
- High-precision verification using arbitrary precision arithmetic
- Stability ladder for impostor detection
- Domain presets for targeted searches (analytic-nt, physics, etc.)
- Run manifest for reproducibility (`--emit-manifest`)
- Deterministic mode for reproducible results (`--deterministic`)
- CITATION.cff for academic citation

## [0.1.0] - 2026-02-18

### Added
- Initial release
- Core equation search with parallel execution
- Automatic differentiation for Newton-Raphson refinement
- User-defined constants and functions via profiles
- Multiple output formats (default, pretty, Mathematica, SymPy)
- Complexity scoring for equation ranking
- Classic mode for compatibility with original RIES
- CLI with comprehensive options

### References
- Based on [RIES by Robert Munafo](https://mrob.com/pub/ries/)
- Compatible with [clsn/ries fork](https://github.com/clsn/ries)
