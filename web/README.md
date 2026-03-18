# Web Interface

`web/index.html` is the repository's browser UI for the WASM build. It is a
static client-side application that loads the generated `pkg/` bundle, renders
results with KaTeX, and supports shareable URLs plus a deployable static-site
bundle.

## Current Capabilities

- in-browser search against the WASM engine
- quick constant buttons for built-in constants
- ranking-mode and match-precision controls
- cards/text result views with copy and download actions
- shareable URL parameters
- dark/light theme toggle

PSLQ remains CLI-only; the web UI exposes it only as an unsupported option with
guidance.

## Prerequisites

From the repository root:

```bash
npm install
rustup toolchain install nightly
```

The current local WASM build scripts use nightly `wasm-pack` via `-Z build-std`.

## Build WASM

From the **repository root**:

```bash
npm run build
```

This produces `pkg/ries_rs.js` and `pkg/ries_rs_bg.wasm`.

## Serve From The Repo

```bash
npx serve . -p 5000
```

Then open: **http://localhost:5000/web/**

This is the developer layout used inside the repository:

- `web/index.html`
- `pkg/ries_rs.js`
- `pkg/ries_rs_bg.wasm`

## Build A Deployable Static Site Bundle

If you want to host the app at a clean subpath such as
`https://example.com/projects/ries-rs/`, build the static bundle instead:

```bash
npm run build:web:site
```

This creates:

- `dist/web-site/index.html`
- `dist/web-site/pkg/ries_rs.js`
- `dist/web-site/pkg/ries_rs_bg.wasm`

Deploy the contents of `dist/web-site/` to the target directory on your site.
The bundle is subpath-safe and self-contained, so it can live at
`/projects/ries-rs/` instead of only `/web/` and does not need runtime CDN
access.

GitHub Releases provide the compiled WASM package tarball
(`ries-rs-wasm.tar.gz`). The static-site bundle itself is intended to be built
from the repository checkout.

## Test The Web UI

Build the static bundle and run the Playwright smoke tests:

```bash
npm run test:web:smoke:build
```

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

The UI restores state from URL parameters for shared links. Example:

```
?target=3.14159&level=3&preset=analytic-nt&max-matches=20&advanced=1
```
