# Full Parity Implementation Design

> Historical planning document: paths, repository metadata, and license snippets in this file reflect its drafting date and may be outdated. Current source-of-truth is `Cargo.toml`, `README.md`, and `src/`.

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Achieve full parity between ries-rs and original RIES across output formatting, diagnostics, algorithm options, and search quality.

**Architecture:** Layer-by-layer approach - each layer is independent and produces a testable milestone. Output surface first (safest), then diagnostics, then algorithm options, finally search quality tuning.

**Tech Stack:** Rust, clap for CLI, existing ries-rs architecture (expr.rs, search.rs, pool.rs, main.rs)

---

## Layer 1: Output Surface

### 1.1 `-F1` Condensed Format

**Status:** Missing

Original RIES defines `-F1` (OF_CONDENSED) but it's minimally documented and rarely used.

**Design:**
- Treat `-F1` as an alias for `-F0` (postfix compact)
- Update `DisplayFormat` enum to include `Condensed` variant
- Map `-F1` to `Condensed` which renders identically to `PostfixCompact`

**Files:**
- `src/main.rs` - DisplayFormat enum and parsing

### 1.2 Output Detail (Legend, Statistics Footer)

**Status:** Missing

Original RIES outputs include:
- Header with target value and active symbols
- Result lines with complexity scores
- Footer with "Total equations tested" count

**Design:**
- Add `print_header()` function for target/settings summary
- Add `print_footer()` function for statistics
- Gate behind `--classic` mode or explicit `--verbose` flag
- Extract stats from search results

**Output format:**
```
Target: 2.5063

x = sqrt(2 pi)  (complexity 12)
x = pi/2 + 1    (complexity 14)
...

Total equations tested: 1,234,567
```

**Files:**
- `src/main.rs` - Header/footer printing functions

---

## Layer 2: Diagnostics (-D Channels)

### 2.1 Current State

**Implemented:**
- `-Ds` / `--show-work` → step breakdown (partial, only with `--report false`)
- `-Dy` / `--stats` → stats output

**Missing:** All other channels warn but don't act

### 2.2 Priority Channels

| Channel | Purpose | Output Format |
|---------|---------|---------------|
| `s/N` | Show work (subexpression values) | Infix (`-F2`) |
| `y` | Stats/decisions in main loop | Text |
| `M` | Memory/timing benchmarks | Text |
| `A/a` | Expressions pruned (arithmetic errors) | Postfix (`-F0`) |
| `B/b` | Expressions pruned (zero/out-of-range) | Postfix (`-F0`) |
| `G/g` | Expressions added to database | Postfix (`-F0`) |
| `o` | Match checks | Text |
| `n` | Newton iteration values | Text |

### 2.3 Implementation

**DiagnosticOutput struct:**
```rust
struct DiagnosticOutput {
    show_work: bool,           // s, N
    show_stats: bool,          // y, M
    show_pruned_arith: bool,   // A, a
    show_pruned_range: bool,   // B, b
    show_db_adds: bool,        // G, g
    show_match_checks: bool,   // o
    show_newton: bool,         // n
    format: DisplayFormat,     // Postfix for most, Infix for s
}
```

**Integration points:**
- Pruning: `src/search.rs` → emit `A/a`, `B/b` diagnostics
- Database: `src/pool.rs` → emit `G/g` diagnostics
- Matching: `src/search.rs` → emit `o` diagnostics
- Newton: evaluation code → emit `n` diagnostics

**Files:**
- `src/main.rs` - DiagnosticOutput struct and parsing
- `src/search.rs` - Diagnostic emission at pruning/matching points
- `src/pool.rs` - Diagnostic emission at database operations

---

## Layer 3: Algorithm Options (No-ops → Real Behavior)

### 3.1 Phase 3a - Quick Wins

**`--match-all-digits`**
- Purpose: Require all significant digits to match target
- Implementation: Compare against target string representation
- Files: `src/search.rs` - match validation

**`--derivative-margin`**
- Purpose: Threshold for derivative checking
- Implementation: Pass threshold to derivative calculation
- Files: `src/precision.rs` or evaluation code

**`--significance-loss-margin`**
- Purpose: Precision loss threshold
- Implementation: Threshold for precision tracking
- Files: `src/precision.rs`

### 3.2 Phase 3b - Canonical Forms

**`--canon-simplify` / `--no-canon-simplify`**
- Purpose: Control expression normalization
- Implementation: Toggle in pool.rs deduplication logic
- Files: `src/pool.rs`

**`--canon-reduction`**
- Purpose: Reduction rules for canonical forms
- Implementation: Additional normalization passes
- Files: `src/pool.rs`, possibly `src/expr.rs`

### 3.3 Phase 3c - Constraint Options

**`--rational-exponents`**
- Purpose: Constrain exponents to rational numbers
- Implementation: Filter during expression generation
- Files: `src/gen.rs`

**`--rational-trig-args`**
- Purpose: Constrain trig arguments to rational multiples of π
- Implementation: Filter during expression generation
- Files: `src/gen.rs`

---

## Layer 4: Search Quality (Ranking/Weight Tuning)

### 4.1 Problem Statement

First-page equations differ from original RIES:
- Different expressions found
- Different complexity scores
- Different ordering

### 4.2 Investigation Phases

**Phase 4a - Diagnostic Comparison**
- Run both tools with same inputs and `-D` channels
- Compare expression generation, pruning, scoring
- Identify divergence layer(s)

**Phase 4b - Complexity Calibration**
- Compare complexity scores for identical expressions
- Adjust weight constants in `src/symbol.rs`, `src/expr.rs`
- Match original's complexity formula

**Phase 4c - Search Order**
- Compare expression generation order
- Adjust BFS traversal if needed
- Match original's exploration strategy

### 4.3 Success Criteria

For benchmark targets (π, e, √2, 2.5063):
- First 5-10 results overlap significantly with original
- Complexity scores within 10% of original
- Total equations tested within same order of magnitude

---

## Implementation Order

1. **Layer 1** - Output surface (3-4 tasks)
2. **Layer 2** - Diagnostics (6-8 tasks)
3. **Layer 3a** - Quick-win options (3 tasks)
4. **Layer 4a** - Diagnostic comparison (2-3 tasks)
5. **Layer 4b** - Complexity calibration (2-4 tasks)
6. **Layer 3b/3c** - Deeper options (4-6 tasks, optional)
7. **Layer 4c** - Search order (2-4 tasks, if needed)

---

## Files Modified

| File | Layers | Changes |
|------|--------|---------|
| `src/main.rs` | 1, 2 | DisplayFormat, DiagnosticOutput, header/footer |
| `src/search.rs` | 2, 3a, 4 | Diagnostics, options, tuning |
| `src/pool.rs` | 2, 3b | Diagnostics, canonical forms |
| `src/gen.rs` | 3c | Constraint filtering |
| `src/precision.rs` | 3a | Margin thresholds |
| `src/symbol.rs` | 4b | Weight constants |
| `src/expr.rs` | 4b | Complexity calculation |

---

## Testing Strategy

- CLI regression tests for each new flag/option
- Integration tests comparing output format
- Benchmark tests for search quality validation
- Comparison script: `tests/compare_with_original.sh`
