# Benchmark Artifacts

This directory contains reproducible benchmark runs and environment metadata used for published performance claims.

## Current Baselines

- `2026-03-20-level3-baseline.md`: Level-3 non-exact CLI baseline regenerated with the newer search heuristics metrics in the JSON and summary report
- `2026-02-25-level3-baseline.md`: Level-3 non-exact CLI benchmark baseline on Apple M1 (sequential deterministic vs parallel)
- `2026-02-25-generation-parallel-scaling.md`: Criterion generation-only benchmark showing parallel generation scaling

## Artifacts

Raw machine/run artifacts are stored in `artifacts/`:

- environment metadata (`rustc`, `cargo`, OS, CPU, RAM)
- raw `--json` outputs from benchmark runs
- raw Criterion benchmark output for generation scaling

Use these alongside `docs/PERFORMANCE.md` for benchmark methodology and reporting rules.

## Capturing New Artifacts

For end-to-end CLI search baselines, use:

```bash
python3 scripts/capture_search_benchmark.py \
  --name 2026-03-19-level3-baseline \
  --target 2.506314 \
  --level 3 \
  --ranking complexity
```

This writes:

- `<name>-environment.txt`
- `<name>-seq-deterministic.json`
- `<name>-parallel.json`
- `<name>-summary.md`

The generated summary includes both the traditional timing counters and the
newer heuristic-tuning metrics such as candidate window width, strict-gate
rejections, Newton success rate, and pool acceptance rate.
