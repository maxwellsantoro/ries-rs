# Complexity weights in ries-rs

Expression complexity is the sum of its symbol weights. Equation complexity
is the sum of both sides; the equals sign adds no cost. Generation budgets
apply separately to each side. Ranking depends on the selected ranking mode:
complexity alone does not determine the default result order. Stop searches
prioritize the simplest qualifying match as described in [SEARCH_MODEL.md](SEARCH_MODEL.md).

## Default built-in weights

These are the historical RIES-calibrated defaults in
`src/symbol.rs::Symbol::default_weight`, including the project's extensions.
The Rust symbol names below identify each operation unambiguously. The table
is checked against the implementation by a regression test.

| Rust symbol | Weight |
|-------------|--------|
| One | 10 |
| Two | 13 |
| Three | 15 |
| Four | 16 |
| Five | 17 |
| Six | 18 |
| Seven | 18 |
| Eight | 19 |
| Nine | 19 |
| Pi | 14 |
| E | 16 |
| Phi | 18 |
| Gamma | 20 |
| Plastic | 20 |
| Apery | 22 |
| Catalan | 20 |
| X | 15 |
| Neg | 7 |
| Recip | 7 |
| Sqrt | 9 |
| Square | 9 |
| Ln | 13 |
| Exp | 13 |
| SinPi | 13 |
| CosPi | 13 |
| TanPi | 16 |
| LambertW | 20 |
| Add | 4 |
| Sub | 5 |
| Mul | 4 |
| Div | 5 |
| Pow | 6 |
| Root | 7 |
| Log | 9 |
| Atan2 | 9 |

User constant and function symbols default to 16; their definitions supply
custom weights. Symbol-table overrides affect the effective generation weights.
Weights are additive search costs, not mathematical measures of simplicity or
proof that expressions with the same cost are equivalent. Legacy parity ranking
uses a separate signed weight system (`Symbol::legacy_parity_weight`).

## Worked examples

| Equation | Sum of default weights | Complexity |
|----------|------------------------|------------|
| x = 2 | 15 + 13 | 28 |
| x + 1 = 3 | 15 + 10 + 4 + 15 | 44 |
| 2*x = 4 | 13 + 15 + 4 + 16 | 48 |
| x² = 4 | 15 + 9 + 16 | 40 |
| x² = π | 15 + 9 + 14 | 38 |
| sqrt(x) = 2 | 15 + 9 + 13 | 37 |
| exp(x) = π | 15 + 13 + 14 | 42 |
| x/2 = π | 15 + 13 + 5 + 14 | 47 |

For example, `2*x = 4` needs an LHS budget of at least 32 and an RHS
budget of at least 16, even though its total equation complexity is 48.

## Search levels

The CLI uses `35 + 10*level` per side. The Rust library level helper,
Python, and browser use `40 + 2*level` per side. These are deliberately
different scales; compare explicit budgets when comparing interfaces.

| Level | CLI budget per side | API budget per side |
|-------|---------------------|---------------------|
| 0 | 35 | 40 |
| 1 | 45 | 42 |
| 2 | 55 | 44 |
| 3 | 65 | 46 |

See [SEARCH_MODEL.md](SEARCH_MODEL.md) for level limits, fractional CLI levels,
pruning, and the conditions on search coverage. Larger budgets can sharply
increase time and memory use; the growth depends on the enabled grammar.

## Custom weights

Profiles and `--symbol-weights` can override defaults, for example
`--symbol-weights :p:25 :e:30`. User-defined constants and functions also carry
weights in their definitions. Reproducible experiments should record the effective
symbol configuration and budgets with `--emit-manifest`.

Changing default weights affects reachable expressions and observable ordering.
Validate such changes against search-contract tests and update this table and
examples together. Runtime weights use `u32`; user-function definition weights
are parsed as `u16`.
