// Run after building pkg and pkg-node: node tests/wasm-package.mjs
import assert from 'node:assert/strict';
import { createRequire } from 'node:module';
import { execFileSync } from 'node:child_process';
import { readFileSync } from 'node:fs';
import init, { initSync, SearchOptions, search, version } from 'ries-rs';

assert.ok((await init()).memory instanceof WebAssembly.Memory);
assert.ok(initSync().memory instanceof WebAssembly.Memory);
assert.ok(version());
const options = new SearchOptions().level(0).max_matches(1);
assert.equal(options.to_json().max_matches, 1);
assert.equal(search(2, options).length, 1);
assert.equal(search(2, options.to_json()).length, 1);
assert.ok(createRequire(import.meta.url)('ries-rs').version());

const declarations = readFileSync(new URL('../pkg/ries_rs.d.ts', import.meta.url), 'utf8');
const optionsDeclaration = declarations.split('export class SearchOptions {')[1].split('\n}')[0];
for (const name of ['level', 'max_matches', 'preset']) {
  assert.match(optionsDeclaration, new RegExp(`\\b${name}\\(`));
  assert.doesNotMatch(optionsDeclaration, new RegExp(`^\\s*(?:get |set )?${name}\\s*:`, 'm'));
}

// wasm-pack may generate nested .gitignore files; ensure publication still
// contains the package's browser and Node entry points and their WASM.
const pack = JSON.parse(execFileSync('npm', ['pack', '--ignore-scripts', '--dry-run', '--json'], { encoding: 'utf8' }))[0];
const packedFiles = new Set(pack.files.map(file => file.path));
for (const file of ['pkg/package.json', 'pkg/ries_rs.js', 'pkg/ries_rs.d.ts',
  'pkg/ries_rs_bg.wasm', 'pkg-node/ries_rs.js', 'pkg-node/ries_rs_bg.wasm',
  'web-package/node.mjs']) {
  assert.ok(packedFiles.has(file), `npm package is missing ${file}`);
}
