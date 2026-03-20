# ries-rs — Comprehensive Code Review

**Reviewer:** Claude (Opus 4.6)
**Date:** 2026-03-19
**Commit range:** v1.1.1 (current HEAD)
**Scope:** Full codebase review via repomix export (~32K lines)

---

## Executive Summary

**Overall grade: A**

ries-rs is a production-grade Rust reimplementation of Robert Munafo's RIES inverse equation solver, shipping the same search engine through four runtime surfaces: CLI, Rust library, Python bindings (PyO3), and WebAssembly. The project demonstrates unusually high engineering discipline for a solo-maintainer repository — thorough documentation, multi-platform CI, tag-driven release automation to crates.io/PyPI/GitHub, Zenodo DOI integration, and property-based testing of core mathematical invariants.

The findings below are organized into three tiers: issues that affect correctness or behavioral consistency, issues that affect maintainability or developer experience, and suggestions for future improvement. None of the findings are blocking for the current v1.x release line.

---

## Tier 1 — Correctness & Behavioral Consistency

### 1.1 Search radius inconsistency between batch and streaming paths

**Severity:** Medium
**Location:** `src/search/db.rs` (batch) vs `src/search.rs` (streaming)

The two main search paths use different algorithms for computing the search radius — the window around each LHS value where RHS candidates are considered for Newton refinement.

**Batch path** (`ExprDatabase::find_matches_with_stats_and_context` in `db.rs`):

```rust
let min_search_radius = 0.5 * lhs.derivative.abs();
let search_radius = (pool.accept_error * lhs.derivative.abs()).max(min_search_radius);
```

**Streaming path** (`search_streaming_with_config` in `search.rs`):

```rust
let search_radius = calculate_adaptive_search_radius(
    lhs.derivative, lhs.expr.complexity(),
    pool.len(), search_config.max_matches, pool.best_error,
);
```

The streaming path uses the full adaptive radius with complexity scaling, pool fullness dampening, and exact-match tightening. The batch path uses a simpler formula that only considers `accept_error × derivative`. Since users don't control which path fires (it's determined by the 2M expression threshold), the same target and configuration can produce different match sets depending on whether the batch or streaming code path is triggered.

**Suggested resolution:** Extract the adaptive radius calculation into a shared function and use it in both paths. The batch path should call `calculate_adaptive_search_radius` with the same parameters the streaming path uses. If there's a performance reason the batch path uses the simpler formula, document the tradeoff explicitly.

---

### 1.2 Newton-Raphson always starts from target value

**Severity:** Low-Medium
**Location:** `src/search/db.rs`, `src/search.rs` — all `newton_raphson_with_constants` call sites

Every Newton-Raphson call uses `config.target` (or `search_config.target`) as the initial guess. The coarse linear estimate `target + x_delta` (where `x_delta = -val_diff / lhs.derivative`) is computed but only used for the pre-Newton error gate — it's never passed as the initial guess to Newton-Raphson itself.

For expressions where the LHS has a local extremum near the target, starting from the target rather than the linear estimate can cause Newton to diverge or converge to a different root. The original RIES uses the linear estimate as the starting point for refinement.

**Suggested resolution:** Pass the linearly estimated `x` (i.e., `config.target + x_delta`) as the initial guess to `newton_raphson_with_constants` instead of `config.target`. This should improve convergence for cases where the LHS slope changes sign near the target, and would bring behavior closer to the original RIES.

---

### 1.3 `pool.accept_error` tightening is monotonic and never relaxes

**Severity:** Low-Medium
**Location:** `src/pool.rs`, `TopKPool::try_insert`

The `accept_error` threshold only ever tightens (multiplied by `ACCEPT_ERROR_TIGHTEN_FACTOR = 0.9999` on each qualifying insertion). Once a few good matches tighten this threshold, it never relaxes — even if those matches are subsequently evicted from the pool and the pool drops well below capacity.

This creates a ratchet effect: matches found later in the search must meet a stricter bar than matches found early, even when the pool has room for more diverse results. At high complexity levels where the expression space is large, this could cause the pool to miss legitimate matches that would have been accepted if encountered earlier in the search.

**Suggested resolution:** Add a periodic relaxation step. For example, when the pool is below 50% capacity after an eviction cycle, relax `accept_error` by a small factor (e.g., `/ 0.999`). Alternatively, reset `accept_error` to the error of the current worst match in the pool whenever an eviction occurs, keeping the threshold anchored to the actual pool contents.

---

### 1.4 Python bindings docstring has incorrect expression counts

**Severity:** Low
**Location:** `ries-py/src/lib.rs`, `search()` docstring

The docstring claims:

```
Level 0 ≈ 89M expressions, Level 2 ≈ 11B, Level 5 ≈ 15T
```

The actual library-level formula (`level_to_complexity`) produces complexity bounds of `(10 + 4*L, 12 + 4*L)`, and the adaptive formula `2000 × 4^(2+level)` gives approximately: Level 0 ≈ 32K, Level 2 ≈ 512K, Level 5 ≈ 32M. The docstring numbers are off by orders of magnitude and would mislead API consumers about expected runtime and resource usage.

**Suggested resolution:** Replace with accurate numbers derived from the actual formula, or remove the expression count estimates and refer to the level guidelines table in `search.rs::level_to_complexity`.

---

### 1.5 `QUANTIZE_SCALE` defined in both `gen.rs` and `thresholds.rs`

**Severity:** Low
**Location:** `src/gen.rs` line ~25291, `src/thresholds.rs` line ~8585

`gen.rs` defines `const QUANTIZE_SCALE: f64 = 1e8;` as a module-private constant. `thresholds.rs` also defines `pub const QUANTIZE_SCALE: f64 = 1e8;`. The gen.rs version shadows the public one. If someone changes one but not the other, the quantization behavior diverges silently between expression generation (dedup) and any code using the thresholds module.

**Suggested resolution:** Remove the local definition in `gen.rs` and import `crate::thresholds::QUANTIZE_SCALE` instead. The gen.rs file already imports other constants from `thresholds`.

---

## Tier 2 — Maintainability & Developer Experience

### 2.1 Duplicated `build_gen_config` across three surfaces

**Severity:** Medium
**Location:** `src/wasm.rs`, `ries-py/src/lib.rs`, `src/cli/config_builder.rs`

The `build_gen_config` function is implemented independently in all three binding surfaces. The WASM and Python versions are nearly identical to each other but meaningfully different from the CLI version (which handles `-S`/`-N`/`-O` symbol filtering). If a new symbol slot, constant, or configuration field is added, three files need coordinated updates.

**Suggested resolution:** Extract a shared `build_gen_config_from_profile(max_lhs: u32, max_rhs: u32, profile: &Profile) -> GenConfig` function into the core library (e.g., in `src/gen.rs` or a new `src/config.rs`). The WASM and Python surfaces can call it directly. The CLI surface can call it and then apply its additional filtering passes on top.

---

### 2.2 Duplicated `From<Match>` conversion logic in WASM and Python

**Severity:** Medium
**Location:** `src/wasm.rs` (`WasmMatch::from`), `ries-py/src/lib.rs` (`PyMatch::from`)

Both conversion implementations compute identical derived fields: `solve_for_x` (via the analytical solver), `canonical_key`, `operator_count`, `tree_depth`, and `is_exact`. The logic is copy-pasted between the two files.

**Suggested resolution:** Create a shared intermediate struct in the core library:

```rust
pub struct MatchSummary {
    pub lhs_infix: String,
    pub rhs_infix: String,
    pub lhs_postfix: String,
    pub rhs_postfix: String,
    pub solve_for_x: Option<String>,
    pub solve_for_x_postfix: Option<String>,
    pub canonical_key: String,
    pub x_value: f64,
    pub error: f64,
    pub complexity: u32,
    pub operator_count: usize,
    pub tree_depth: usize,
    pub is_exact: bool,
}

impl From<Match> for MatchSummary { /* shared logic */ }
```

Both `WasmMatch` and `PyMatch` can then convert from `MatchSummary` with trivial field copies.

---

### 2.3 `#[allow(dead_code)]` on public API functions

**Severity:** Low
**Location:** `src/search.rs`, `src/pool.rs`, `src/report.rs`, `src/udf.rs`

Several core public API functions (`search()`, `search_with_stats()`, `search_streaming()`, `search_parallel()`, `is_valid()`, `is_empty()`, `stack_effect()`, `description()`, etc.) are annotated with `#[allow(dead_code)]`. These are the entry points library consumers will call. The annotation suggests they're not exercised by the CLI or internal callers, which makes sense, but the pattern is mildly alarming to a library user reading the source.

**Suggested resolution:** Write thin integration tests that call each public API function directly. This both suppresses the dead_code warning legitimately and validates the public surface. Alternatively, add a brief comment on each annotation explaining the intent (e.g., `// Public API — not called internally but used by library consumers`).

---

### 2.4 `SearchStats::print()` writes directly to stdout

**Severity:** Low
**Location:** `src/search.rs`, `SearchStats::print()`

The `print()` method writes directly to `stdout` with `println!()`, which breaks composability for library consumers who may want to redirect diagnostic output or format it differently.

**Suggested resolution:** Replace `print()` with a `Display` implementation or a `format_report(&self) -> String` method. The CLI caller can then `println!("{}", stats.format_report())` while library consumers can route the output wherever they need.

---

### 2.5 `ExprDatabase` doesn't use `TieredExprDatabase`

**Severity:** Low
**Location:** `src/search/db.rs`

The batch search path uses the flat `ExprDatabase`, while the streaming path uses `TieredExprDatabase`. The tiered database allows complexity-prioritized search (searching simpler RHS first), but the batch path doesn't benefit from this optimization. This creates another behavioral divergence between the two paths.

**Suggested resolution:** Migrate the batch `ExprDatabase` to also use the tiered structure, or document why the flat structure is preferred for the batch case (e.g., if the sorting + binary search on a contiguous array has better cache behavior than the tiered approach for small-to-medium expression sets).

---

### 2.6 `cdylib` crate type in default builds

**Severity:** Low
**Location:** `Cargo.toml`

The crate specifies `crate-type = ["cdylib", "rlib"]` globally. The `cdylib` is required for WASM builds but adds overhead to normal Rust builds. The inline comment acknowledges this and suggests `cargo build --bin ries-rs` to skip the cdylib link step, but this is easy for contributors to miss.

**Suggested resolution:** Investigate whether Cargo supports gating `cdylib` behind the `wasm` feature flag. If not, add a more prominent note in `CONTRIBUTING.md` about using `--bin ries-rs` for faster development builds.

---

## Tier 3 — Suggestions for Future Improvement

### 3.1 No timeout or cancellation mechanism

**Location:** All search functions in `src/search.rs`

None of the search functions accept a timeout duration or cancellation token. At high complexity levels, searches can run for minutes. The WASM binding has `MAX_API_LEVEL = 5` as a safety valve, but the Rust library and CLI have no equivalent.

**Suggested resolution:** Add an optional `timeout: Option<Duration>` parameter to the search config, or accept a `CancellationToken` (e.g., an `Arc<AtomicBool>`). The streaming callback architecture (`StreamingCallbacks`) already supports early termination by returning `false` — document this as a supported cancellation mechanism, and wire the timeout into it.

---

### 3.2 Streaming search doesn't truly early-exit

**Location:** `src/search.rs`, `search_streaming_with_config`

The streaming search remembers `best_stop_match` and splices it to the front of results, but always processes the entire LHS expression space even if an exact match is found in the first 1% of expressions. For very large search spaces (the exact scenario where streaming is triggered), this is a significant waste.

**Suggested resolution:** If the generator could yield expressions in complexity order (or approximately so), the streaming path could perform true early exit. Alternatively, a two-pass approach where the first pass generates LHS expressions into a buffer sorted by complexity, then the second pass streams through it in order, would enable principled early termination.

---

### 3.3 Adaptive search re-generates all expressions each iteration

**Location:** `src/search.rs`, `search_adaptive`

The adaptive search loop re-generates all expressions from scratch on each iteration as the complexity bounds grow. The comment correctly notes the geometric series overhead is ≤33%, but for high levels where generation dominates, caching previously-generated expressions and only generating the delta for the new complexity tier would be a meaningful optimization.

**Suggested resolution:** Maintain a running `HashMap` that persists across iterations and only generate expressions for the new complexity range `(old_bound, new_bound]` on each iteration. The dedup logic already handles keeping the simplest equivalent expression per key.

---

### 3.4 `get_constant_candidates()` allocates on every call

**Location:** `src/fast_match.rs`

The fast-match path creates a `Vec<FastCandidate>` of ~60+ static candidates on every invocation. Since all candidates are compile-time data, this allocation is unnecessary.

**Suggested resolution:** Use `std::sync::LazyLock` (stable since Rust 1.80) or a `const` array to avoid per-call allocation.

---

### 3.5 Expression `pop()` recomputes `contains_x` via linear scan

**Location:** `src/expr.rs`, `Expression::pop()` and `pop_with_table()`

When `pop()` removes an `X` symbol, it re-scans the entire expression to check if any `X` remains. For long expressions being built incrementally during generation, this creates O(n) work per pop.

**Suggested resolution:** Replace `contains_x: bool` with `x_count: u32`. Increment on push, decrement on pop. `contains_x()` becomes `self.x_count > 0`.

---

### 3.6 `canonical_expression_key` has limited commutativity handling

**Location:** `src/solver.rs`

The canonicalization sorts children of `Add` and `Mul` for commutativity but doesn't handle associativity (`(a + b) + c` vs `a + (b + c)`) or derived identities (`x - 1` vs `x + (-1)`). This limits deduplication power — some structurally equivalent equations appear as distinct matches.

**Suggested resolution:** Document this as a known limitation. Full canonicalization (flattening associative operators, normalizing subtraction to addition of negation, etc.) is a significant undertaking and may not be worth the complexity for the improvement in dedup quality. The original RIES had similar limitations.

---

### 3.7 Web UI uses Tailwind CDN runtime

**Location:** `web/index.html`

The web UI loads `tailwindcdn.js` from a vendored local copy. The Tailwind CDN runtime generates all utility classes at page load time, which is slower than a purged production build that only includes used classes.

**Suggested resolution:** Add a Tailwind build step to the web build pipeline (`scripts/build_web_site.sh`) that produces a minimal CSS file. This would improve page load performance and eliminate the ~300KB CDN script.

---

### 3.8 Web UI accessibility

**Location:** `web/index.html`

The web UI uses color-only indicators (emerald for exact matches, yellow for close), hover effects that require a pointer device, and KaTeX-rendered math that screen readers may not handle well. For a project aimed at educators and researchers, accessibility matters.

**Suggested resolution:** Add `aria-label` attributes to result cards, ensure keyboard navigability for all interactive elements, and provide text alternatives for color-coded status indicators (e.g., a "(exact)" text label alongside the color).

---

### 3.9 `.repo` homepage inconsistency

**Location:** `.repo`

The `.repo` file has `homepage = "https://github.com/maxwellsantoro/ries-rs"`, but `Cargo.toml` and the README both designate `https://maxwellsantoro.com/projects/ries-rs` as the canonical homepage.

**Suggested resolution:** Update `.repo` to `homepage = "https://maxwellsantoro.com/projects/ries-rs"` for consistency.

---

### 3.10 `.repo` relations field is empty

**Location:** `.repo`, `[relations]`

The `references = []` field is empty despite ries-rs being an explicit reimplementation of Robert Munafo's RIES and tracking compatibility with the `clsn/ries` fork.

**Suggested resolution:** Populate with upstream references:

```toml
[relations]
references = [
    { url = "https://mrob.com/pub/ries/", relation = "upstream" },
    { url = "https://github.com/clsn/ries", relation = "related" },
]
```

This would also serve as a dogfooding exercise for dotrepo's relations model.

---

### 3.11 Missing `CITATION.cff` in repomix export

**Location:** Repository root (absent from export)

The README references `CITATION.cff` as the canonical citation source, but it wasn't included in the repomix pack. This may be a repomix configuration issue (`.repomixignore`), but worth verifying the file is present, up-to-date, and not accidentally excluded from the crate package.

**Suggested resolution:** Verify `CITATION.cff` is in the repo and not listed in `.repomixignore` or `Cargo.toml`'s `exclude` list. Add it to the release integrity check if not already covered.

---

### 3.12 `PoolEntry::rank_key` IEEE 754 bit-pattern trick

**Location:** `src/pool.rs`, `PoolEntry::new`

The rank key uses `error_abs.to_bits() as i64` to create a sortable integer from a float. This relies on the fact that positive f64 bit patterns fit in i64 (the sign bit is 0 for positive values, so the cast is safe). The NaN/Infinity special cases are handled correctly. However, the invariant is subtle enough that a future reader might worry about it.

**Suggested resolution:** Add a one-line comment: `// Safe: positive f64 bit patterns have sign bit 0, so to_bits() < i64::MAX`.

---

## What's Working Well

For completeness, here's a summary of the project's strongest aspects:

- **Architecture:** Clean layered separation between core engine, CLI, reporting/analysis, and bindings. All four runtime surfaces share the same search pipeline without branching.
- **Performance engineering:** `SmallVec<[Symbol; 21]>` for inline expression storage, `EvalWorkspace` for hot-path evaluation, `TopKPool` with BinaryHeap eviction, automatic batch→streaming fallback at the 2M threshold.
- **Forward-mode AD:** Dual-number automatic differentiation tracking `(value, derivative)` through every operation, with careful edge-case handling.
- **Testing:** CLI regression tests, integration tests, property-based tests (proptest) verifying derivative correctness via finite differences and calculus rules, WASM tests, optional parity checks against the original C RIES binary.
- **Documentation:** `SEARCH_MODEL.md` formalizes the computational model, `ARCHITECTURE.md` maps the codebase, `PARITY_STATUS.md` tracks compatibility, each binding surface has its own guide.
- **Release engineering:** Tag-driven CI publishes to crates.io, PyPI, and GitHub releases. Integrity check script validates version sync across seven files. Go/no-go checklist with per-surface smoke checks.
- **dotrepo integration:** Using `.repo` metadata in CI with validation and generated surface checks.

---

## Priority Matrix

| # | Finding | Severity | Effort | Suggested Timeline |
|---|---------|----------|--------|-------------------|
| 1.1 | Search radius inconsistency | Medium | Low | Next patch |
| 1.2 | Newton initial guess | Low-Med | Low | Next patch |
| 1.3 | Pool accept_error ratchet | Low-Med | Low | Next minor |
| 1.4 | Python docstring numbers | Low | Trivial | Next patch |
| 1.5 | Duplicate QUANTIZE_SCALE | Low | Trivial | Next patch |
| 2.1 | Duplicate build_gen_config | Medium | Medium | Next minor |
| 2.2 | Duplicate From\<Match\> | Medium | Medium | Next minor |
| 2.3 | dead_code annotations | Low | Low | Next minor |
| 2.4 | SearchStats::print stdout | Low | Low | Next minor |
| 2.5 | ExprDatabase vs Tiered | Low | Medium | v2.0 |
| 2.6 | cdylib in default builds | Low | Low | Next minor |
| 3.1 | No timeout/cancellation | — | Medium | v2.0 |
| 3.2 | Streaming early-exit | — | High | v2.0 |
| 3.3 | Adaptive re-generation | — | Medium | v2.0 |
| 3.4 | fast_match allocation | — | Trivial | Anytime |
| 3.5 | Expression pop() scan | — | Trivial | Anytime |
| 3.6 | Canonical key limits | — | Document | Anytime |
| 3.7 | Tailwind CDN runtime | — | Low | Next minor |
| 3.8 | Web UI accessibility | — | Medium | Next minor |
| 3.9 | .repo homepage | — | Trivial | Next patch |
| 3.10 | .repo relations | — | Trivial | Next patch |
| 3.11 | CITATION.cff in export | — | Trivial | Verify |
| 3.12 | PoolEntry bit trick | — | Trivial | Anytime |

---

*End of review.*
