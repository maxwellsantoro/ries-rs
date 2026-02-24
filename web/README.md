# RIES browser demo

Minimal web UI for the RIES WASM build. Enter a target value, optional options, and run a search to see matching equations.

## Build WASM

From the **repository root** (not this directory):

```bash
npm run build
```

This runs `wasm-pack build --target web --out-dir pkg -- --features wasm` and produces `pkg/ries_rs.js` and `pkg/ries_rs_bg.wasm`.

## Serve the app

You must serve the app over HTTP (or HTTPS); `file://` will not work for loading ES modules and WASM.

**Option A — from repo root:**

```bash
npx serve . -p 5000
```

Then open: **http://localhost:5000/web/**

**Option B — from this directory:**

```bash
cd web && npx serve .. -p 5000
```

Then open: **http://localhost:5000/web/**

The demo loads the WASM module from `../pkg/ries_rs.js` relative to the script URL.

## Threaded build (optional)

From the repo root, with Rust nightly installed:

```bash
npm run build:threads
```

This produces `pkg-threads/` (not `pkg/`). To use it in this demo, either copy `pkg-threads` to `pkg` or change `main.js` to load from `../pkg-threads/ries_rs.js`. The demo will then detect `initThreadPool` and call it with `navigator.hardwareConcurrency` so a single search uses multiple workers. The server must send [COOP/COEP](https://web.dev/articles/cross-origin-isolation) headers for `SharedArrayBuffer`.
