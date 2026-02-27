# RIES-RS Performance Documentation

This document describes the performance characteristics of the Rust implementation
of RIES (Reverse Integer Equation Search) and provides guidance for optimization.

## Quick Start

```bash
# Build in release mode for best performance
cargo build --release

# Run basic search
./target/release/ries-rs 2.5

# Compare against original C implementation
./scripts/profile_comparison.sh
```

## Benchmark Reporting for v1.0 (Reproducible)

For any benchmark numbers published in the README, release notes, or papers, record all of the following:

- `ries-rs` git commit / tag
- `cargo` and `rustc` versions (`rustc --version --verbose`)
- enabled features (for example default parallel feature, `highprec`, `wasm`)
- build profile and flags (`cargo build --release`, `RUSTFLAGS`, target triple)
- CPU model, core/thread count, RAM size
- OS version and kernel
- exact CLI command lines (including target, level, ranking mode, deterministic/parallel flags)
- number of runs and aggregation method (single run vs median of N)

### Recommended CLI Benchmark Protocol

Use explicit flags so the run configuration is unambiguous:

```bash
# Sequential baseline (deterministic)
cargo run --release --no-default-features -- \
  3.141592653589793 -l3 --classic --deterministic --report false -n 16 --json

# Parallel run (default features)
cargo run --release -- \
  3.141592653589793 -l3 --classic --report false -n 16 --json
```

Why `--json`:

- captures structured search stats (including timing, threads, and peak memory when supported)
- avoids parsing human-formatted text output
- makes benchmark artifact storage easy

### Machine/Compiler Metadata Capture

Recommended metadata commands (macOS/Linux):

```bash
rustc --version --verbose
cargo --version
uname -a

# macOS
sysctl -n machdep.cpu.brand_string
sysctl -n hw.ncpu
sysctl -n hw.memsize

# Linux (alternatives)
lscpu
free -h
```

### Benchmark Table Template (README / Paper)

Use a table with explicit environment details near the numbers:

| Target | Level | Precision | Mode | Threads | Time (ms) | Notes |
|--------|-------|-----------|------|---------|-----------|-------|
| π | 3 | f64 | sequential deterministic | 1 | `<measured>` | `--no-default-features --deterministic` |
| π | 3 | f64 | parallel | `<measured>` | `<measured>` | default feature set |
| π | 3 | f64 | wasm | browser-dependent | `<measured>` | include browser + device |

Do not mix measurements from different machines in one headline table unless the row notes state that explicitly.

See `docs/benchmarks/` for repository-local baseline artifacts that include raw JSON outputs and environment metadata.

## Benchmark Suite

RIES-RS includes a comprehensive benchmark suite using Criterion:

```bash
# Run all benchmarks
cargo bench

# Run specific benchmark suite
cargo bench --bench evaluation
cargo bench --bench search
cargo bench --bench generation
```

### Evaluation Benchmarks

Measures expression evaluation performance:

| Expression Type | Time (ns) | Notes |
|-----------------|-----------|-------|
| Simple (3+2) | ~57 | No variable, constant folding |
| Variable (x*2) | ~56 | Simple variable expression |
| Complex (x²+2x+1) | ~105 | Multiple operations |
| Trig (sin+cos) | ~118 | Transcendental functions |
| Lambert W | ~189 | Iterative algorithm |
| x^x | ~87 | Power with variable base |

### Workspace Strategies

Three strategies for evaluation in hot loops:

| Strategy | Time (ns) | Notes |
|----------|-----------|-------|
| Allocating | ~101 | Creates new workspace each call |
| Reusable Workspace | ~74 | Pass mutable workspace |
| Thread-Local | ~77 | Zero-config, thread-safe |

**Recommendation**: Use thread-local (`evaluate_fast`) for most cases, or
explicit workspace reuse (`evaluate_with_workspace`) for maximum control.

### Search Performance

Search time scales with complexity level:

| Level | LHS Complexity | RHS Complexity | Search Time |
|-------|----------------|----------------|-------------|
| 1 | 20 | 15 | Fast |
| 2 | 43 | 36 | Moderate |
| 3 | 60 | 50 | Slower |

### Parallel vs Sequential

With the `parallel` feature (default), generation and search can use multiple cores:

```bash
# Parallel (default — parallel feature is on by default)
cargo run --release -- 2.5 -l2

# Sequential (disable default parallel feature)
cargo run --release --no-default-features -- 2.5 -l2
```

## Memory Usage

RIES-RS is designed for minimal allocations:

1. **Expression Generation**: Uses pre-allocated vectors with capacity hints
2. **Evaluation**: Thread-local workspaces eliminate per-call allocations
3. **Search Database**: Sorted vector for cache-friendly range queries

### Typical Memory Profile

| Component | Memory | Notes |
|-----------|--------|-------|
| Expression Pool | O(n) expressions | Scales with complexity level |
| RHS Database | O(n) sorted values | Binary search range queries |
| Thread-Local Workspace | ~512 bytes | Fixed per thread |

### Runtime Memory Introspection

CLI `--json` output and `--stats` now include peak resident set size (RSS) when supported by the platform runtime.

- Unix/macOS: populated using `getrusage(RUSAGE_SELF)`
- Other platforms: may be unavailable (`null` in JSON)

## Optimization Opportunities

### Implemented Optimizations

1. **Workspace Reuse**: Zero-allocation evaluation in hot loops
2. **Cache-Friendly Database**: Sorted vectors for range queries
3. **Adaptive Newton-Raphson**: Early termination when converged
4. **Expression Pruning**: Skip degenerate expressions (zero derivative)

### Future Optimization Opportunities

1. **SIMD Evaluation**: Vectorized evaluation for multiple x values
2. **GPU Acceleration**: Offload expression evaluation to GPU
3. **Memoization**: Cache common subexpressions
4. **Better Pruning**: Statistical pruning based on value distribution

## Comparison with Original C Implementation

The Rust implementation provides:

- **Memory Safety**: No buffer overflows or use-after-free
- **Thread Safety**: Safe parallel execution with Rayon
- **Error Handling**: Comprehensive error types instead of exit codes
- **Extensibility**: Trait-based numeric types for arbitrary precision

### Benchmark Results (vs C ries)

| Target | C ries | ries-rs | Speedup |
|--------|--------|---------|---------|
| π (exact match) | 0.004s | 0.003s | ~equal |
| e (exact match) | 0.002s | 0.003s | ~equal |
| √2 (exact match) | 0.003s | 0.003s | ~equal |
| φ (golden ratio) | 0.003s | 0.002s | ~equal |
| 2.506314 (no exact) | 0.19s | 0.02s | **11x faster** |
| 2.506314 -l4 | 6.4s | 1.0s | **6x faster** |

### Why ries-rs is Faster for Non-Exact Matches

1. **Fast Path**: Known constants (π, e, √2, φ, etc.) are checked instantly before expensive generation
2. **Parallel Generation**: Expression generation uses Rayon for work-stealing parallelism
3. **Adaptive Pruning**: The Rust version uses stricter early-exit criteria in classic mode
4. **Better Memory Locality**: Arena allocation and cache-friendly data structures

## Profiling

### Using perf (Linux)

```bash
# Build with debug symbols
cargo build --release

# Profile a run
perf record -g ./target/release/ries-rs 2.5 -l3
perf report
```

### Using Instruments (macOS)

```bash
# Profile with Instruments
instruments -t "Time Profiler" ./target/release/ries-rs 2.5 -l3
```

### Memory Profiling

```bash
# Using valgrind (Linux)
valgrind --tool=massif ./target/release/ries-rs 2.5 -l3
ms_print massif.out.*
```

## Tuning for Your Use Case

### For Accuracy (High Precision)

```bash
# Use higher complexity levels
./target/release/ries-rs 2.5 -l4

# Future: use high-precision feature
cargo run --release --features highprec -- 2.5 --precision 256
```

### For Speed

```bash
# Use lower complexity levels
./target/release/ries-rs 2.5 -l1

# Stop at first exact match
./target/release/ries-rs 2.5 -l2 --stop-at-exact
```

### For Memory Efficiency

```bash
# Reduce max matches
./target/release/ries-rs 2.5 -l2 --max-matches 10
```
