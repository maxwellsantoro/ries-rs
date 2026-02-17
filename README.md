# RIES-RS

**R**everse **I**nteger **E**quation **S**earch - A Rust implementation of Robert Munafo's RIES program.

RIES finds algebraic equations given their solution. Given a target number, it searches for equations that have that number as a solution.

## Overview

```
$ ries-rs 2.5

   Your target value: T = 0.5772100000          ries-rs v0.1.0

Generated 1234 LHS and 567 RHS expressions

  -- Exact matches --

                       x = 5/2                  ('exact' match) {14}
                     2*x = 5                    ('exact' match) {16}
                 1/(x-1) = 2/3                  ('exact' match) {24}

  -- Best approximations --

                       x = phi                  for x = T - 8.97e-2 {9}
                  sqrt(x) = e^(-1)              for x = T + 1.35e-2 {22}
```

## How It Works

RIES generates expressions in postfix notation and matches them:

1. **LHS Generation**: Creates left-hand side expressions containing `x` (e.g., `2x*` for `2*x`)
2. **RHS Generation**: Creates right-hand side constant expressions (e.g., `5` for `5`)
3. **Matching**: Uses Newton-Raphson iteration to find `x` values where `LHS(x) = RHS`
4. **Ranking**: Sorts matches by complexity (sum of symbol weights)

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

## Usage

### Basic Search

```bash
ries-rs 3.14159          # Find equations for π
ries-rs 2.71828          # Find equations for e
ries-rs 1.61803          # Find equations for φ (golden ratio)
```

### Search Depth

The `-l` (level) option controls search depth. Higher levels find more equations:

```bash
ries-rs -l0 2.5          # Quick search (~89M equations)
ries-rs -l2 2.5          # Default depth (~11B equations)
ries-rs -l5 2.5          # Deep search (~15T equations)
```

### Symbol Restrictions

Limit which mathematical symbols are used:

```bash
ries-rs -N'+-' 2.5       # Exclude + and - operators
ries-rs -S'123*+' 2.5    # Only use 1, 2, 3, *, and +
```

### Numeric Type Restrictions

Restrict solutions to specific number types:

```bash
ries-rs -i 4.0           # Integer solutions only
ries-rs -r 3.5           # Rational solutions only
ries-rs -a 2.414         # Algebraic solutions only
```

### Expression Evaluation

Evaluate an expression at a given `x` value:

```bash
ries-rs --eval-expression "2x*" --at 3    # Evaluates 2*3 = 6
ries-rs --eval-expression "xq" --at 4     # Evaluates sqrt(4) = 2
```

## User-Defined Constants

Add custom constants with the `-X` option:

```bash
# Format: -X "weight:name:description:value"
ries-rs -X "8:g:gamma:0.5772156649" 0.5772156649
# Will find: x = u0 (where u0 is the user constant g)
```

User constants are assigned to symbols `u0` through `u15`.

## Profile Files

Create `.ries` profile files for reusable configurations:

```
# my_constants.ries
-X "8:gamma:Euler-Mascheroni:0.5772156649"
-X "10:phi:Golden Ratio:1.6180339887498948482"
--symbol-names :p:π :e:ℯ
```

Load profiles with `-p`:

```bash
ries-rs -p my_constants.ries 0.5772156649
```

## Output Formats

Choose how expressions are displayed:

```bash
ries-rs -F default 2.5     # Default: 2*x = 5
ries-rs -F pretty 2.5      # Unicode: 2·x = 5
ries-rs -F sympy 2.5       # SymPy: 2*x - 5
ries-rs -F mathematica 2.5 # Mathematica: 2*x == 5
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
| `xe` | `eˣ` | Exponential |
| `xl` | `ln(x)` | Natural logarithm |

## Symbol Reference

### Constants
| Symbol | Value | Weight |
|--------|-------|--------|
| `1`-`9` | Integers 1-9 | 4-7 |
| `p` | π (pi) | 8 |
| `e` | e (Euler's number) | 8 |
| `f` | φ (golden ratio) | 9 |
| `x` | Variable | 2 |

### Unary Operators
| Symbol | Operation | Weight |
|--------|-----------|--------|
| `n` | Negate (-) | 2 |
| `r` | Reciprocal (1/x) | 4 |
| `s` | Square (x²) | 3 |
| `q` | Square root (√x) | 4 |
| `l` | Natural log (ln) | 6 |
| `e` | Exponential (eˣ) | 6 |

### Binary Operators
| Symbol | Operation | Weight |
|--------|-----------|--------|
| `+` | Addition | 4 |
| `-` | Subtraction | 4 |
| `*` | Multiplication | 4 |
| `/` | Division | 4 |
| `^` | Power | 7 |

## Features Compared to Original RIES

| Feature | Original C | ries-rs |
|---------|------------|---------|
| Basic search | ✓ | ✓ |
| Newton-Raphson refinement | ✓ | ✓ |
| User constants (-X) | ✓ | ✓ |
| Profile files | ✓ | ✓ |
| Parallel search | ✗ | ✓ |
| Report mode | ✗ | ✓ |
| Symbol filtering | ✓ | ✓ |
| Numeric type restrictions | ✓ | ✓ |
| High precision | ✓ | Planned |
| User-defined functions | ✓ | Planned |

## Algorithm

1. **Expression Generation**: Enumerate all valid postfix expressions up to complexity limit
2. **Deduplication**: Keep only the simplest expression for each (value, derivative) pair
3. **Matching**: For each LHS-RHS pair, use Newton-Raphson to solve LHS(x) = RHS
4. **Refinement**: Refine root estimates to high precision (1e-14)

The search is exhaustive within complexity bounds, guaranteeing that the simplest exact match is found.

## Performance

On a modern CPU:
- Level 0: ~0.1 seconds
- Level 2: ~1 second
- Level 5: ~10 seconds

Parallel search (`--parallel`, default) uses all available cores.

## License

MIT License. Based on Robert Munafo's original RIES program.

## References

- [Original RIES](https://mrob.com/pub/ries/) by Robert Munafo
- [RIES Documentation](https://mrob.com/pub/ries/ries.html)
