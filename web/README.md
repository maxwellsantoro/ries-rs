# RIES-RS Web Interface

Modern, user-friendly web interface for RIES-RS with beautiful math rendering, instant search, and progressive disclosure for beginners and experts.

## Features

- **Instant Search**: Find algebraic equations for any number in milliseconds
- **Beautiful Math**: KaTeX rendering for LaTeX-quality equations
- **Quick Constants**: One-click access to π, e, φ, and more
- **Advanced Options**: Ranking modes and other power-user controls (with clear web-only/CLI-only guidance)
- **Shareable Links**: Every search has a unique URL
- **Dark/Light Mode**: Toggle between themes with persistence
- **Copy Formats**: Export as plain text, LaTeX, or SymPy

Note: PSLQ is currently **CLI-only**; the web UI surfaces it as an unsupported option with guidance.

## Build WASM

From the **repository root**:

```bash
npm run build
```

This produces `pkg/ries_rs.js` and `pkg/ries_rs_bg.wasm`.

## Serve

```bash
npx serve . -p 5000
```

Then open: **http://localhost:5000/web/**

## Threaded Build (Optional)

For parallel search in browsers:

```bash
npm run build:threads
```

Requires nightly Rust and server headers for `SharedArrayBuffer`:

```
Cross-Origin-Opener-Policy: same-origin
Cross-Origin-Embedder-Policy: require-corp
```

## Keyboard Shortcuts

| Key | Action |
|-----|--------|
| `/` | Focus target input |
| `Esc` | Clear results / blur input |
| `Enter` | Submit search |

## URL Parameters

All options can be set via URL for sharing:

```
?target=3.14159&level=3&preset=analytic-nt&maxMatches=20&advanced=true
```
