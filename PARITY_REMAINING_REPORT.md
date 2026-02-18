# RIES-RS Parity: Remaining Gaps

Date: 2026-02-17 (Updated)
Scope: `/Users/maxwell/Apps/ries/ries/ries-rs` vs `/Users/maxwell/Apps/ries/ries/ries-original/ries` and `ries.1`.

## 1. Executive Summary

`ries-rs` now has **full P0 CLI parity** and **near-complete P1 parity** with the original RIES.

**Completed in earlier session (2026-02-17) - P0:**
- ✅ `-s` solve-for-x no longer shows misleading output
- ✅ `-p` optional-value parsing fixed (detects numeric target)
- ✅ `-l` Liouvillian vs level disambiguation implemented
- ✅ `-i` fallback to `-r` with warning implemented
- ✅ `--ie` and `--re` exact-mode flags added
- ✅ `-S` bare symbol table mode implemented
- ✅ `-E` bare enable-all mode implemented

**Completed in this session (2026-02-17) - P1:**
- ✅ `-F1` condensed format implemented (alias for `-F0`)
- ✅ `--verbose` flag with header/footer output
- ✅ `-Do` match checks diagnostic
- ✅ `-Dn` Newton iteration diagnostic
- ✅ `-DA/-Da` pruned arithmetic diagnostic
- ✅ `-DB/-Db` pruned range diagnostic
- ✅ `-DG/-Dg` database adds diagnostic
- ✅ `--match-all-digits` option (stricter matching)
- ✅ `--derivative-margin` option (Newton threshold)
- ✅ Comparison script for debugging parity

**Remaining gaps:**
- Additional diagnostic channels (C/c, D/d, E/e, F/f, H/h, I/i, J/j, K/k, L/l, etc.)
- Some no-op compatibility options
- Ranking/weight tuning for result ordering parity
- Report-mode format parity (-F not propagated to report output)
- Full `-s` algebraic transformation (current: safe equation form)

---

## 2. P0: Correctness/Compatibility Breaks - ALL RESOLVED

### 2.1 `-s` solve-for-x output

Status: **RESOLVED** (safe-disabled)

The `-s` flag no longer produces mathematically misleading `x = RHS` output. Instead, it shows the equation form `LHS = RHS` which is always correct. Full algebraic transformation (like original RIES) remains a future enhancement.

### 2.2 `-p` optional-value parsing

Status: **RESOLVED**

`-p 2.5` now correctly interprets `2.5` as the target value (not a profile filename). Numeric detection logic added.

### 2.3 `-l` legacy dual semantics

Status: **RESOLVED**

`-l 2.5` now enables Liouvillian mode with target 2.5. Use `-l3` or `--level 3` for explicit level setting.

### 2.4 `-i` fallback to `-r`

Status: **RESOLVED**

`-i 2.5` now prints the warning "Replacing -i with -r because target isn't an integer." and falls back to rational mode.

### 2.5 `-ie` and `-re` exact-mode

Status: **RESOLVED**

Implemented as `--ie` and `--re` flags (clap doesn't support compound short options like `-ie`). Both enable their respective mode AND set `stop_at_exact=true`.

---

## 3. P1: Major Feature Parity Gaps

### 3.1 `-S` bare symbol table mode

Status: **RESOLVED**

`-S` without argument now prints the full symbol table and exits.

### 3.2 `-E` bare enable-all mode

Status: **RESOLVED**

`-E 2.5` now enables all symbols and treats `2.5` as the target. Bare `-E` with explicit target also works.

### 3.3 `-F` format modes

Status: **RESOLVED**

All format modes now implemented:
- `-F0` compact postfix output
- `-F1` condensed format (alias for `-F0`)
- `-F2` default infix output
- `-F3` verbose postfix output

Note: Report mode (`--report true`) still uses infix format regardless of `-F` setting.

### 3.4 Diagnostics coverage (`-D*`)

Status: **partial**

**Implemented with output:**
- `-Ds` / `--show-work` -> step breakdown output
- `-Dy` -> stats output
- `-Do` -> match checks diagnostic output
- `-Dn` -> Newton iteration diagnostic output
- `-DA` / `-Da` -> expressions pruned (arithmetic errors)
- `-DB` / `-Db` -> expressions pruned (zero/out-of-range)
- `-DG` / `-Dg` -> expressions added to database

**Still unrecognized:**
- Most other channels (`C/c`, `D/d`, `E/e`, `F/f`, `H/h`, `I/i`, `J/j`, `K/k`, `L/l`, etc.)

### 3.5 Output detail (--verbose)

Status: **RESOLVED**

`--verbose` flag now shows:
- Header with target value and level
- Footer with summary statistics (total expressions tested, LHS/RHS counts, search time)

### 3.6 No-op options still accepted but not functional

Status: **not parity (surface only)**

Still no-op in `/Users/maxwell/Apps/ries/ries/ries-rs/src/main.rs`:
- `--any-exponents`
- `--any-subexpressions`
- `--any-trig-args`
- `--canon-reduction`
- `--canon-simplify`
- `--max-memory`
- `--memory-abort-threshold`
- `--max-trig-cycles`
- `--min-memory`
- `--no-canon-simplify`
- `--no-slow-messages`
- `--numeric-anagram`
- `--rational-exponents`
- `--rational-trig-args`
- `--significance-loss-margin`
- `--trig-argument-scale`

**Now implemented (removed from no-op list):**
- `--match-all-digits` - stricter matching based on target's significant digits
- `--derivative-margin` - configurable Newton-Raphson derivative threshold

---

## 4. P2: Behavioral Divergence (Quality/Parity)

### 4.1 Result ordering/content still differs from original

Status: **diverged**

Repro comparison:
```bash
./tests/compare_with_original.sh 2.5063 2 6
```

Observed:
- First-page equations are materially different.
- Complexity numbers differ significantly (different scale/calibration and expression ordering).

This is not a parser issue; it is search/ranking/weighting behavior divergence.

### 4.2 Output detail parity gaps

Status: **RESOLVED** (via --verbose)

Original's legend/explanatory lines and statistics footer are now available via `--verbose` flag.

### 4.3 Report-mode format parity

Status: **partial**

Classic mode (`--report false`) honors `-F0/-F1/-F3`. Report mode still renders infix via `src/report.rs` regardless of `-F` setting.

### 4.4 `-s` algebraic transformation

Status: **safe but incomplete**

Current behavior avoids fake `x = RHS` output (correct), but does not replicate original RIES's algebraic "solve for x" transformation.

---

## 5. Test Coverage

### 5.1 CLI Regression Tests - Comprehensive

Location: `/Users/maxwell/Apps/ries/ries/ries-rs/tests/cli_regression_tests.rs`

**Tests added in earlier session:**
- `test_s_flag_shows_equation_form_not_misleading_x_equals`
- `test_s_flag_without_complex_lhs_works_correctly`
- `test_p_flag_without_file_accepts_target`
- `test_l_flag_liouvillian_mode`
- `test_level_flag_with_integer`
- `test_i_flag_fallback_to_r`
- `test_ie_integer_exact_mode`
- `test_re_rational_exact_mode`
- `test_s_bare_symbol_table`
- `test_e_bare_enable_all`

**Tests added in this session:**
- `test_f1_condensed_format_accepted`
- `test_verbose_output_shows_target`
- `test_verbose_output_shows_total_equations`
- `test_diagnostic_channel_o_recognized`
- `test_diagnostic_o_shows_match_output`
- `test_diagnostic_n_shows_newton_iterations`
- `test_diagnostic_a_recognized`
- `test_diagnostic_b_recognized`
- `test_diagnostic_g_recognized`
- `test_diagnostic_g_shows_db_add_output`
- `test_derivative_margin_option_accepted`
- `test_match_all_digits_option_accepted`

### 5.2 Comparison Script

Location: `/Users/maxwell/Apps/ries/ries/ries-rs/tests/compare_with_original.sh`

Usage:
```bash
./tests/compare_with_original.sh [target] [level] [max_matches]
```

---

## 6. Recommended Next Steps

1. **P2**: Tune ranking/weights vs original benchmark set
2. **P2**: Investigate complexity score calibration
3. **P2**: Unify format handling in report mode
4. **P2**: Implement remaining no-op options (`canon-*`, `rational-*`)
5. **P2**: Implement remaining `-D` channels if needed for debugging

---

## 7. Definition of "Meets or Exceeds"

For parity signoff:
- ~~CLI compatibility: parse + semantics + output mode for legacy options.~~ **DONE**
- ~~Correctness: no mathematically misleading transformations (`-s`).~~ **DONE**
- ~~Regression guardrails: every fixed item covered by CLI/integration tests.~~ **DONE**
- ~~Output detail: verbose mode with header/footer.~~ **DONE**
- ~~Diagnostic channels: core channels implemented (s, y, o, n, A/a, B/b, G/g).~~ **DONE**
- ~~Key options: --match-all-digits, --derivative-margin working.~~ **DONE**
- Behavioral similarity: representative benchmark targets produce comparably plausible first-page equations. **REMAINING**
