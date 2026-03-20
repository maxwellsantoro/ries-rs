# Performance

This document describes how `ries-rs` handles benchmark claims and where the
artifact-backed performance data lives.

## Source of Truth

For any published performance claim, prefer these sources in order:

1. `docs/benchmarks/`
2. raw files under `docs/benchmarks/artifacts/`
3. reproducible local commands recorded alongside the report

Avoid citing hand-written numbers without a matching benchmark report or raw
artifact in the repository.

## Current Benchmark Reports

- `docs/benchmarks/2026-03-20-level3-baseline.md`
  End-to-end CLI baseline regenerated with candidate-window and pool-gating metrics.
- `docs/benchmarks/2026-02-25-level3-baseline.md`
  End-to-end CLI baseline for a non-exact level-3 workload.
- `docs/benchmarks/2026-02-25-generation-parallel-scaling.md`
  Criterion-based generation scaling report.
- `docs/benchmarks/README.md`
  Index of the current benchmark set and raw artifact layout.

## Reporting Rules

When publishing numbers in the README, release notes, issues, or papers, record:

- the `ries-rs` git commit or tag
- `rustc --version --verbose` and `cargo --version`
- enabled features (`parallel`, `highprec`, `wasm`, etc.)
- exact command lines
- target triple and build profile
- CPU, core/thread count, RAM, and OS details
- whether the result is a single run, median, or another aggregation

If different machines are involved, say so explicitly in the report.

## Recommended CLI Benchmark Commands

Use explicit flags so the configuration is unambiguous:

```bash
# Sequential deterministic baseline
cargo run --release --no-default-features -- \
  3.141592653589793 -l3 --classic --deterministic --report false -n 16 --json

# Parallel baseline
cargo run --release -- \
  3.141592653589793 -l3 --classic --report false -n 16 --json
```

Why `--json`:

- includes structured timing and search stats
- preserves the exact run configuration more clearly than formatted text output
- is easy to archive under `docs/benchmarks/artifacts/`

For artifact-backed end-to-end benchmark captures, prefer:

```bash
python3 scripts/capture_search_benchmark.py \
  --name 2026-03-19-level3-baseline \
  --target 2.506314 \
  --level 3 \
  --ranking complexity
```

That command writes:

- raw sequential deterministic JSON
- raw parallel JSON
- environment metadata
- a generated Markdown summary table including the newer search metrics used for heuristic tuning

## Criterion Benchmarks

Repository microbenchmarks live under `benches/` and use Criterion.

```bash
# Run all Criterion benches
cargo bench

# Or run one suite
cargo bench --bench evaluation
cargo bench --bench generation
cargo bench --bench search
```

The GitHub benchmark workflow uploads Criterion reports as artifacts; treat those
reports as exploratory data unless a matching repository benchmark note has been
written under `docs/benchmarks/`.

## Local Profiling

Build with release settings before profiling:

```bash
cargo build --release --locked
```

Linux CPU profiling:

```bash
perf record -g ./target/release/ries-rs 2.5 -l3
perf report
```

macOS CPU profiling:

```bash
instruments -t "Time Profiler" ./target/release/ries-rs 2.5 -l3
```

The repository also includes `scripts/profile_comparison.sh` for side-by-side
local comparison against a historical C RIES build when that binary is
available. The script auto-selects the verbose `time` flag for the host
platform so it works on both macOS and Linux, and it now extracts the Rust
search JSON metrics most useful for heuristic tuning:

- `candidate_window_avg`
- `candidate_window_max`
- `strict_gate_rejections`
- `candidates_per_pool_insertion`
- `newton_success_rate`
- `pool_acceptance_rate`

Use `scripts/profile_comparison.sh` for quick local exploration. Use
`scripts/capture_search_benchmark.py` when the goal is to generate repository
artifacts suitable for benchmark notes, release documentation, or future
before/after comparisons.

## Memory Notes

`--json` output and `--stats` include peak RSS when the platform runtime can
report it:

- Unix/macOS: populated via `getrusage(RUSAGE_SELF)`
- other platforms: may be unavailable and appear as `null`

For architectural context on batch vs streaming generation and why memory usage
varies strongly with search mode, see `docs/ARCHITECTURE.md`.

## Tuning for Your Use Case

### For Accuracy (High Precision)

```bash
# Use higher complexity levels
./target/release/ries-rs 2.5 -l4

# Rebuild with the optional high-precision engine
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
