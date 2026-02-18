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

### Python Bindings

The Python bindings allow using ries-rs from Python code:

```bash
# Install maturin (Python package builder for Rust)
pip install maturin

# Build and install the Python package
maturin develop --features python

# Or build a wheel for distribution
maturin build --features python --release
```

#### Python Usage

```python
import ries_rs

# Simple search
results = ries_rs.search(3.1415926535)
for r in results:
    print(f"{r.lhs} = {r.rhs}  (error: {r.error:.2e})")

# With options
results = ries_rs.search(
    1.618033988,      # Golden ratio
    level=3,          # Higher search level (more results)
    max_matches=20,   # Maximum matches to return
    preset="physics", # Domain-specific preset
    parallel=True     # Use parallel search
)

# Access match properties
for m in results:
    print(f"LHS: {m.lhs}")          # Expression with x
    print(f"RHS: {m.rhs}")          # Constants only
    print(f"x value: {m.x_value}")  # Solved value of x
    print(f"Error: {m.error}")      # x_value - target
    print(f"Complexity: {m.complexity}")
    print(f"Is exact: {m.is_exact}")
    print(f"As dict: {m.to_dict()}")

# List available presets
presets = ries_rs.list_presets()
for name, desc in presets.items():
    print(f"{name}: {desc}")

# Get version
print(ries_rs.version())
```

#### search() Parameters

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `target` | float | required | The target value to find equations for |
| `level` | int | 2 | Search depth (0-5). Higher = more expressions searched |
| `max_matches` | int | 16 | Maximum number of matches to return |
| `preset` | str | None | Domain preset: "analytic-nt", "elliptic", "combinatorics", "physics", "number-theory", "calculus" |
| `parallel` | bool | True | Use parallel search (recommended) |

#### PyMatch Properties

| Property | Type | Description |
|----------|------|-------------|
| `lhs` | str | Left-hand side expression (contains x) |
| `rhs` | str | Right-hand side expression (constants only) |
| `lhs_postfix` | str | Postfix representation of LHS |
| `rhs_postfix` | str | Postfix representation of RHS |
| `x_value` | float | Solved value of x |
| `error` | float | Error (x_value - target) |
| `complexity` | int | Complexity score (lower = simpler) |
| `is_exact` | bool | True if error < 1e-14 |

#### PyMatch Methods

| Method | Returns | Description |
|--------|---------|-------------|
| `to_dict()` | dict | Convert match to a Python dictionary |
| `__repr__()` | str | Developer-friendly representation |
| `__str__()` | str | Human-readable string with equation and error |

#### Python Examples

**Find equations for mathematical constants:**
```python
import ries_rs

# Find equations for π
for m in ries_rs.search(3.141592653589793, level=2):
    print(f"{m.lhs} = {m.rhs}  [{m.error:.2e}]")

# Find equations for Euler's number
for m in ries_rs.search(2.718281828459045, level=3):
    print(m)
```

**Use domain presets for targeted searches:**
```python
# Physics preset includes common physical constants
results = ries_rs.search(137.035999, preset="physics", level=2)

# Number theory preset focuses on integer/rational relationships
results = ries_rs.search(2.678938534707747, preset="number-theory")
```

**Export results for analysis:**
```python
import json

results = ries_rs.search(1.618033988749895)  # Golden ratio

# Convert to list of dicts for JSON export
data = [m.to_dict() for m in results]
print(json.dumps(data, indent=2))
```

**Filter results by quality:**
```python
results = ries_rs.search(2.5066282746310002, level=4, max_matches=50)

# Get only exact matches
exact = [m for m in results if m.is_exact]

# Get matches with error below threshold
good = [m for m in results if abs(m.error) < 1e-10]

# Get simplest matches
simplest = sorted(results, key=lambda m: m.complexity)[:10]
```

#### Troubleshooting

**ImportError: cannot import name 'ries_rs'**
- Make sure you built with maturin: `maturin develop --features python`
- Check that you're using the same Python environment where maturin was run

**Build fails with linking errors**
- Ensure Python development headers are installed:
  - Ubuntu/Debian: `sudo apt install python3-dev`
  - macOS: `xcode-select --install`
  - Windows: Install Python from python.org (includes dev headers)

**Performance is slower than CLI**
- Make sure `parallel=True` (default)
- For batch processing, reuse results rather than calling search multiple times

### WebAssembly (WASM) Bindings

The WASM bindings allow using ries-rs from JavaScript/TypeScript in browsers and Node.js:

```bash
# Install wasm-pack
cargo install wasm-pack

# Build for web browsers
npm run build

# Or build for different targets:
npm run build:bundler  # For bundlers (webpack, vite, etc.)
npm run build:node     # For Node.js
```

#### JavaScript/TypeScript Usage

```javascript
import init, { search, WasmMatch, listPresets, version } from 'ries-rs';

// Initialize the WASM module
await init();

// Simple search
const results = search(3.1415926535);
for (const m of results) {
  console.log(`${m.lhs} = ${m.rhs} (error: ${m.error.toExponential(2)})`);
}

// With options
const results = search(1.618033988, {
  level: 3,
  maxMatches: 20,
  preset: 'physics'
});

// Access match properties
for (const m of results) {
  console.log(m.lhs);         // "x"
  console.log(m.rhs);         // "phi+1"
  console.log(m.x_value);     // 1.618033988...
  console.log(m.error);       // 5.55e-10
  console.log(m.complexity);  // 16
  console.log(m.is_exact);    // true
  console.log(m.to_string()); // "x = phi+1  [error: 5.55e-10] {16}"
  console.log(m.to_json());   // JSON object
}

// List presets
const presets = listPresets();
console.log(presets); // {analytic-nt: "...", physics: "...", ...}

// Get version
console.log(version()); // "0.1.0"
```

#### WasmMatch Properties

| Property | Type | Description |
|----------|------|-------------|
| `lhs` | string | Left-hand side expression (contains x) |
| `rhs` | string | Right-hand side expression (constants only) |
| `lhs_postfix` | string | Postfix representation of LHS |
| `rhs_postfix` | string | Postfix representation of RHS |
| `x_value` | number | Solved value of x |
| `error` | number | Error (x_value - target) |
| `complexity` | number | Complexity score (lower = simpler) |
| `is_exact` | boolean | True if error < 1e-14 |

#### SearchOptions

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `level` | number | 2 | Search depth (0-5) |
| `maxMatches` | number | 16 | Maximum matches to return |
| `preset` | string | null | Domain preset name |

#### Browser Example

```html
<!DOCTYPE html>
<html>
<head>
  <script type="module">
    import init, { search } from './pkg/ries_rs.js';

    async function run() {
      await init();

      const results = search(3.14159, { level: 2, maxMatches: 5 });
      for (const m of results) {
        document.body.innerHTML += `<p>${m.to_string()}</p>`;
      }
    }

    run();
  </script>
</head>
<body></body>
</html>
```

#### Node.js Example

```javascript
const { search, listPresets } = require('ries-rs');

const results = search(2.718281828, { level: 3 });
console.log(`Found ${results.length} matches`);
```

### PSLQ Integer Relation Detection

RIES-RS includes PSLQ (Partial Sums LQ) algorithm for finding integer relations:

```bash
# Find rational approximation for π
ries-rs 3.141592653589793 --pslq
# Output: 355 / 113 = 3.141592920353983  (error: 2.67e-7)

# Use extended constant set (includes √3, √5, ln(3), etc.)
ries-rs 2.718281828459045 --pslq --pslq-extended

# Increase coefficient bounds for deeper search
ries-rs 1.41421356 --pslq --pslq-max-coeff 10000
```

#### PSLQ Options

| Option | Default | Description |
|--------|---------|-------------|
| `--pslq` | off | Enable PSLQ integer relation detection |
| `--pslq-extended` | off | Use extended constant set |
| `--pslq-max-coeff` | 1000 | Maximum coefficient magnitude to search |

#### What PSLQ Finds

- **Rational approximations**: π ≈ 355/113, √2 ≈ 99/70
- **Integer relations**: Finds a·x + b·π + c·e + ... ≈ 0 with small integers a, b, c
- **Minimal polynomials**: Detects algebraic numbers by their defining polynomial

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

### Zenodo DOI

[![DOI](https://img.shields.io/badge/DOI-10.5281/zenodo.XXXXXXX-blue)](https://doi.org/10.5281/zenodo.XXXXXXX)

Each release is automatically archived on [Zenodo](https://zenodo.org) with a persistent DOI for citation. To cite a specific version:

1. Go to the [Zenodo record](https://zenodo.org/doi/10.5281/zenodo.XXXXXXX)
2. Select the version you used
3. Export the citation in your preferred format

### Reproducibility

For reproducibility in publications, use `--deterministic` and `--emit-manifest` flags:

```bash
ries-rs --deterministic --emit-manifest manifest.json 3.141592653589793
```

Include the generated `manifest.json` as supplementary material with your publication.

## License

MIT License. See `LICENSE`.

## References

- [Original RIES](https://mrob.com/pub/ries/) by Robert Munafo
- [RIES Documentation](https://mrob.com/pub/ries/ries.html)
- [clsn/ries fork](https://github.com/clsn/ries)
- Stoutemyer, D.R. (2024). "Computing with No Machine Constants, Only Constructive Axioms". arXiv:2402.03304
