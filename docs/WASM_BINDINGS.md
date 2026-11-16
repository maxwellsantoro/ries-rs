# WebAssembly Bindings

The WebAssembly build exposes the `ries-rs` engine to JavaScript and
TypeScript. It supports browser-facing bundles, bundler output, Node-oriented
output, and the repository's static browser UI.

GitHub releases include a `ries-rs-wasm.tar.gz` artifact containing `pkg`,
`pkg-node`, and `pkg-bundler`.

## Build Targets

From the repository root:

```bash
npm ci
rustup toolchain install nightly --profile minimal --component rust-src --target wasm32-unknown-unknown

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

For the browser UI and static bundle itself, see [web/README.md](../web/README.md).

## Exported Functions

The package exports:

- `init()`
- `discover(targetLiteral, options?)` — precision-aware hypotheses and complete run evidence
- `search(target, options?)` — compatibility/raw match objects
- `listPresets()`
- `version()`

Threaded builds also re-export `initThreadPool(n)` when built with the
`wasm-threads` feature.

## Quick Start

```javascript
import init, { discover, search, SearchOptions, listPresets, version } from "ries-rs";

await init();

console.log(version());
console.log(listPresets());

const run = discover("3.14159265", { level: 3, maxMatches: 3 });
console.log(run.hypotheses[0].representative.solve_for_x); // x = pi
console.log(run.config.input_tolerance);
console.log(JSON.stringify(run)); // preserves the original literal and settings

// Existing raw callers retain their match objects and ordering.
const results = search(3.141592653589793, {
  level: 3,
  maxMatches: 8,
  rankingMode: "complexity",
});

console.log(results[0].to_string());
console.log(results[0].to_json());

// Fluent options can also be passed directly:
const options = new SearchOptions().level(1).max_matches(4);
console.log(search(2, options));
```

## `discover(targetLiteral, options?)`

This additive entry point accepts the original decimal as a string and returns a
plain JSON-compatible object with schema `ries-discovery-v1`. It uses the shared
Rust discovery policy: prefer simple hypotheses that meet the entered precision,
retain supporting equations, and safely group recognized duplicate presentations.
Existing `search()` and `SearchOptions` calls retain their raw behavior.

| Field | Type | Default | Notes |
|-------|------|---------|-------|
| `level` | integer | `2` | accepted range `0..=5` |
| `maxMatches` | integer | `8` | hypothesis limit, `1..=1000`; a run can return fewer |
| `preset` | string or null | null | validated against `listPresets()` |
| `parallel` | boolean | false | true requires a threaded build with its pool initialized |
| `verificationBits` | integer or null | null | optional numerical corroboration, at most 4096 bits |

Snake-case aliases `max_matches` and `verification_bits` are accepted. Options
must be a plain object; unknown fields, duplicate aliases, and invalid types or
ranges fail rather than being ignored. `null`, `undefined`, and omitted options
select defaults. Decimal validation rejects nonfinite values, nonzero underflow,
and rounding loss beyond the original literal's last-place tolerance. Surrounding
space and trailing zeros are preserved in `target_literal`; this still searches
using a parsed `f64` target.

The result includes:

- `engine_version`, `target_literal`, and parsed `target`
- `config`: effective precision tolerance, budgets, grammar/weights, preset
  constants, selection policy, requested verification, and parallel settings
- `stats`: effective matcher, candidate/hypothesis counts, and search work
- `hypotheses`: unique keys, representative match summaries, other retained
  `supporting_equations`, `within_input_precision`, and validation statuses

Validation distinguishes floating-point search, optional higher-precision
numerical corroboration, and an independent polynomial root certificate. The
standard WASM build cannot provide GMP/MPFR corroboration; a requested unsupported
check reports `unavailable` with a reason. Precision requests of 53 bits or less
also report unavailable. No numerical status is a proof about the intended
constant, and discovery does not compute independent polynomial certificates.

Save the full JSON run to use the repository's independent checker:

```bash
python3 scripts/verify_discovery.py run.json --output evidence.json
python3 scripts/verify_discovery.py evidence.json --verify
```

The checker supports a conservative polynomial grammar, can use a retained
supporting equation, and reports unavailable cases explicitly. Browser selected
exports additionally wrap the full run with schema `ries-discovery-selection-v1`
and a zero-based `hypothesis_index`; the checker accepts that form too.

## `search(target, options?)`

The default initializer works in browsers and Node.js ESM. Node's CommonJS
`require("ries-rs")` loads the synchronous Node bindings directly. Node ESM
also loads the bundled WASM synchronously at import; its default initializer and
`initSync()` return those existing exports and ignore custom module arguments.

`SearchOptions` has fluent `level`, `max_matches`, and `preset` methods;
its `to_json()` method returns a plain options object. These methods consume
the builder, so retain their returned value (or chain them).

`target` is a required JavaScript number.

Supported `options` fields:

| Field | Type | Default | Notes |
|-------|------|---------|-------|
| `level` | `number` | `2` | accepted range: `0..=5` |
| `maxMatches` | `number` | `16` | hard-capped at `10000` |
| `preset` | `string \| null` | `null` | validated against `listPresets()` |
| `rankingMode` | `"complexity" \| "parity"` | `"complexity"` | controls the tie-break ranking mode |
| `matchAllDigits` | `boolean` | `false` | uses target-significant-digit tolerance instead of the default relative tolerance |
| `targetText` | `string` | omitted | decimal spelling of `target`, including trailing zeros or an exponent; must parse to the same finite number |
| `usePslq` | `boolean` | `false` | accepted for compatibility, but returns an error because PSLQ is not available in the WASM build |

Notes:

- The WASM bindings use `search::level_to_complexity` (`40 + 2*L` on each side),
  not the CLI's `-l/--level` mapping (`35 + 10*L`).
- Non-threaded WASM builds search sequentially; threaded builds can use the
  parallel engine after `initThreadPool(...)`.
- With `matchAllDigits`, `targetText` determines the requested last-digit tolerance.
  The exact decimal is checked against the exact binary value: rounding loss
  beyond half that last decimal place is rejected. Long zero padding on an exact
  representable value is allowed. This does not create an arbitrary-precision target.
- Nonzero `targetText` that underflows to zero is rejected even without
  `matchAllDigits`. Numeric-only callers already pass a JavaScript f64; lost
  spelling cannot be reconstructed. With no `targetText`, match-all-digits uses
  the shortest round-trip spelling of that numeric value.
- `target` must be finite; `NaN` and `Infinity` return an error.
- Results use raw match ordering, with error ahead of complexity for inexact
  matches. Complexity ranking does not apply the CLI report's precision-aware
  preference for simpler equations within the entered precision. Use `discover()`
  for precision-aware hypothesis selection and run evidence.

### API vs CLI level scale

The WASM `level` option uses `search::level_to_complexity`, not the CLI
`-l/--level` mapping. The API budget is `40 + 2*L` on each side. The CLI uses
`35 + 10*L` on each side, so the two scales meet near the bottom and then the
CLI pulls ahead. The smaller API steps limit interactive search growth, but
runtime still depends on the target, grammar, device, and build; no fixed browser
latency is guaranteed.

| Level | API `(max_lhs, max_rhs)` | CLI `(max_lhs, max_rhs)` |
|-------|--------------------------|--------------------------|
| 0 | (40, 40) | (35, 35) |
| 2 | (44, 44) | (55, 55) |
| 3 | (46, 46) | (65, 65) |
| 5 | (50, 50) | (85, 85) |

See `docs/SEARCH_MODEL.md` for the full mapping rationale.

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
| `constants` | `Array<[string, number]>` | preset constant names and values in postfix slot order (character codes 128 onward) |
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

Then run `npm run build:web:site`, deploy its contents, and open the UI with
`?threads=1`. The site builder copies `pkg-threads/` when it exists. The
normal build remains the default; without cross-origin isolation the UI uses
it even when the query parameter is present. Build both packages before
packaging the site, and rebuild the threaded package whenever the code changes.

This requires nightly Rust and `SharedArrayBuffer` headers on the page and its
worker responses:

```text
Cross-Origin-Opener-Policy: same-origin
Cross-Origin-Embedder-Policy: require-corp
```
