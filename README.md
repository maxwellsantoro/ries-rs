# RIES-RS

**R**everse **I**nteger **E**quation **S**earch - A Rust implementation of Robert Munafo's RIES program.

RIES finds algebraic equations given their solution. Given a target number, it searches for equations that have that number as a solution.

## Quick Start

```bash
# Build
cargo build --release

# Find equations for π
./target/release/ries-rs 3.141592653589793

# Output:
#                    x = pi                       ('exact' match) {14}
#                  x-3 = 1/7                      for x = T + 1.26e-3 {24}
```

## Comparison Baselines

This project compares behavior against two historical references:

1. **Original RIES by Robert Munafo (mrob)**
2. **The `clsn/ries` fork**, which adds additional compatibility-oriented CLI behavior

In this repository, parity and compatibility tracking is documented in:

- `PARITY_REMAINING_REPORT.md`
- `docs/README.md`

## Compatibility Snapshot

| Area | mrob RIES | `clsn/ries` | ries-rs |
|------|-----------|-------------|---------|
| Core equation search | ✓ | ✓ | ✓ |
| Classic/sniper output flow | ✓ | ✓ | ✓ (`--classic`) |
| Legacy CLI semantics (`-p`, `-l`, `-i`, bare `-S`, bare `-E`) | partial | ✓ | ✓ |
| Core diagnostics (`-Ds`, `-Dy`, `-Do`, `-Dn`, `-DA`, `-DB`, `-DG`) | ✓ | ✓ | ✓ |
| Additional compatibility `-D` channels recognized | mixed | ✓ | ✓ |
| `-s` solve-for-x transformation | ✓ | ✓ | ✓ (safe transform + fallback) |
| Profile/include workflow | limited | ✓ | ✓ |
| Parallel search | ✗ | ✗ | ✓ |
| Report mode categories | ✗ | ✗ | ✓ |
| Streaming mode | ✗ | ✗ | ✓ |

## Known Differences From Older Versions

- `ries-rs` has additional modes/features (parallel, report mode, streaming) not present in upstream mrob RIES.
- `ries-rs` defaults to parity ranking in classic mode; use `--complexity-ranking` to force complexity-first ordering.
- Internal generation/scoring architecture is Rust-native, so exact result ordering and complexity numbers may still differ on some targets.

## Usage Examples

### Basic Search

```bash
ries-rs 3.14159          # Find equations for π
ries-rs 2.71828          # Find equations for e
ries-rs 1.61803          # Find equations for φ (golden ratio)
```

### Classic Mode (Like Original RIES)

```bash
ries-rs --classic 2.5
#                       x = 5/2                  ('exact' match) {14}
#                     2*x = 5                    ('exact' match) {16}

# Classic mode defaults to parity-style ranking:
ries-rs --classic 2.5063

# Force complexity-first ranking instead:
ries-rs --classic --complexity-ranking 2.5063
```

### Search Depth

```bash
ries-rs -l0 2.5          # Quick search
ries-rs -l2 2.5          # Default depth
ries-rs -l5 2.5          # Deep search
```

### Symbol Restrictions

```bash
ries-rs -N'+-' 2.5       # Exclude + and - operators
ries-rs -S'123*+' 2.5    # Only use 1, 2, 3, *, and +
```

### Numeric Type Restrictions

```bash
ries-rs -i 4.0           # Integer solutions only
ries-rs -r 3.5           # Rational solutions only
ries-rs -a 2.414         # Algebraic solutions only
```

### Output Formats

```bash
ries-rs -F default 2.5     # Default: 2*x = 5
ries-rs -F pretty 2.5      # Unicode: 2·x = 5
ries-rs -F sympy 2.5       # SymPy: Eq(2*x, 5)
ries-rs -F mathematica 2.5 # Mathematica: 2*x == 5
```

### User-Defined Constants

```bash
# Format: -X "weight:name:description:value"
ries-rs -X "8:gamma:Euler-Mascheroni:0.5772156649" 0.5772156649
# Will find: x = gamma
```

### User-Defined Functions

```bash
# Format: --define "weight:name:description:postfix_formula"
# Postfix uses | for dup, @ for swap
ries-rs --define "6:sinh:hyperbolic sine:E|r-2/" --classic 3.6268604078
# Will find: x = sinh(2)
```

## How To Compare With Older Versions

### Side-by-side output check

Use the included comparison script:

```bash
./tests/compare_with_original.sh 2.5063 2 6

# Or set the original binary path explicitly
RIES_ORIGINAL_BIN=/path/to/ries ./tests/compare_with_original.sh 2.5063 2 6

# Or pass original binary as arg 4
./tests/compare_with_original.sh 2.5063 2 6 /path/to/ries
```

Arguments:
- target value
- level
- max matches
- optional: path to original `ries` binary (or use `RIES_ORIGINAL_BIN`)

### Explicit classic-mode parity checks

```bash
# ries-rs parity-style classic ordering (default in --classic):
ries-rs --classic 2.5063

# ries-rs complexity-first classic ordering:
ries-rs --classic --complexity-ranking 2.5063

# ries-rs explicit parity flag (equivalent ordering in classic mode):
ries-rs --classic --parity-ranking 2.5063
```

### What to compare

- First page equation ordering in classic mode
- Whether legacy CLI options parse and behave the same
- Diagnostic channel acceptance and output shape
- Solve-for-x output safety/transform behavior (`-s`)

## Expression Syntax

RIES uses postfix (Reverse Polish) notation:

| Postfix | Infix | Description |
|---------|-------|-------------|
| `2x*` | `2*x` | Multiplication |
| `x1-r` | `1/(x-1)` | Reciprocal of (x-1) |
| `xs` | `x²` | Square |
| `xq` | `√x` | Square root |
| `xp1+` | `x+1` | Addition |
| `xE` | `eˣ` | Exponential |
| `xl` | `ln(x)` | Natural logarithm |

## Symbol Reference

### Constants
| Symbol | Value | Weight |
|--------|-------|--------|
| `1`-`9` | Integers 1-9 | 3-6 |
| `p` | π (pi) | 8 |
| `e` | e (Euler's number) | 8 |
| `f` | φ (golden ratio) | 10 |
| `g` | γ (Euler-Mascheroni) | 10 |
| `P` | ρ (plastic constant) | 10 |
| `z` | ζ(3) (Apéry's constant) | 12 |
| `G` | G (Catalan's constant) | 10 |
| `x` | Variable | 6 |

### Unary Operators
| Symbol | Operation | Weight |
|--------|-----------|--------|
| `n` | Negate (-) | 4 |
| `r` | Reciprocal (1/x) | 5 |
| `s` | Square (x²) | 5 |
| `q` | Square root (√x) | 6 |
| `l` | Natural log (ln) | 8 |
| `E` | Exponential (eˣ) | 8 |
| `S` | sin(πx) | 9 |
| `C` | cos(πx) | 9 |
| `T` | tan(πx) | 10 |
| `W` | Lambert W | 12 |

### Binary Operators
| Symbol | Operation | Weight |
|--------|-----------|--------|
| `+` | Addition | 3 |
| `-` | Subtraction | 3 |
| `*` | Multiplication | 3 |
| `/` | Division | 4 |
| `^` | Power | 5 |
| `v` | Nth root | 6 |
| `L` | Log base | 7 |
| `A` | atan2 | 7 |

Authoritative source for current weights: `src/symbol.rs`.

## How It Works

1. **Expression Generation**: Enumerate all valid postfix expressions up to complexity limit
2. **Fast Path**: Check for exact matches against known constants (π, e, √2, etc.) instantly
3. **Parallel Search**: Generate LHS (with x) and RHS (constants) expressions in parallel
4. **Matching**: For each LHS-RHS pair, use Newton-Raphson to solve LHS(x) = RHS
5. **Refinement**: Refine root estimates to high precision (1e-14)
6. **Ranking**: Sort by exactness/error, then parity-style or complexity-style ordering (mode-dependent)

## Command Line Options

The exhaustive, authoritative option list is the CLI help:

```bash
ries-rs --help
```

High-value option groups:

- Search and output: `-l/--level`, `-n/--max-matches`, `--classic`, `--report`, `--streaming`, `--parallel`, `-F/--format`, `-k/--top-k`
- Ranking: `--parity-ranking`, `--complexity-ranking`
- Legacy/compatibility semantics: `-p`, `-S`, `-E`, `-i`, `-l`, `--ie`, `--re`, `-s`, `--no-solve-for-x`
- Match controls: `--max-match-distance`, `--min-match-distance`, `--match-all-digits`, `--derivative-margin`, `--stop-at-exact`, `--stop-below`
- Expression constraints: `--rational-exponents`, `--any-exponents`, `--rational-trig-args`, `--any-trig-args`, `--max-trig-cycles`, `--any-subexpressions`
- Canonicalization and filtering: `--canon-reduction`, `--canon-simplify`, `--no-canon-simplify`, `--numeric-anagram`, `--min-equate-value`, `--max-equate-value`
- Diagnostics/verbosity: `-D[...]`, `--show-work`, `--stats`, `--verbose`, `--no-slow-messages`
- Profiles and extension points: `--profile`, `--include`, `-X/--user-constant`, `--define`, `--symbol-weights`, `--symbol-names`

## Installation

### From Source

```bash
cd ries-rs
cargo build --release
```

The binary will be at `target/release/ries-rs`.

### With Cargo

```bash
cargo install --path .
```

## How to Cite

If you use ries-rs in academic work, please cite it using the following BibTeX:

```bibtex
@software{ries-rs2026,
  author       = {RIES Contributors},
  title        = {ries-rs: A Rust Implementation of the RIES Inverse Equation Solver},
  year         = {2026},
  version      = {0.1.0},
  url          = {https://github.com/maxwellsantoro/ries-rs},
  license      = {MIT},
  note         = {Features parallel search, deterministic mode, and run manifest for reproducibility}
}
```

For reproducibility in publications, use `--deterministic` and `--emit-manifest` flags:

```bash
ries-rs --deterministic --emit-manifest manifest.json 3.141592653589793
```

## License

MIT License. See `LICENSE`.

## References

- [Original RIES](https://mrob.com/pub/ries/) by Robert Munafo
- [RIES Documentation](https://mrob.com/pub/ries/ries.html)
- [clsn/ries fork](https://github.com/clsn/ries)
- Stoutemyer, D.R. (2024). "Computing with No Machine Constants, Only Constructive Axioms". arXiv:2402.03304
