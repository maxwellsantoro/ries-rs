# 2026-02-25 Generation Parallel Scaling (Criterion)

This benchmark isolates expression generation throughput to measure the impact of parallel generation directly.

It complements the end-to-end CLI benchmark baseline by showing where the `parallel` feature *does* scale well (generation), even when full search workloads are dominated by matching/Newton refinement.

## Benchmark

- Benchmark source: `benches/generation.rs` (`bench_parallel_generation`)
- Target workload inside benchmark: generation config `max_lhs_complexity = 60`, `max_rhs_complexity = 50`
- Benchmark harness: Criterion

## Command

```bash
cargo bench --bench generation parallel_generation -- \
  --noplot --sample-size 10 --measurement-time 2 \
  > docs/benchmarks/artifacts/2026-02-25-generation-parallel-criterion.txt 2>&1
```

## Results (Criterion estimates)

Sequential generation:

- time: `[193.41 ms 193.62 ms 193.88 ms]`

Parallel generation:

- time: `[51.442 ms 60.884 ms 69.314 ms]`

## Speedup

- Median-based estimate: `193.62 / 60.884 = 3.18x`
- Conservative bound (seq low / par high): `193.41 / 69.314 = 2.79x`

This is the stronger parallel-scaling datapoint to cite when discussing the generation phase specifically.

## Notes

- This is not an end-to-end search benchmark; it isolates generation only.
- Use `docs/benchmarks/2026-02-25-level3-baseline.md` for an end-to-end CLI search baseline on the same machine.
- Full raw Criterion output is preserved for auditability.

## Raw Artifact

- `docs/benchmarks/artifacts/2026-02-25-generation-parallel-criterion.txt`
