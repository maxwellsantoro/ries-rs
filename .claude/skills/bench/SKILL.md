---
name: bench
description: Run Criterion benchmarks for ries-rs, optionally comparing against a saved baseline
disable-model-invocation: true
---

Run Criterion benchmarks. Available benches: `evaluation`, `search`, `generation`.

## Usage
```
/bench                        # Run all three benches, save as 'main' baseline
/bench search                 # Run only the search bench (save as 'main')
/bench search main            # Compare search bench against the 'main' baseline
/bench all main               # Compare all three benches against 'main' baseline
```

## Steps

1. Parse arguments:
   - Arg 1 (bench name): `evaluation` | `search` | `generation` | `all` (default: `all`)
   - Arg 2 (baseline): name of saved baseline to compare against, or omit to save as `main`

2. If comparing against a baseline:
   ```
   cargo bench --bench <name> -- --baseline <baseline>
   ```

3. If saving a new baseline:
   ```
   cargo bench --bench <name> -- --save-baseline main
   ```

4. When bench = `all`, run all three: `evaluation`, `search`, `generation`.

5. After running, note where Criterion saves the HTML report:
   `target/criterion/report/index.html`
