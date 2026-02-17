# Complexity Weight System in RIES

This document explains the complexity weight calibration methodology used in RIES
to rank equations by simplicity.

## Overview

RIES uses a complexity scoring system to present simpler equations first.
Each symbol (constant, variable, or operator) has a weight, and an expression's
total complexity is the sum of its symbols' weights.

Lower complexity = simpler equation = shown first.

## Weight Table

### Constants (Seft::A)

| Symbol | Name | Weight | Rationale |
|--------|------|--------|-----------|
| 1 | One | 3 | Smallest positive integer, most fundamental |
| 2 | Two | 3 | Most common multiplier, "doubling" is basic concept |
| 3 | Three | 4 | First prime after 2 |
| 4 | Four | 4 | 2², common in geometry |
| 5 | Five | 5 | Half of 10, common in mental math |
| 6 | Six | 5 | 2×3, highly composite |
| 7 | Seven | 6 | Often considered "random" looking |
| 8 | Eight | 6 | 2³, cubic |
| 9 | Nine | 6 | 3², close to 10 |
| π | Pi | 8 | Fundamental transcendental |
| e | Euler's number | 8 | Fundamental transcendental |
| φ | Golden ratio | 10 | Algebraic but less common |
| γ | Euler-Mascheroni | 10 | Believed transcendental, obscure |
| ρ | Plastic constant | 10 | Algebraic (root of x³=x+1), obscure |
| ζ(3) | Apery's constant | 12 | Irrational but type unknown, very obscure |
| G | Catalan's constant | 10 | Believed transcendental, obscure |
| x | Variable | 6 | The unknown we're solving for |

### Unary Operators (Seft::B)

| Symbol | Name | Weight | Rationale |
|--------|------|--------|-----------|
| -x | Negation | 4 | Simplest unary operation |
| 1/x | Reciprocal | 5 | Division by self |
| √x | Square root | 6 | Inverse of squaring |
| x² | Square | 5 | Multiplying by self, very common |
| ln(x) | Natural log | 8 | Transcendental |
| e^x | Exponential | 8 | Transcendental, inverse of ln |
| sin(πx) | Scaled sine | 9 | Periodic transcendental |
| cos(πx) | Scaled cosine | 9 | Periodic transcendental |
| tan(πx) | Scaled tangent | 10 | Has asymptotes, more complex |
| W(x) | Lambert W | 12 | Most complex, rarely needed |

### Binary Operators (Seft::C)

| Symbol | Name | Weight | Rationale |
|--------|------|--------|-----------|
| + | Addition | 3 | Most basic arithmetic |
| - | Subtraction | 3 | Addition's inverse |
| * | Multiplication | 3 | Repeated addition |
| / | Division | 4 | Multiplication's inverse, harder |
| ^ | Power | 5 | Repeated multiplication |
| ᵃ√b | Root | 6 | Inverse of power, more notation |
| log_a(b) | Logarithm base | 7 | Two transcendental operations |
| atan2(a,b) | Two-arg arctangent | 7 | Coordinate to angle |

## Calibration Methodology

### Historical Basis

Weights were calibrated against the original C RIES implementation to ensure
similar output ordering for common inputs like π, e, √2, etc.

### Intuitive Principles

1. **Fundamental concepts are cheap**: 1, 2, +, -, * have the lowest weights
2. **Transcendental > Algebraic > Rational**: Transcendental operations cost more
3. **Common usage**: sin(πx) is cheaper than W(x) because it's more commonly taught
4. **Notation complexity**: log_a(b) costs more than ln() due to two arguments

### Practical Testing

The weight system is validated by:

1. **Ordering tests**: Known "simple" equations should appear before "complex" ones
2. **Golden ratio consistency**: φ equations should have similar complexity to √2 equations
3. **User expectations**: Humans should agree that lower-weight equations are simpler

## Example Complexity Calculations

### Simple Equations

```
x = 2           [x, 2]              = 6 + 3 = 9
x + 1 = 3       [x, 1, +, 3]        = 6 + 3 + 3 + 4 = 16
2x = 4          [2, x, *, 4]        = 3 + 6 + 3 + 4 = 16
x² = 4          [x, s, 4]           = 6 + 5 + 4 = 15
```

### Moderate Equations

```
x² = π          [x, s, p]           = 6 + 5 + 8 = 19
√x = 2          [x, q, 2]           = 6 + 6 + 3 = 15
e^x = 10        [x, E, 1, 0]        = 6 + 8 = 14  (10 is two symbols: 1, 0)
x/2 = π         [x, 2, /, p]        = 6 + 3 + 4 + 8 = 21
```

### Complex Equations

```
x^x = π²        [x, x, ^, p, s]     = 6 + 6 + 5 + 8 + 5 = 30
ln(x) = γ       [x, l, g]           = 6 + 8 + 10 = 24
sin(πx) = 0     [x, S, 0]           = 6 + 9 = 15  (0 not available, use x-x)
W(x) = 1        [x, W, 1]           = 6 + 12 + 3 = 21
```

## Weight Adjustment Guidelines

When adding new symbols or adjusting weights:

1. **Preserve ordering**: Ensure existing equation orderings remain stable
2. **Test against known values**: Verify π, e, √2 produce expected results
3. **Consider user profiles**: Custom weights via `.ries` profiles allow overrides
4. **Document rationale**: Add comments explaining weight choices

## Relationship to Search Depth

The `--level` or `-l` option in RIES controls maximum complexity:

| Level | Default max complexity |
|-------|------------------------|
| 1     | ~80                    |
| 2     | ~120                   |
| 3     | ~160                   |
| ...   | +40 per level          |

Higher levels find more equations but take exponentially longer.

## Implementation Notes

- Weights are `u16` values to allow fine-grained control
- The `Symbol::weight()` method is `const` for compile-time optimization
- User profiles can override weights via `--symbol-weights :p:25 :e:30`
