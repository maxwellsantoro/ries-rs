// Test to bypass search and directly test expression creation
import * as fs from 'fs';

async function test() {
  try {
    const wasmBytes = fs.readFileSync('pkg/ries_rs_bg.wasm');
    const module = await import('./pkg/ries_rs.js');

    await module.default({ module_or_path: wasmBytes.buffer });

    console.log('WASM initialized!');
    console.log('Version:', module.version());

    // Test listPresets first (should work)
    console.log('\nTesting listPresets()...');
    const presets = module.listPresets();
    console.log('Presets:', Object.keys(presets).length, 'available');

    // Try to test with a very minimal search - level 0 should generate minimal expressions
    console.log('\nTesting minimal search (level 0, max 1 match)...');
    try {
      const results = module.search(1.0, { level: 0, maxMatches: 1 });
      console.log('Success! Found', results.length, 'matches');
    } catch (e) {
      console.log('Search failed:', e.message);
    }

  } catch (e) {
    console.error('Error:', e.message);
  }
}

test();
