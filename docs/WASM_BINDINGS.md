# WebAssembly Bindings

The WebAssembly build exposes `ries-rs` to JavaScript and TypeScript in
browsers, Node.js, and static web deployments.

GitHub releases include a `ries-rs-wasm.tar.gz` artifact containing the built
`pkg`, `pkg-node`, and `pkg-bundler` outputs. Rebuild locally if you need a
fresh bundle from the current checkout.

## Build Targets

From the repository root:

```bash
# Browser-friendly package
npm run build

# Bundler output (webpack, Vite, Rollup, ...)
npm run build:bundler

# Node.js output
npm run build:node

# Static site bundle for deployment at a subpath
npm run build:web:site
```

The deployable web bundle ends up in `dist/web-site/`. For the browser UI
itself, including subpath hosting, see [`web/README.md`](../web/README.md).

## Quick Start

```javascript
import init, { search, listPresets, version } from "ries-rs";

await init();

const results = search(3.1415926535, {
  level: 3,
  maxMatches: 20,
});

console.log(version());
console.log(listPresets());
console.log(results[0].to_string());
```

## `SearchOptions`

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `level` | number | 2 | Search depth (0-5) |
| `maxMatches` | number | 16 | Maximum matches to return |
| `preset` | string | `null` | Domain preset name |

## `WasmMatch` Properties

| Property | Type | Description |
|----------|------|-------------|
| `lhs` | string | Left-hand side expression (contains `x`) |
| `rhs` | string | Right-hand side expression (constants only) |
| `lhs_postfix` | string | Postfix representation of the LHS |
| `rhs_postfix` | string | Postfix representation of the RHS |
| `x_value` | number | Solved value of `x` |
| `error` | number | `x_value - target` |
| `complexity` | number | Complexity score (lower is simpler) |
| `is_exact` | boolean | `true` if error < `1e-14` |

## Browser Example

```html
<!DOCTYPE html>
<html>
<head>
  <script type="module">
    import init, { search } from "./pkg/ries_rs.js";

    async function run() {
      await init();

      const results = search(3.14159, { level: 2, maxMatches: 5 });
      for (const match of results) {
        document.body.innerHTML += `<p>${match.to_string()}</p>`;
      }
    }

    run();
  </script>
</head>
<body></body>
</html>
```

## Node.js Example

```javascript
const { search, listPresets } = require("ries-rs");

const results = search(2.718281828, { level: 3 });
console.log(`Found ${results.length} matches`);
console.log(listPresets());
```

## Static Hosting

To host the browser UI at a path such as
`https://example.com/projects/ries-rs/`:

```bash
npm run build:web:site
```

Then deploy the contents of `dist/web-site/` to the target directory.

## Threaded Build

For parallel search in browsers:

```bash
npm run build:threads
```

This requires nightly Rust plus the browser headers needed for
`SharedArrayBuffer`:

```text
Cross-Origin-Opener-Policy: same-origin
Cross-Origin-Embedder-Policy: require-corp
```
