# Web Interface

`web/index.html` is the repository's browser UI for the WASM build. It is a
static client-side application that loads the generated `pkg/` bundle, renders
results with KaTeX, and supports shareable URLs plus a deployable static-site
bundle.

## Current Capabilities

- precision-aware Discovery mode using the shared Rust policy, with a default
  limit of eight distinct hypotheses and explicit raw complexity/parity modes
- quick constant buttons for built-in constants
- ranking-mode and match-precision controls (entered decimal digits, including trailing zeros, determine match precision)
- cards/text views bound to the completed run, with copy and download actions
- original decimal literal, effective tolerance, retained supporting equations,
  and explicit numerical/verification availability
- full run JSON and selected-hypothesis exports for the independent checker
- LaTeX copy for rendered equations, including indexed roots and grouped trig arguments, and SymPy copy
  with an exact expression for the plastic constant
- shareable URL parameters
- dark/light theme toggle

PSLQ remains CLI-only; the web UI exposes it only as an unsupported option with
guidance. Discovery always uses the entered decimal precision. Match-all-digits
is an optional filter in raw modes, whose error-first ranking remains available.

## Completed runs and validation

A successful search stores the original target literal, captured control values,
engine version, results, and (in Discovery mode) the resolved discovery run.
Editing controls marks them as pending; cards, text, copy, and downloads continue
to describe that completed run, including edits made while its worker was running.
A failed later search preserves the last completed run with an explicit message.
A successful empty search replaces it with a visible zero-result run.

Use **Download run JSON** for the complete `ries-discovery-v1` object. **Export**
on a hypothesis saves the same full run plus the selected index. From a checkout:

```bash
python3 scripts/verify_discovery.py run.json --output evidence.json
python3 scripts/verify_discovery.py evidence.json --verify
```

Replace `run.json` with the downloaded filename. The checker can independently
bound roots only within its documented polynomial grammar and may use a retained
supporting equation. Unsupported checks report unavailable. Numerical agreement,
high-precision corroboration, and a bounded polynomial root are different claims;
none proves that a decimal's intended mathematical constant satisfies an equation.
Text exports are readable summaries; JSON retains the resolved grammar and full
evidence needed by the checker.

## Prerequisites

From the repository root:

```bash
npm ci
rustup toolchain install nightly --profile minimal --component rust-src --target wasm32-unknown-unknown
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

Use [VALIDATION.md](../docs/VALIDATION.md) to choose UI-only checks versus a fresh
WASM build. Reuse a verified current WASM package while iterating on HTML/CSS;
rebuild after Rust, dependency, feature, or build-configuration changes. The
integration pass tests the deployable static bundle as well as the source UI.

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

When the web surface is selected, CI runs the built browser suite, including a
mobile viewport, and checks that committed CSS matches a fresh build. Results
explain the numerical meaning of “exact”; they are not proof certificates.

Shareable URLs describe editable controls; use a JSON export to retain the
completed discovery run. `ranking-mode=discovery` is the default;
`ranking-mode=complexity` and `ranking-mode=parity` explicitly request raw ordering.
Legacy camelCase aliases remain supported. The matching precision in Discovery
mode comes from the literal, including trailing zeros, rather than the raw-mode
match-all-digits checkbox.
