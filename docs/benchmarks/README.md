# Benchmark Artifacts

This directory contains reproducible benchmark runs and environment metadata used for published performance claims.

## Current Baselines

- `2026-02-25-level3-baseline.md`: Level-3 non-exact CLI benchmark baseline on Apple M1 (sequential deterministic vs parallel)
- `2026-02-25-generation-parallel-scaling.md`: Criterion generation-only benchmark showing parallel generation scaling

## Artifacts

Raw machine/run artifacts are stored in `artifacts/`:

- environment metadata (`rustc`, `cargo`, OS, CPU, RAM)
- raw `--json` outputs from benchmark runs
- raw Criterion benchmark output for generation scaling

Use these alongside `docs/PERFORMANCE.md` for benchmark methodology and reporting rules.
