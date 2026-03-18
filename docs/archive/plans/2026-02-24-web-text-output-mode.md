# Web Text Output Mode

**Date:** 2026-02-24
**Status:** Approved
**Scope:** `web/index.html` only — no WASM rebuild

## Problem

The web UI shows results as individual cards with KaTeX rendering. This is good for
reading but makes it hard to copy, save, or pipe all results at once. There's also no
format that's easy to feed into other tools (scripts, editors, spreadsheets).

## Goal

Add a **Cards / Text** toggle to the results section. Text mode renders all results
in a single readable, grep-able plain-text block that can be copied or downloaded.
The format is the default output; future work can add CSV, Markdown, and other formats
via the same `formatResults()` abstraction.

## Design

### Toggle

A segmented `Cards | Text` control appears in the results section header once results
exist. Switching is instant (no re-search). The active mode persists in the URL as
`?view=text` / `?view=cards` (default: cards).

### Text Format

```
# RIES-RS v0.1.0 — target: 3.141592653589793 (level 3, 3 results)
x = pi          error=0.00e+0   complexity=29
-x = -pi        error=0.00e+0   complexity=43
1/x = 1/pi      error=0.00e+0   complexity=43
```

Rules:
- **ASCII names** — `pi`, `phi`, `sqrt(...)`, not Unicode symbols. Plain text is meant
  for terminals and editors, not display.
- **Tab-aligned columns** — equation left-padded to a fixed width, then `error=` and
  `complexity=` fields separated by tabs so `cut`, `awk`, etc. work naturally.
- **Header comment line** — prefixed with `#`, includes version, target, level, and
  match count for provenance when the text is saved or shared.

### Text Box

A `<textarea readonly>` — not `<pre>` — so users can triple-click to select all,
use the browser's built-in copy, or use the **Copy all** button above it. A
**Download .txt** button saves the text as a file using a data URL.

### Extensibility

The text generation lives in a single `formatResults(matches, format)` function.
`format` defaults to `'text'`; future values like `'csv'` and `'markdown'` just add
cases to that function. The toggle UI can grow into a dropdown when more formats exist.

## Implementation

All changes are in `web/index.html`:

1. Add `formatResults(matches, format)` function that produces the text string.
2. Add `renderTextOutput(matches)` function that populates the textarea and wires
   Copy all / Download buttons.
3. Add `Cards | Text` toggle buttons to the results section header HTML.
4. Wire toggle click handlers: show/hide card container vs. text container, update URL.
5. Update `loadFromURL()` to restore `?view=` param and add it to the `updateURL()` call.
6. Update smoke test to verify the toggle appears after search and that text output
   contains the header line and at least one equation.

## Non-Goals

- No WASM rebuild.
- No CSV, Markdown, or other formats in this iteration.
- No server-side rendering or file persistence.
- The text format does not attempt to replicate the exact CLI column alignment
  (that comes later with the CLI parity work).
