// Minimal WASM test to isolate the issue
import * as fs from 'fs';

async function test() {
  try {
    const wasmBytes = fs.readFileSync('pkg/ries_rs_bg.wasm');
    console.log('WASM file size:', wasmBytes.length, 'bytes');

    const module = await import('./pkg/ries_rs.js');

    console.log('Initializing WASM...');
    await module.default({ module_or_path: wasmBytes.buffer });

    console.log('WASM initialized!');
    console.log('Version:', module.version());

    // Test with very simple target
    console.log('\nTesting search for 2.0...');
    const results = module.search(2.0);
    console.log('Found', results.length, 'results');

  } catch (e) {
    console.error('Error:', e.message);
    if (e.stack) console.error('Stack:', e.stack);
  }
}

test();
