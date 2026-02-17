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
# Sequential
cargo run --release -- 2.5 -l2

# Parallel (default)
cargo run --release --features parallel -- 2.5 -l2
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
