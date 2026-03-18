# WebAssembly Bindings

The WebAssembly build exposes the `ries-rs` engine to JavaScript and
TypeScript. It supports browser-facing bundles, bundler output, Node-oriented
output, and the repository's static browser UI.

GitHub releases include a `ries-rs-wasm.tar.gz` artifact containing `pkg`,
`pkg-node`, and `pkg-bundler`.

## Build Targets

From the repository root:

```bash
npm install
rustup toolchain install nightly

# Browser-friendly package
npm run build

# Bundler output
npm run build:bundler

# Node-targeted output
npm run build:node

# Browser UI bundle
npm run build:web:site
```

The local scripts currently use nightly `wasm-pack` with `-Z build-std`.

For the browser UI and static bundle itself, see `web/README.md`.

## Exported Functions

The package exports:

- `init()`
- `search(target, options?)`
- `listPresets()`
- `version()`

Threaded builds also re-export `initThreadPool(n)` when built with the
`wasm-threads` feature.

## Quick Start

```javascript
import init, { search, listPresets, version } from "ries-rs";

await init();

console.log(version());
console.log(listPresets());

const results = search(3.141592653589793, {
  level: 3,
  maxMatches: 8,
  rankingMode: "complexity",
});

console.log(results[0].to_string());
console.log(results[0].to_json());
```

## `search(target, options?)`

`target` is a required JavaScript number.

Supported `options` fields:

| Field | Type | Default | Notes |
|-------|------|---------|-------|
| `level` | `number` | `2` | accepted range: `0..=5` |
| `maxMatches` | `number` | `16` | hard-capped at `10000` |
| `preset` | `string \| null` | `null` | validated against `listPresets()` |
| `rankingMode` | `"complexity" \| "parity"` | `"complexity"` | controls the tie-break ranking mode |
| `matchAllDigits` | `boolean` | `false` | uses target-significant-digit tolerance instead of the default relative tolerance |
| `usePslq` | `boolean` | `false` | accepted for compatibility, but returns an error because PSLQ is not available in the WASM build |

Notes:

- The WASM bindings use the library-level complexity mapping, not the CLI's
  heavier `-l/--level` mapping.
- Non-threaded WASM builds search sequentially; threaded builds can use the
  parallel engine after `initThreadPool(...)`.

## `WasmMatch` Properties

| Property | Type | Description |
|----------|------|-------------|
| `lhs` | `string` | left-hand side in infix form |
| `rhs` | `string` | right-hand side in infix form |
| `lhs_postfix` | `string` | left-hand side in postfix form |
| `rhs_postfix` | `string` | right-hand side in postfix form |
| `solve_for_x` | `string \| null` | solve-for-x rendering when analytically available |
| `solve_for_x_postfix` | `string \| null` | postfix form of `solve_for_x` |
| `canonical_key` | `string` | canonicalized equation key used for dedupe/reporting |
| `x_value` | `number` | solved numeric value for `x` |
| `error` | `number` | `x_value - target` |
| `complexity` | `number` | total complexity score |
| `operator_count` | `number` | total operator count across both sides |
| `tree_depth` | `number` | maximum tree depth across both sides |
| `is_exact` | `boolean` | whether the match is within exact-match tolerance |

Methods:

- `to_string()`
- `to_json()`

## Static Hosting

To build the subpath-safe browser bundle:

```bash
npm run build:web:site
```

Deploy the contents of `dist/web-site/` to the target directory.

## Threaded Build

For browser parallelism:

```bash
npm run build:threads
```

This requires nightly Rust and `SharedArrayBuffer` headers:

```text
Cross-Origin-Opener-Policy: same-origin
Cross-Origin-Embedder-Policy: require-corp
```
