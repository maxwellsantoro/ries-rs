// Simple test for WASM module
import * as fs from 'fs';

async function test() {
  try {
    // Read the WASM file as buffer
    const wasmBytes = fs.readFileSync('pkg/ries_rs_bg.wasm');
    console.log('WASM file size:', wasmBytes.length, 'bytes');

    // Import the module
    const module = await import('./pkg/ries_rs.js');
    console.log('Module exports:', Object.keys(module).slice(0, 5));

    // Try to init with the WASM bytes directly
    console.log('Initializing WASM with bytes...');
    await module.default({ module_or_path: wasmBytes.buffer });

    console.log('WASM initialized successfully!');
    console.log('Version:', module.version());

    // Test search
    console.log('\nTesting search for pi (3.14159)...');
    const results = module.search(3.14159);
    console.log('Found', results.length, 'results');
    if (results.length > 0) {
      console.log('First result:');
      const r = results[0];
      console.log('  lhs:', r.lhs);
      console.log('  rhs:', r.rhs);
      console.log('  x_value:', r.x_value);
      console.log('  error:', r.error);
    }

  } catch (e) {
    console.error('Error:', e.message);
    if (e.stack) console.error('Stack:', e.stack);
  }
}

test();
