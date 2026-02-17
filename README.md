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

## Benchmark vs Original C RIES

| Target | C ries | ries-rs | Speedup |
|--------|--------|---------|---------|
| π (exact match) | 0.004s | 0.003s | ~equal |
| e (exact match) | 0.002s | 0.003s | ~equal |
| √2 (exact match) | 0.003s | 0.003s | ~equal |
| φ (golden ratio) | 0.003s | 0.002s | ~equal |
| 2.506314 (no exact) | 0.19s | 0.02s | **11x faster** |
| 2.506314 -l4 | 6.4s | 1.0s | **6x faster** |

**Key insight**: ries-rs uses a fast-path optimization for known constants (π, e, √2, φ, etc.) that returns instantly, and parallel search for complex queries.

## Features Compared to Original RIES

| Feature | Original C | ries-rs |
|---------|------------|---------|
| Basic search | ✓ | ✓ |
| Newton-Raphson refinement | ✓ | ✓ |
| User constants (-X) | ✓ | ✓ |
| User-defined functions | ✓ | ✓ |
| Profile files | ✗ | ✓ |
| **Parallel search** | ✗ | ✓ (default) |
| **Report mode** | ✗ | ✓ (categorized output) |
| **Classic/sniper mode** | ✓ | ✓ (--classic) |
| Symbol filtering (-N, -S) | ✓ | ✓ |
| Numeric type restrictions (-a, -r, -i) | ✓ | ✓ |
| **Multiple output formats** | ✗ | ✓ (pretty, sympy, mathematica) |
| **Streaming mode** | ✗ | ✓ (low memory) |
| High precision (MPFR) | ✓ | Planned |

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
| `1`-`9` | Integers 1-9 | 4-7 |
| `p` | π (pi) | 8 |
| `e` | e (Euler's number) | 8 |
| `f` | φ (golden ratio) | 9 |
| `g` | γ (Euler-Mascheroni) | 8 |
| `P` | ρ (plastic constant) | 9 |
| `z` | ζ(3) (Apéry's constant) | 9 |
| `G` | G (Catalan's constant) | 9 |
| `x` | Variable | 2 |

### Unary Operators
| Symbol | Operation | Weight |
|--------|-----------|--------|
| `n` | Negate (-) | 2 |
| `r` | Reciprocal (1/x) | 4 |
| `s` | Square (x²) | 3 |
| `q` | Square root (√x) | 4 |
| `l` | Natural log (ln) | 6 |
| `E` | Exponential (eˣ) | 6 |
| `S` | sin(πx) | 7 |
| `C` | cos(πx) | 7 |
| `T` | tan(πx) | 7 |
| `W` | Lambert W | 12 |

### Binary Operators
| Symbol | Operation | Weight |
|--------|-----------|--------|
| `+` | Addition | 4 |
| `-` | Subtraction | 4 |
| `*` | Multiplication | 4 |
| `/` | Division | 4 |
| `^` | Power | 7 |
| `v` | Nth root | 7 |
| `L` | Log base | 7 |
| `A` | atan2 | 7 |

## How It Works

1. **Expression Generation**: Enumerate all valid postfix expressions up to complexity limit
2. **Fast Path**: Check for exact matches against known constants (π, e, √2, etc.) instantly
3. **Parallel Search**: Generate LHS (with x) and RHS (constants) expressions in parallel
4. **Matching**: For each LHS-RHS pair, use Newton-Raphson to solve LHS(x) = RHS
5. **Refinement**: Refine root estimates to high precision (1e-14)
6. **Ranking**: Sort matches by complexity (sum of symbol weights)

## Command Line Options

```
Options:
  -l, --level <LEVEL>        Search level (default: 2)
  -n, --max-matches <N>      Maximum matches to display (default: 16)
  -x, --absolute             Show absolute x values
  -s, --solve                Solve for x form
  -N, --exclude <SYMS>       Exclude symbols
  -S, --only-symbols <SYMS>  Only use these symbols
  -a, --algebraic            Algebraic solutions only
  -r, --rational             Rational solutions only
  -i, --integer              Integer solutions only
  --classic                  Classic/sniper mode (like original RIES)
  --parallel                 Use parallel search (default: true)
  --streaming                Low memory mode
  -F, --format <FORMAT>      Output format: default, pretty, sympy, mathematica
  -k, --top-k <N>            Matches per category in report mode (default: 8)
  --stop-at-exact            Stop when exact match found
  -X, --user-constant <SPEC> Add user constant
  --define <SPEC>            Define user function
  -p, --profile <FILE>       Load profile file
```

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

## License

MIT License. Based on Robert Munafo's original RIES program.

## References

- [Original RIES](https://mrob.com/pub/ries/) by Robert Munafo
- [RIES Documentation](https://mrob.com/pub/ries/ries.html)
