---
name: parity-check
description: Run a side-by-side parity comparison between ries-rs and the original RIES C binary for a given target value
disable-model-invocation: true
---

Run the parity comparison script between ries-rs and the original RIES binary.

## Usage
```
/parity-check <target> [level] [max-matches]
```
Defaults: level=2, max-matches=6

## Steps

1. Locate the original RIES binary:
   - Local path: `/Users/maxwell/Apps/ries/ries/ries-original`
   - If that directory does not exist, clone it:
     ```
     git clone https://github.com/clsn/ries /Users/maxwell/Apps/ries/ries/ries-original
     ```

2. If the binary `/Users/maxwell/Apps/ries/ries/ries-original/ries` does not exist, compile it:
   ```
   gcc /Users/maxwell/Apps/ries/ries/ries-original/ries.c -lm -o /Users/maxwell/Apps/ries/ries/ries-original/ries
   ```

3. Build ries-rs if `target/release/ries-rs` does not exist:
   ```
   cargo build --release
   ```

4. Run the comparison:
   ```
   ./tests/compare_with_original.sh <target> <level> <max-matches> /Users/maxwell/Apps/ries/ries/ries-original/ries
   ```

5. Show the full output. Point out any equations present in one output but missing from the other.
