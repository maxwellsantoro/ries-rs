// Test that bypasses search and directly tests expression conversion
import * as fs from 'fs';

async function test() {
  try {
    const wasmBytes = fs.readFileSync('pkg/ries_rs_bg.wasm');
    const module = await import('./pkg/ries_rs.js');

    await module.default({ module_or_path: wasmBytes.buffer });

    console.log('WASM initialized!');

    // Try to list presets - this should work without search
    console.log('Presets:', module.listPresets());

  } catch (e) {
    console.error('Error:', e.message);
  }
}

test();
