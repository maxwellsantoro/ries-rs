# RIES-RS Web Interface v2.0 Design

**Date:** 2026-02-24
**Author:** Design brainstorming session
**Status:** Approved

---

## Overview

Modern, user-friendly web interface for RIES-RS that surpasses the original mrob.com RIES page in every dimension. Single-file application with Tailwind CSS and KaTeX math rendering, designed for both general curious users and domain experts through progressive disclosure.

---

## Goals

1. **Modern UX**: Delightful, responsive interface with instant feedback
2. **Mixed Audience**: Simple defaults for beginners, advanced options for experts
3. **Beautiful Math**: KaTeX rendering for LaTeX-quality equations
4. **Zero Friction**: Single-file deployment, no build step beyond WASM
5. **Shareable**: URL state syncs all parameters for easy sharing

---

## Architecture

### Single-File Approach

```
web/
├── index.html          ← Complete app (~400 lines)
└── README.md           ← Build/serve instructions
```

**Key decisions:**
- **No build step** — Tailwind via CDN, KaTeX via CDN
- **Vanilla JS** — No framework overhead, direct WASM integration
- **Self-contained** — Single HTML file after WASM build
- **URL state sync** — All options encoded in query params

---

## UI Layout

```
┌─────────────────────────────────────────────────┐
│ Header: RIES-RS • Find equations for any number│
│         [GitHub] [Theme Toggle]                 │
├─────────────────────────────────────────────────┤
│ Search Card:                                    │
│   ┌──────────────────────────────────────────┐  │
│   │ [Large Target Input]        [Search Btn] │  │
│   └──────────────────────────────────────────┘  │
│                                                  │
│ Quick Constants: [π] [e] [φ] [√2] [+ More]       │
│                                                  │
│ Controls: [Level 3 ▼] [Preset: Analytic ▼]      │
│           [▼ Advanced Options]                   │
├─────────────────────────────────────────────────┤
│ Results Area:                                    │
│   ┌──────────────────────────────────────────┐  │
│   │ x = π                    [Exact] [23]    │  │
│   │ [Copy] [LaTeX] [SymPy] [▼ Details]       │  │
│   └──────────────────────────────────────────┘  │
│   ... (more results)                            │
├─────────────────────────────────────────────────┤
│ Footer: v0.1.0 • Ready (4 workers)              │
└─────────────────────────────────────────────────┘
```

---

## Color System

### Dark Mode (Default)
| Role | Tailwind | Usage |
|------|---------|-------|
| Background | `zinc-950` | Page background |
| Surface | `zinc-900` | Cards, inputs |
| Border | `zinc-800` | Subtle borders |
| Text Primary | `zinc-100` | Main text |
| Text Muted | `zinc-500` | Secondary text |
| Accent | `emerald-500` | CTAs, focus |

### Light Mode
| Role | Tailwind |
|------|---------|
| Background | `stone-50` |
| Surface | `white` |
| Text Primary | `zinc-900` |

---

## Interactions

| Interaction | Behavior |
|-------------|----------|
| Type in target | Debounced search after 250ms (optional) |
| Click constant | Immediate search with that value |
| Press Enter | Triggers search |
| Click "Advanced" | Smooth expand/collapse with chevron |
| Hover result | Border glow + shadow increase |
| Click copy | Button → "Copied!" for 1.5s |
| Keyboard `/` | Focuses target input |
| Keyboard `Esc` | Clears results |

---

## Advanced Options Panel

Collapsible panel with:

- **Ranking Mode**: Complexity (default) / Parity
- **PSLQ**: Integer relation detection toggle
- **Match All Digits**: Stricter matching
- **Derivative Margin**: Number input (1e-12)
- **Output Options**: LaTeX preview, postfix notation
- **Solve for X**: Variable name (default: x)

All options persist in URL params.

---

## Famous Constants Presets

Quick-access buttons for:

| Symbol | Value | Label |
|--------|-------|-------|
| π | 3.141592653589793 | Pi |
| e | 2.718281828459045 | Euler's number |
| φ | 1.618033988749895 | Golden ratio |
| √2 | 1.414213562373095 | Square root of 2 |
| γ | 0.577215664901532 | Euler–Mascheroni |
| c | 299792458 | Speed of light (m/s) |
| h | 6.62607015e-34 | Planck constant |
| G | 6.67430e-11 | Gravitational constant |

Math constants: purple tint; Physics constants: blue tint.

---

## Accessibility

- ARIA labels on all interactive elements
- Full keyboard navigation
- Focus indicators (ring-2 ring-emerald-500)
- `aria-live` regions for status updates
- WCAG AA contrast ratios
- Semantic HTML with landmarks

---

## Responsive Breakpoints

| Size | Layout |
|------|--------|
| Mobile (< 640px) | Stacked controls, 4 constants visible |
| Tablet (640-1024px) | 2-column controls, 8 constants |
| Desktop (> 1024px) | Max-width 1280px, 3-column advanced |

---

## URL State Management

```javascript
?target=3.14159&level=3&preset=analytic-nt&maxMatches=20&advanced=true&ranking=complexity
```

- Updates on every interaction (no page reload)
- Full state restoration on page load
- Auto-search if target present

---

## Code Organization (Within index.html)

```javascript
// Section 1: Configuration & State (~40 lines)
const FAMOUS_CONSTANTS = [...];
let appState = { target, level, preset, ... };

// Section 2: WASM Loading (~50 lines)
async function initWasm() { ... }

// Section 3: UI Rendering (~80 lines)
function renderPresets() { ... }
function renderResults(matches) { ... }
function renderAdvancedPanel() { ... }

// Section 4: Search Logic (~40 lines)
async function performSearch() { ... }
function debounceSearch() { ... }

// Section 5: Event Handlers (~30 lines)
function handleSubmit() { ... }
function copyToClipboard() { ... }

// Section 6: URL State Management (~30 lines)
function updateURL() { ... }
function loadFromURL() { ... }

// Section 7: Utilities (~30 lines)
function formatError() { ... }
function toggleTheme() { ... }
```

---

## Technical Dependencies

| Resource | Version | Purpose |
|----------|---------|---------|
| Tailwind CSS | 3.x CDN | Styling |
| KaTeX | 0.16.x CDN | Math rendering |
| RIES-RS WASM | local | Core search functionality |

---

## Implementation Checklist

- [ ] Single-file HTML structure with Tailwind CDN
- [ ] KaTeX integration for equation rendering
- [ ] WASM loading with thread detection
- [ ] Famous constants quick buttons
- [ ] Collapsible advanced options panel
- [ ] URL state sync (read/write)
- [ ] Dark/light theme toggle
- [ ] Copy to clipboard (plain/LaTeX/SymPy)
- [ ] Responsive design (mobile/tablet/desktop)
- [ ] Keyboard navigation
- [ ] Accessibility features (ARIA, focus, etc.)
- [ ] Loading and error states
- [ ] Update README with build/serve instructions

---

## Success Criteria

1. **Functional**: All search options from CLI available in UI
2. **Fast**: Searches complete in < 100ms for typical inputs
3. **Beautiful**: KaTeX rendering, smooth animations
4. **Accessible**: WCAG AA compliant, fully keyboard navigable
5. **Shareable**: Every search has a unique, restorable URL
6. **Mobile**: Fully functional on phones and tablets

---

## Future Enhancements (Post-v1.0)

- Search history (localStorage)
- "Surprise me" button with random constants
- Export all results as JSON/CSV
- PWA install banner
- Performance stats display
- Custom user constants (via profile upload)
