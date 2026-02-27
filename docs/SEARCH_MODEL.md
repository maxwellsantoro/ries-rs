# RIES-RS Search Model (v1.0)

This document defines the core computational model implemented by `ries-rs` for Phase A / v1.0.

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

In infix form, parentheses are added according to precedence/associativity rules implemented in `src/expr.rs` (power is right-associative; multiplication/division bind tighter than addition/subtraction).

## Search Space Partition

RIES-style search builds equations by matching:

- LHS expressions containing `x`
- RHS expressions that are constant-only

The engine generates candidate expressions up to complexity limits, evaluates them numerically, and then tests LHS/RHS pairs with Newton refinement (unless disabled).

Search implementations:

- sequential: `search_with_stats_and_config`
- parallel: `search_parallel_with_stats_and_config`
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

Important v1.0 note:

- The core ranking metric does **not** add explicit tree-depth penalties.
- Tree depth and operator count may be reported as metadata (for JSON/reporting), but they are not part of `C_match`.

See `docs/COMPLEXITY.md` for the default weight table and rationale.

## Search Level to Complexity Limits

The CLI and library APIs use different level mappings.

### CLI (`src/main.rs`)

For CLI `-l/--level = L`:

- `max_lhs_complexity = 35 + 10*L`
- `max_rhs_complexity = 35 + 10*L`

This is calibrated for practical coverage while avoiding expression explosion.

### Library helper (`src/search.rs::level_to_complexity`)

Programmatic API helper uses a lighter mapping:

- `max_lhs = 10 + 4*L`
- `max_rhs = 12 + 4*L`

This difference is intentional and should not be conflated in benchmark or reproducibility claims.

## Match Ranking (Ordering)

The canonical comparator is `compare_matches` in `src/pool.rs`.

Ordering is:

1. Exactness (exact before non-exact)
2. Absolute error (`|x_value - target|`, smaller first)
3. Ranking-mode tie-break:
   - `complexity`: lower `C_match` first
   - `parity`: lower legacy parity score first, then lower `C_match`
4. Lexicographic postfix order of LHS symbols
5. Lexicographic postfix order of RHS symbols

This final lexical tie-break is what makes total ordering explicit and stable.

## Determinism Contract

`--deterministic` exists for reproducibility-oriented runs.

In deterministic mode, `ries-rs`:

- disables parallel search execution
- applies stable final sorting with the canonical comparator

This yields stable output ordering for identical:

- target
- search level / complexity limits
- ranking mode
- symbol set / weights / profile configuration
- build + feature set

Practical caveat:

- floating-point behavior is deterministic within a given build/runtime environment, but results should still be treated as build-config specific unless a manifest is recorded (`--emit-manifest`).

## Reproducible CLI Output Modes

For automation and archival:

- `--json`: structured stdout results + search statistics
- `--emit-manifest FILE`: full run manifest JSON for replay/audit metadata

Recommended v1.0 reproducibility workflow:

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
