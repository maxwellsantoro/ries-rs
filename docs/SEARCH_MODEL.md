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

Postfix strings use one character per symbol (`Symbol::to_postfix_char` and
`Symbol::from_postfix_char`). Built-in symbols are ASCII. User constant slots
0-15 and user function slots 0-15 are U+0080..U+009F; they are decoded per
character, never per UTF-8 byte. `Expression::parse` also accepts the printable
user-function aliases `H I J K M N O Q R U V Y Z B D F`. `-S`, `-N`, `-E`, `-O`,
`--symbol-weights`, and `--symbol-names` decode characters the same way, so a
character above U+00FF never truncates onto an ASCII symbol.

## Symbol Classes (Arity)

Each symbol has a fixed arity category (`Seft` in code):

- `A`: atom / constant / variable (`0`-ary push)
- `B`: unary operator (`1`-input, `1`-output)
- `C`: binary operator (`2`-input, `1`-output)

The search model is defined over the active symbol set after applying CLI/profile filters (`-S`, `-N`, presets, user constants/functions, etc.).
Use `-E` or `-E all` to clear both `-S` and `-N`. Pass symbol codes such as
`-E lA` to restore only `ln` and `atan2` to the active set.

A user-defined function starts with its argument on the postfix stack. Use `|`
to duplicate that argument. A function body cannot reference the search variable
`x` directly, so a function applied to a constant stays constant during matching.

For a negative radicand, the root operator requires an exactly represented odd
integer index. If the index changes with `x` at that point, the evaluator can
return the point value but supplies no finite derivative for refinement.

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

Numeric classification describes expression structure, not the value sampled at
one target. The variable is a polynomial indeterminate: `x^2` preserves the
integer-polynomial class, but `2^x` and a root with an x-dependent index have no
algebraic guarantee, even at an integer or stationary sample. Evaluation tracks
variable dependence separately from the derivative, including through user
functions. Shared power/root classification rules govern evaluation, symbol type
inference, and structural constraints. Constant rational powers of algebraic
bases remain algebraic; equivalent square-root spellings therefore all survive
algebraic-only filtering.

Division and reciprocal reject an actual zero denominator. Nonzero subnormal
operands remain eligible; unrepresentable results are handled as overflow, and
nonfinite derivatives remain unavailable for root refinement.

Integer-only generation excludes negative powers of integer bases: `2^(-2)`
is classified as rational, while `2^3` remains integer. Type-only inference
conservatively treats an integer power of an integer base as rational when the
exponent's sign is unknown. Root pruning reads the index operand in `a b v`
(`b^(1/a)`), so cube roots of 2 remain eligible.

The evaluator scales reciprocal, power, root, logarithm, and `atan2` derivative
calculations to avoid losing representable slopes through intermediate overflow
or underflow. A power's value may underflow to zero while its derivative remains
representable.

Root expressions preserve an algebraic classification only when the index is
rational and the radicand is algebraic. A transcendental radicand or an irrational
index receives a conservative non-algebraic classification, so these roots do
not bypass algebraic-only generation. Binary identity pruning inspects complete
operands, including when an exponent or factor is a compound expression.

A user function's class depends on its argument: with `sq(a) = a^2`, `sq(2)`
is an integer and `sq(pi)` is not. Structural constraints (`--rational-exponents`,
`--rational-trig-args`, `--max-trig-cycles`) therefore walk the function body
using `UserFunctionConstraints`. The body counts toward trig cycles and must
satisfy the same restrictions. A function without a body summary is opaque,
and its fixed `user_function_types` entry applies. The walker and evaluator
traverse expressions separately but use the same built-in type transitions in
`Symbol::result_type` and `result_type_with_facts`. A differential test
(`constraint_walker_type_matches_evaluator`) checks traversal agreement on
small expressions. Without a known value for a non-literal integer exponent,
the walker can conservatively classify a power as rational rather than integer.

### Pruning

Generation prunes candidates before evaluation (`src/gen.rs`). Each rule
belongs to one class:

- **Identity.** The candidate is degenerate (`a/a`, `1^b`, `log_1(b)`, roots of
  1) or equals a spelling from its own symbols at no greater complexity
  (`--a`, `ln(e^a)`, `-(a-b)` as `b-a`, `2+3` as `3+2`). A respelling counts
  only when that spelling is not itself pruned. `1/sqrt(ln a)` is kept because
  `1/ln(a)` is pruned heuristically.
- **Rewrite.** The candidate has a preferred spelling that uses a symbol it does
  not contain: `2 b v` as `b q`, `log_e(b)` as `ln(b)`, and `a+a` as `2*a`. A
  rewrite fires only when the replacement is enabled on the side being
  generated, is not heavier under the active weights, fits its per-symbol
  count limit, and is not itself pruned. With `-S x2v`, for example, `x = 22v`
  remains reachable.
- **Heuristic.** Deliberate, lossy reductions that may remove values:
  `sqrt(a^2)`, `1/ln(a)`, `sinpi(sinpi(a))`, `cospi(cospi(a))`,
  `exp(exp(a))`, `W(e^a)`, and `W(1/a)`. `(a^2)^2` and `sqrt(sqrt(a))` are pruned
  only when a power (`^` and `4`) or a fourth root (`v` and `4`) is enabled.

`pruning_never_loses_reachable_values` compares generation under restricted
symbol sets, custom weights, and count limits against generation with only
the heuristics active. Identities and rewrites must never lose a value or
raise its minimum complexity. Excluded from this check are constant LHS sides
and RHS values 0 and 1, which come from degenerate forms: `A - B = 0` and
`A / B = 1` are costlier spellings of `A = B`.

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

A row is exact when `match_exactness` accepts it and a double-double
re-evaluation agrees (`label_exact`):

- the two sides, both evaluated at the typed target, agree to a few ulps of
  `max(|lhs|, |rhs|)` (`sides_agree_within_ulps`)
- the solved `x` is within eight ulps of the typed target
- one ulp of the RHS corresponds to at most eight ulps of the target under
  the LHS slope, i.e. `ulp(rhs) ≤ 8 · ulp(target) · |f′(target)|`
- those f64 sides still agree to eight ulps when both expressions are
  evaluated again at the same f64 in double-double arithmetic

The last check is what rejects a rounded reciprocal or argument. At `1e-15`,
f64 `cospi(1/x)` lands on `cospi(1/8)` after the reciprocal rounds onto an
eighth-turn, and the trig reduction then reports a zero residual. The real
values at that f64 differ by about `0.046`. The same pattern stamps
`e^sinpi(1/x) = 1` at `1e-12` and `cospi(pi + x) = -cospi(1/8)` at `1e15`.
Double-double can see those gaps. An argument whose magnitude exceeds `2^60`,
or any evaluation the checker cannot finish, is not labeled exact. Genuine
identities stay exact: `2*x = 5` at `2.5`, `x = pi`, `x = sqrt(2)`, `sinpi` of
an integer, and `x = ln(1)` at `0`.

A stationary LHS cannot locate a root with the linearized search and is
skipped. Every match, including an approximation, must resolve x at the
precision of its claimed error: `ulp(rhs) / |f′(target)|` must be at most
`max(|solved_x - target|, 8 · ulp(target))`. A rounded residual of zero can
span many representable values around a trigonometric extremum or arise
from an identity such as `x*(1/x) = 1`.

Side agreement alone used to call a different root exact when the sides barely
move (`e^x = 1` at `1e-15`, Newton walks to 0; `cospi(plastic"/x) = 1` near
`1e-11`). The absolute window `|x_value - target| < EXACT_MATCH_TOLERANCE`
used to drop roots whose sides already match (`x^(1/3) = (1+9)^2` at `1e6`,
about seven ulps of `1e6`, and `x^(1/3) = 4` at `64`, about two ulps).
Newton tests the residual against
`1e-10 * max(|f(x)|, |rhs|)`. It also keeps a steep root whose correction is
at most 8 ulps and whose residual is within `1e-5 * max(|f(x)|, |rhs|)`.
Exactness is still decided at the original target. Using the actual side
magnitudes prevents a change in equation scale from hiding a large relative miss.

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
a small nonzero stand-in for that limit. The flush check applies only at
non-integer targets. The integer-lattice probe still drops `2 + 1/x` once
`|x|` is large enough that the reciprocal vanishes into the sum.

Ordering is:

1. Exactness (exact before non-exact)
2. Absolute root error bucket, smaller first. Each bucket spans eight ulps
   of the target. Exact rows share a single error rank. The report's Best
   Approximations section also uses these buckets with complexity ties.
   The raw error is preserved for output and tolerance checks.
3. Ranking-mode tie-break:
   - `complexity`: lower `C_match` first
   - `parity`, exact rows: lower `C_match` first, then lower legacy parity score
   - `parity`, inexact rows: lower legacy parity score first, then lower `C_match`

Exact rows are one root. Ordering them by complexity keeps `2*x = 5`,
`x = 5/2`, `x = 3/2`, `x = 2*6`, `x = 6*7`, `x = 2`, `x = pi`, `x = e`,
`x = phi`, and `x = sqrt(2)` on the first page of a level-3 parity search
(`maxMatches` 50, the website default). Operator-dense rewrites of those roots
used to fill that page because Add and Mul have negative parity weights.
Inexact approximations keep the historical parity order.
4. Lexicographic postfix order of LHS symbols
5. Lexicographic postfix order of RHS symbols

This final lexical tie-break is what makes total ordering explicit and stable.

Within each report category, the highest-ranked row represents equations that
share the same default infix form or differ only by negating both sides.
Default and pretty infix omit multiplication by literal `1` and use explicit
`*` after compound factors. Postfix output records the original operations.

Search keeps one admissible RHS representative for each bit-identical value.
It applies RHS symbol, value, and expression filters before choosing the
simplest spelling. Parity breaks a complexity tie. A denser rewrite of the
same bits does not replace `5` or `5/2`, so the plain equation can appear in
the parity page above. Numeric-anagram searches keep a representative per
value and digit multiset. Equations with the same RHS value can differ
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
the reported absolute error, including for exact rows. The default CLI report
has room for at least 5,000 candidates so its simplicity-based categories can
still see equations crowded out by closer approximations. The CLI caps the
pool at 1,000,000 entries after applying report-mode sizing.

For nonzero targets smaller than one in magnitude, the adaptive candidate
window and initial pool acceptance threshold scale with `|target|`. The radius
cap uses an eight-ulp floor in x rather than an absolute `1e-10` floor, so tiny
targets do not scan RHS values far outside their local x scale. Zero retains
the ordinary search window.

In CLI reports, Best approximations treats errors at or below half the place
value of the last typed decimal digit as equally accurate and prefers lower
complexity, then lower ugliness. For example, `3.14159265` puts `x = pi` before
more complicated cancellation fits with errors below `5e-9`. Extra trailing
zeros tighten this threshold. This report preference does not change the
recorded errors, exactness classification, or canonical pool ordering.
Library reports can opt into this behavior with
`ReportConfig::with_target_precision`; the default `best_error_floor` is zero.
CLI, WASM, and Python infix simplify certified constant zeros such as
`sinpi(1)` and `ln(1)` to `0`; postfix output retains the searched expression.
A level-3 parity search for `0` or `-0` therefore shows plain `x = 0`.


### Measured cost of the LHS key

The measurements below predate the current exactness and error-bucketing
rules. Use the [paired benchmark runner](PERFORMANCE.md#comparing-lhs-deduplication-policies)
to measure the current checkout.

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

**Stop contract.** With `stop_at_exact` or `stop_below`, every matcher (batch,
streaming, one-sided, and the parallel and turbo paths, which delegate to
batch) follows one rule, implemented once in `StopBound` (`src/search.rs`). A
match that meets the stop condition bounds result complexity by its own total
complexity. The simplest such match, with ties broken by the requested
ranking, is the stop match.

1. Phase 1 scans until no unvisited pair can fall within the bound. Batch and
   one-sided matching visit expressions in complexity order and end early;
   streaming scans its whole LHS stream and reports `early_exit = false`.
2. Phase 2 rescans only the pairs within the bound, using a fresh pool.
   Streaming regenerates LHS expressions up to the bound for this pass.
3. The result contains the phase-2 matches within the bound, led by the stop
   match and truncated to `max_matches`.

Before the bound is known, a pool can admit costlier matches that displace
cheaper ones, so phase 2 makes the result independent of visit order. For
bit-identical RHS values under a stop condition, the RHS representative is the
simplest spelling, with ties broken by the ranking. A costlier spelling would
raise the bound, and any costlier spelling lies past it anyway.
`tests/stop_contract.rs` checks that batch, streaming, and the certified fast
path agree. It also independently enumerates a tiny arithmetic grammar and
solves each linear/quadratic LHS analytically. This oracle checks the simplest
qualifying winner under both stop conditions, both rankings, custom weights,
and a pool of one, without using generation, Newton, or `StopBound` to choose
that winner. Agreement between optimized paths alone is not sufficient.

**Fast match.** The CLI skips the search only for an exact `x = <atom>` match
from `find_certified_fast_match`. It tries every enabled RHS atom and applies
the caller's full eligibility check. Among atoms meeting a stop condition
(exact, or within `stop_below`, which `--classic` sets), it chooses the simplest,
with ties broken by `compare_matches`. The choice is accepted only when it is
exact and its complexity is below
`non_atomic_equation_complexity_floor`, which is `w(x) + min_atom +
min(min_unary, min_binary + min_atom)` over the enabled symbols. Below that
floor no other equation shape can be as simple. Other targets run the search.
A fast match reports a single result.

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

Parity ranking disables both uses of the ceiling. Inexact rows still rank by
legacy score before complexity, so the worst retained complexity is not a
bound on what can enter. Exact rows are complexity-first: a more complex
exact match does not outrank a simpler one.

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

Both formats report whether the selected matcher uses Rayon. Serial execution,
including one-sided search, streaming fallback, and fast matches, reports one
thread. A parallel matcher reports the size of its Rayon thread pool.

New CLI manifests record the resolved settings under `config.replay`. The
`generation` object contains the ordered symbol lists, effective weights,
per-side limits, and definitions for custom symbols. Symbols are encoded as
RIES byte values. The `matching` object records the pool size, filters,
trig scale, and refinement settings. The `output` object records display and
verification choices. Self-contained CLI runs also record `cli_args` for
`tools/replay_manifest.py`. The tool checks the resolved settings and ranked
results against the original manifest. It uses an empty profile and refuses
file paths from manifests. Runs that rely on profile files, implicit profile
settings, or legacy implicit-target syntax may require manual reproduction.
For older manifests, replay is limited to the settings that were recorded.

With the `highprec` feature, `--precision` verifies the top results in text
and JSON modes. It refines the reported root locally in higher precision and
checks the residual relative to the scale of the two sides. JSON includes the
verification results under
`high_precision_verification`. Each result has a `status` of `verified`,
`failed`, or `unavailable`. Verification requires more than 53 bits of
working precision. A request for 1 through 53 bits, or an equation with an
unsupported operator, has status `unavailable`.

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
python3 tools/replay_manifest.py run-manifest.json --bin ries-rs
```

## Source of Truth

If this document and code diverge, code wins.

Authoritative implementation points:

- `src/expr.rs` (representation + infix formatting semantics)
- `src/symbol.rs` / `src/symbol_table.rs` (symbol set + weights)
- `src/search.rs` (search execution)
- `src/pool.rs` (ranking + dedupe + ordering)

## Effective RHS grammar and numerical interpretation

Search resolves `SearchConfig::rhs_allowed_symbols` and `rhs_excluded_symbols`
into the RHS generation grammar before pruning, representative selection, or
matching. The intersection never adds symbols omitted by `GenConfig`, and LHS
symbols are unaffected. Library callers do not need to synchronize these lists
manually. Batch, streaming, one-sided, parallel, and turbo use this resolution.

Numeric type transitions for built-in operators live in `Symbol::result_type`
and `result_type_with_facts`. Evaluation and the structural walker use those
same transitions; the walker also interprets UDF bodies and records restriction
events. Unknown constant values can require a conservative class rather than
assuming a sign from a sampled derivative.

The stop contract is complexity-first even with parity ranking: minimize the
complexity of a qualifying stop match, break ties with `compare_matches`, then
rank the bounded second-pass pool with the requested ranking and prepend the
stop match. Parity cannot widen the complexity bound to choose a more complex
stop match. This ordering is deliberate and applies to both stop conditions.

An oscillatory equation is not automatically an impostor. For example,
`cospi(1/(pi-x)) = 1` near the typed target `3.14159` has the real root
`pi - 1/376848`. The historical reported float `3.141590000000015` is within
eight ulps of that root, but the root is more than eight ulps from the target;
it is an approximation, not exact. A 192-bit independent analytic check in
`tests/numeric_contract.rs` preserves this distinction. Such equations can
encode many nearby roots and are weak evidence for identifying a rounded
constant, even though a nearby root is mathematically valid. This does not
justify admitting an RHS outside cosine's range or a float with no nearby root.

Sine/cosine RHS values outside [-1, 1] are rejected before refinement and pool
admission, including when refinement is disabled. The Newton entry point uses
the same range check. A loose residual threshold near an extremum is insufficient
to establish existence of a root; `tests/numeric_contract.rs` includes an
out-of-range regression that failed before this guard.

## Independent qualification and resource limits

[QUALIFICATION.md](QUALIFICATION.md) documents the finite matcher corpus,
randomized tiny-grammar analytic oracle, historical polynomial differential,
experimental rational root certificates, and Linux address-space supervisor.
CLI JSON includes `target_literal` and a candidate-window histogram; high-precision
reevaluation still starts from the parsed `f64`, not that original decimal text.
No narrower candidate gate or new completeness claim is introduced by these tools.

## Decimal input, verification requests, and adaptive bounds

CLI decimal input and WASM `targetText` use `target::validate_decimal_target`.
It compares exact decimal integers to the exact binary f64 value without rounded
subtraction. With match-all-digits enabled, parsing error must be at most half
the last typed decimal place. Nonzero input rounded to zero is always rejected.
These checks protect the requested input precision; accepted results still carry
numerical exactness, not symbolic proof. The shared check bounds input length to
4096 characters. An independent Python `Fraction` oracle exercises 110 boundary
and seeded cases in CI (`scripts/check_decimal_boundary.py`).

Explicit CLI verification requests fail when unavailable unless
`--allow-unverified` is supplied; fallback JSON then records unavailable status.
The flag is preserved in self-contained replay arguments.

Adaptive generation deliberately grows independently of supplied fixed bounds.
Its statistics retain actual final per-side bounds; JSON and manifests use those
as `max_lhs_complexity` and `max_rhs_complexity`. JSON also records requested
bounds, and manifest replay metadata stores `requested_complexity_bounds`.
CLI adaptive and one-sided modes reject `--streaming`, `--turbo`, and memory-hint
flags, as well as each other. Their fixed corpus case compares the applicable
serial/parallel adaptive paths and checks every returned side against its reported
bound using the manifest's effective symbol weights.

`--pslq --match-all-digits` is explicitly unsupported and returns a configuration
error before results. Decimal validation still applies to PSLQ targets, but a
PSLQ residual tolerance is not a general root-distance guarantee. Use ordinary
numerical search for the match-all-digits result contract.
