#!/usr/bin/env bash
set -euo pipefail

# wasm-bindgen needs growable shared memory and the linker's heap and TLS
# exports to initialize the module in each worker.
export RUSTFLAGS="
  -C target-feature=+atomics,+bulk-memory
  -C link-arg=--shared-memory
  -C link-arg=--import-memory
  -C link-arg=--max-memory=2147483648
  -C link-arg=--export=__heap_base
  -C link-arg=--export=__wasm_init_tls
  -C link-arg=--export=__tls_size
  -C link-arg=--export=__tls_align
  -C link-arg=--export=__tls_base"

exec rustup run nightly wasm-pack build --target web --out-dir pkg-threads -- \
  --features wasm-threads,wasm-bindgen-rayon/no-bundler -Z build-std=panic_abort,std
