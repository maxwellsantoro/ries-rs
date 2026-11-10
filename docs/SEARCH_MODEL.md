# RIES-RS Search Model

This document defines the core computational model implemented by `ries-rs`.

Goal: make the search behavior explicit, reproducible, and reviewable.

## Scope

This is the model for the core RIES-style equation search engine:

- expression representation (postfix / RPN)
- well-formedness rules
- complexity scoring
- match ranking
- deterministic ordering

This document does not define:

- report-mode category heuristics (`docs/COMPLEXITY.md` + `src/report.rs`, `src/metrics.rs`)
- PSLQ mode
- future experimental search branches

## Expression Representation

Expressions are stored in postfix notation (Reverse Polish Notation) as a sequence of symbols.

- Implementation type: `Expression` in `src/expr.rs`
- Symbol definitions and default weights: `src/symbol.rs`
- Runtime symbol-table overrides (weights/names): `src/symbol_table.rs`

Examples:

- `52/` -> `5/2`
- `xT` -> `tanpi(x)`
- `xCr` -> `1/cospi(x)`

## Symbol Classes (Arity)

Each symbol has a fixed arity category (`Seft` in code):

- `A`: atom / constant / variable (`0`-ary push)
- `B`: unary operator (`1`-input, `1`-output)
- `C`: binary operator (`2`-input, `1`-output)

The search model is defined over the active symbol set after applying CLI/profile filters (`-S`, `-N`, presets, user constants/functions, etc.).

## Well-Formedness Rule (Postfix Grammar)

`ries-rs` generates and evaluates only well-formed postfix expressions. Operationally:

1. Start with stack depth `d = 0`.
2. For each symbol:
   - atom (`A`): `d := d + 1`
   - unary (`B`): requires `d >= 1`, then `d := d`
   - binary (`C`): requires `d >= 2`, then `d := d - 1`
3. A complete expression is valid iff final depth is exactly `1`.

Equivalent abstract grammar (stack-discipline form):

- atoms are terminals
- unary nodes apply to one valid subexpression
- binary nodes apply to two valid subexpressions

In infix form, parentheses follow the precedence and associativity rules in `src/expr.rs`. Power (`^`, including printed `^2` and `e^`) is right-associative and binds tighter than unary minus. Unary minus binds tighter than multiplication, division, and reciprocal (`1/`). Addition and subtraction are left-associative and bind least tightly. A subexpression is parenthesized when reprinting it at the same precedence would change the value (`(a^b)^c`, `a-(b-c)`, `1/(1/x)`, `-(1/(1/(-phi)))`). Parsing that infix with the same rules evaluates to the postfix value.

Solve-for-x display uses `solve_for_x_at` and the matched root. Multi-valued inverses emit the real branch nearest that root: `±√y` for squares and even integer powers (odd powers keep their single real root), `asin(y)/π + 2k` or `1 - asin(y)/π + 2k` for sine, `2k ± acos(y)/π` for cosine, and `atan(y)/π + k` for tangent. Integer shifts print as decimal literals and evaluate to that exact integer for every finite integer `f64`, including magnitudes past 2^63. The function returns nothing when the hint is not finite, the inverse is not real, or the branch cannot be represented exactly. Text `-s` (report and classic) and `--format` use this same closed form. `solve_for_x_rhs_expression` remains the principal-branch inverse for callers that do not have a root.

For the default trig scale, the evaluator reduces the argument before
multiplying by π and handles representable integers and half-integers exactly.
`--trig-argument-scale` changes the evaluation and the displayed infix
equations. `solve_for_x_at_with_context` uses the same scale and custom
symbols as the search when choosing and checking the inverse branch.

## Search Space Partition

RIES-style search builds equations by matching:

- LHS expressions containing `x`
- RHS expressions that are constant-only

The engine generates candidate expressions up to complexity limits, evaluates them numerically, and then tests LHS/RHS pairs with Newton refinement (unless disabled).

Search implementations:

- sequential: `search_with_stats_and_config`
- parallel: `search_parallel_with_stats_and_config` (parallel generation, serial batch matching/Newton)
- turbo: `search_turbo_with_stats_and_config` (canonical generation, parallel Newton refinement within wide windows)
- streaming: `search_streaming_with_config`
- one-sided: `search_one_sided_with_stats_and_config`

## Complexity Metric (Core Engine)

Core complexity is additive over active symbol weights.

For an expression `e = (s_1, ..., s_n)`:

`C_expr(e) = sum_i w(s_i)`

Where:

- `w(s)` is the active weight for symbol `s`
- by default, weights come from `Symbol::weight()` (`src/symbol.rs`)
- when overridden by profiles/CLI, generation uses the active `SymbolTable`

For a match (equation) `lhs = rhs`:

`C_match(lhs, rhs) = C_expr(lhs) + C_expr(rhs)`

Important note:

- The core ranking metric does **not** add explicit tree-depth penalties.
- Tree depth and operator count may be reported as metadata (for JSON/reporting), but they are not part of `C_match`.

See `docs/COMPLEXITY.md` for the default weight table and rationale.

## Search Level to Complexity Limits

The CLI and library APIs use different level mappings.

### CLI (`src/cli/runtime.rs::cli_level_to_complexity`)

For CLI `-l/--level = L`:

- `max_lhs_complexity = 35 + 10*L`
- `max_rhs_complexity = 35 + 10*L`

This is calibrated for practical coverage while avoiding expression explosion.
Explicit CLI levels must be finite and in the range 0 through 100. Fractional
levels and the legacy shorthand `-l 2.5` without a positional target are supported.

### Library helper (`src/search.rs::level_to_complexity`)

Python, WebAssembly, and other programmatic callers use:

- `max_lhs = 40 + 2*L`
- `max_rhs = 40 + 2*L`

| Level | API `(max_lhs, max_rhs)` | CLI `(max_lhs, max_rhs)` |
|-------|--------------------------|--------------------------|
| 0 | (40, 40) | (35, 35) |
| 1 | (42, 42) | (45, 45) |
| 2 | (44, 44) | (55, 55) |
| 3 | (46, 46) | (65, 65) |
| 4 | (48, 48) | (75, 75) |
| 5 | (50, 50) | (85, 85) |

Level 3 is the usual default. Symbol weights make `2*x = 5` cost 32 on the left, `x = 5/2` and `x = 2*6` cost 35 on the right, `x = 6*7` cost 40, and `x = 9*9` cost 42. Budget 46 is one add or multiply past that heaviest single-digit product, so the default level finds those equations with room to spare. It is not the smallest budget that contains `6*7`.

The two scales are not the same function, and benchmarks should not treat them as interchangeable. The CLI step of 10 is the historical command-line tuning. Sharing it with the browser would make the demo's default (level 3, complexity 65) run for minutes on a target with no cheap exact match, and the CLI's own default (level 2, complexity 55) is already past an interactive full scan. The API step is 2 so that level 5, complexity 50, still finishes within a few seconds on that same full scan. `MAX_API_LEVEL` stays 5: the slider already covers the interactive range, and another step would cross the measured cliff past complexity 50.

## Match Ranking (Ordering)

The canonical comparator is `compare_matches` in `src/pool.rs`.

A row is exact when both of these hold (`match_is_exact`):

- the two sides, both evaluated at the typed target, agree to a few ulps of
  `max(|lhs|, |rhs|)` (`sides_agree_within_ulps`)
- the solved `x` is within 8 ulps of the typed target (`solved_x_within_ulps`)

Side agreement alone used to call a different root exact when the sides barely
move (`e^x = 1` at `1e-15`, Newton walks to 0; `cospi(plastic"/x) = 1` near
`1e-11`). The absolute window `|x_value - target| < EXACT_MATCH_TOLERANCE`
used to drop a flat identity whose sides already match (`x^(1/3) = (1+9)^2`
at `1e6`, about 7 ulps of `1e6`, and `x^(1/3) = 4` at `64`, about 2 ulps).
Eight ulps of `|target|` keeps both. Newton also keeps a steep root whose
correction is at most 8 ulps and whose residual is within
`1e-5 * max(|f(x)|, |rhs|)`, with a floor of `1e-300` so two vanished sides do
not collapse the cap. That looser accept is not exact. `max(|rhs|, 1)` is not
the scale: it turned every right-hand side below 1 into an absolute `1e-5`
window.

An LHS is skipped when its value at a non-integer target is within that same
side tolerance of its limit at 0, the value at `0.9 x` still is, and a probe at
`0.25`, `0.5`, or `±1` still moves. The `0.9 x` probe keeps a root that only
happens to equal the limit (`cospi(x^2) = 1` at `sqrt(2)`, `cospi(1/x) = 1`
at `0.1`). The check follows the flush, not a fixed `|x|` cutoff: `2 + x^2`
is bitwise 2 at `1.1e-8`, `4 + x^2` is still bitwise 4 at `2e-8`, and `2 + x^2`
stays within 8 ulps of 2 out near `6e-8` (`cospi(x^2)`, `pi ± x^2`, and
`3 ± x^2` with them). A derivative large enough that `|f' * x|` already exceeds
many ulps of the value cannot be that flush, so ordinary targets do not pay
for an extra evaluation at 0. Expressions undefined at 0 (`0^phi`, `1/x`) use
a subnormal stand-in for that limit. A true identity such as `x*(1/x) = 1` or
`log_x(x^2) = 2` holds at the probes and stays. Exact integers, including every
`|x| >= 2^52`, are not part of this rule: `cospi(x) = 1` on the evens equals
`cospi(0)`, and the integer-lattice check still drops `2 + 1/x` once `|x|` is
that large. Target 0 is excluded too: `x^2` is flat there but the value is genuine.

Ordering is:

1. Exactness (exact before non-exact)
2. Absolute error (`|x_value - target|`, smaller first) — **for non-exact matches
   only**. Rows flagged exact are treated as equally exact: the sides already
   agree at the target and the solved x is within 8 ulps, so the remaining
   x-error is not a quality signal. They collapse to a single error rank and
   are decided by complexity instead.
   This keeps the bounded pool from churning on sub-tolerance residuals and
   matches the original RIES intent of ranking exact forms by simplicity.
3. Ranking-mode tie-break:
   - `complexity`: lower `C_match` first
   - `parity`: lower legacy parity score first, then lower `C_match`
4. Lexicographic postfix order of LHS symbols
5. Lexicographic postfix order of RHS symbols

This final lexical tie-break is what makes total ordering explicit and stable.

Search keeps one admissible RHS representative for each bit-identical value.
It applies RHS symbol, value, and expression filters before choosing by the
configured ranking mode. Numeric-anagram searches keep a representative per
value and digit multiset. Parity compares the signed score first. Both modes compare complexity
and then postfix symbol order. Equations with the same RHS value can differ
on the left, while all use the same chosen RHS expression.

By default search passes all generated LHS forms to the matcher. With
`--lhs-value-dedup` (`SearchConfig::lhs_value_dedup` in Rust), materialized
searches keep a least-complex representative for each bit-identical LHS
(value, derivative) pair at the target. LHS expressions with identical keys
can differ in parity score or refine to different roots. Streaming, including
the automatic fallback, processes the generated LHS stream with either setting.

Without stop priority, streaming ranks the matches it discovers with the same
comparator as batch search. The streaming traversal and candidate windows can
admit a different set of matches, so an automatic fallback does not promise
the same rank one or ordered top k as a materialized run. The CLI `--max-memory`
option selects a strategy; it does not enforce an RSS limit.

Value bounds, expression constraints, numeric anagrams, and minimum match
distance are applied before matches enter the pool or trigger an early stop.
With canonical simplification enabled, the pool keeps the highest-ranked
equation for each canonical pair. An explicit maximum match distance bounds
the reported absolute error, including for exact rows. The CLI caps its pool
at 1,000,000 entries after applying report-mode sizing.

### Measured cost of the LHS key

A bounded study held RHS representative selection, the batch matcher, and
the order of equal-complexity LHS candidates fixed. It compared the former
target-local key with keeping every structurally distinct LHS. Across 20
cases (two small fixtures and 16 default-grammar searches, split across
complexity and parity ranking), the key changed the first equation in 7
cases and the ordered result list in 17. All seven first-equation changes
occurred under parity. One of them also changed the winning root by more
than eight ulps.

At complexity budget 55, the key removed 66–68% of LHS candidates. Keeping
them all raised peak RSS by 11–22%, while the median end-to-end time ranged
from 5% faster to 14% slower on an Apple M1 (five release runs per case).
The 17 changed lists measure equation identity and order. The default-grammar
cases returned the same number of exact rows with either policy.

At CLI level 3 (budget 65), the full grammar produced about 4.78 million
expressions for the `3.301` qualification case. Serial and parallel requests
crossed the two-million-expression guard and streamed. Turbo materialized the
search and peaked at 448 MiB. The forced streaming run and all three requested
modes returned the same ordered 16 equations on that case.

To compare materialized paths at the same budget, the qualification excluded
the symbols in `TASClW^vLE` and used a maximum match distance of `0.001`. For
the square-root-of-two parity case, serial preservation raised median wall
time from 0.55 to 1.04 seconds at about 148 MiB. Candidate checks rose from
0.51 to 1.48 million while RHS database construction stayed near 5 ms. The
larger matching cost is workload dependent, even when memory fits.

**Streaming stop priority.** With `stop_at_exact` or `stop_below`, the streaming
matcher still scans the full LHS stream and reports `early_exit = false`. It
tracks a qualifying match separately from the ranked pool. It prefers lower
LHS complexity, then lower RHS complexity, then lower absolute error, then
lexicographic LHS and RHS postfix strings. After the scan it places that match
first and truncates the list to `max_matches`. At capacity one this can replace
the best match under the configured ranking. With neither option, streaming
returns the ranked pool directly. The batch matcher can stop on a qualifying
match during traversal.

Under complexity ranking, once the bounded result pool is full of exact
matches, `TopKPool::exact_complexity_ceiling` returns the complexity of its
worst-ranked entry, which has the highest retained complexity. A new exact
match can be more complex than the simplest retained match and still displace
the worst one.

The batch matcher skips Newton refinement when the candidate's combined LHS and
RHS complexity is strictly greater than the ceiling. Candidates at the ceiling
remain eligible for the postfix tie-breaks. The scan can stop once the LHS
alone exceeds the ceiling, because the remaining LHS expressions are processed
in ascending complexity order and RHS complexity is nonnegative.

Parity ranking disables both uses of the ceiling. Its signed legacy score
precedes total complexity, so a more complex exact match can rank higher.

## Determinism Contract

`--deterministic` exists for reproducibility-oriented runs.

In deterministic mode, `ries-rs`:

- disables parallel search execution
- orders materialized candidates with explicit complexity and postfix tie-breaks
- applies stable final sorting with the canonical comparator

This yields stable output ordering for identical:

- target
- search level / complexity limits
- ranking mode and LHS deduplication policy
- symbol set / weights / profile configuration
- build + feature set

Practical caveat:

- floating-point behavior is deterministic within a given build/runtime environment, but results should still be treated as build-config specific unless a manifest is recorded (`--emit-manifest`).

## Turbo Contract

`--turbo` can evaluate Newton refinements in parallel within wide RHS windows.
Candidate admission uses the same traversal and the same single adaptive pool
as serial matching. Materialized runs with the same inputs therefore return
the same ordered equations, including rank one. First-hit options use serial
matching.

Turbo can materialize a larger search than the standard batch path. When one
path falls back to streaming, the two paths can discover different matches.
Speculative refinements count toward the reported Newton call total.

## Reproducible CLI Output Modes

For automation and archival:

- `--json`: structured stdout results + search statistics
- `--emit-manifest FILE`: full run manifest JSON for replay/audit metadata

Both formats record `matcher`, `lhs_value_dedup` (requested), and
`lhs_value_dedup_applied` (effective). In a streaming fallback, `matcher` is
`streaming` and `lhs_value_dedup_applied` is false even if the heuristic was
requested. The top-level CLI JSON `streaming` flag reflects the actual path.
Older manifests may omit these fields.

New CLI manifests record the resolved settings under `config.replay`. The
`generation` object contains the ordered symbol lists, effective weights,
per-side limits, and definitions for custom symbols. Symbols are encoded as
RIES byte values. The `matching` object records the pool size, filters,
trig scale, and refinement settings. The `output` object records display and
verification choices.

With the `highprec` feature, `--precision` verifies the top results in text
and JSON modes. JSON includes the verification results under
`high_precision_verification`. Each result has a `status` of `verified`,
`failed`, or `unavailable`. An equation using an operator that the
high-precision evaluator cannot evaluate has status `unavailable`.

`--match-all-digits` sets the maximum root error to half the place value of
the last digit in the target as typed. For example, `0.3330` and `3.330e-1`
both use a tolerance of `0.00005`. An explicit `--max-match-distance` takes
precedence. JSON reports the effective `trig_argument_scale` for interpreting
postfix results and reports the minimum retained root error as
`search_stats.best_error` (`null` for an empty pool). The internal radius
threshold is recorded separately as `adaptive_error_threshold`.

`--stability-check` compares results from the same matcher at each
tolerance level. The stability runs disable first-hit stopping, even when the
main search uses it.

Recommended reproducibility workflow:

```bash
ries-rs 3.141592653589793 --deterministic --json --emit-manifest run-manifest.json
```

## Source of Truth

If this document and code diverge, code wins.

Authoritative implementation points:

- `src/expr.rs` (representation + infix formatting semantics)
- `src/symbol.rs` / `src/symbol_table.rs` (symbol set + weights)
- `src/search.rs` (search execution)
- `src/pool.rs` (ranking + dedupe + ordering)
