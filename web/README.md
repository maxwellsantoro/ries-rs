# Web Interface

`web/index.html` is the repository's browser UI for the WASM build. It is a
static client-side application that loads the generated `pkg/` bundle, renders
results with KaTeX, and supports shareable URLs plus a deployable static-site
bundle.

## Current Capabilities

- in-browser search against the WASM engine
- quick constant buttons for built-in constants
- ranking-mode and match-precision controls (entered decimal digits, including trailing zeros, determine match precision)
- cards/text result views with copy and download actions
- LaTeX copy for rendered equations, including indexed roots, and SymPy copy
  with an exact expression for the plastic constant
- shareable URL parameters
- dark/light theme toggle

PSLQ remains CLI-only; the web UI exposes it only as an unsupported option with
guidance.

## Prerequisites

From the repository root:

```bash
npm ci
rustup toolchain install nightly
```

The current local WASM build scripts use nightly `wasm-pack` via `-Z build-std`.

## Build WASM

From the **repository root**:

```bash
npm run build
```

This produces `pkg/ries_rs.js` and `pkg/ries_rs_bg.wasm`.
Run `npm run build:css` after editing utility classes; the checked-in
`web/styles.css` is compiled with the pinned Tailwind dependency. The browser
loads static CSS and does not run a Tailwind compiler.

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
- `dist/web-site/styles.css`
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
npx playwright install chromium
npm run test:web:smoke:build
```

## Threaded Build (Optional)

For parallel search in browsers:

```bash
npm run build:threads
npm run build:web:site
```

Deploy `dist/web-site/` and open the page with `?threads=1`. The site
builder copies the threaded package when present. Without cross-origin
isolation the UI uses the regular package. Rebuild the threaded package after
code changes.

The build requires nightly Rust with `rust-src` and `wasm-pack`. Serve it
with these headers to enable `SharedArrayBuffer`.

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

CI runs the built browser suite, including a mobile viewport, and checks that
committed CSS matches a fresh build. Results explicitly explain the numerical
meaning of “exact”; they are not proof certificates.
