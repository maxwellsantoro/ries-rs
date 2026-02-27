# 2026-02-25 Level-3 CLI Baseline (Non-Exact Workload)

This benchmark provides a v1.0 reference datapoint using a non-exact target so the engine measures real search work (not the fast exact-match path).

## Workload

- Target: `2.506314`
- Search level: `3`
- Ranking mode: `complexity`
- Output: `--json`
- Max matches: `16`
- Report mode: disabled (`--report false`)

## Commands

Sequential deterministic baseline (no `parallel` feature):

```bash
cargo run --release --quiet --no-default-features -- \
  2.506314 -l 3 --report false --max-matches 16 \
  --complexity-ranking --deterministic --json \
  > docs/benchmarks/artifacts/2026-02-25-level3-seq-deterministic.json
```

Parallel run (default features):

```bash
cargo run --release --quiet -- \
  2.506314 -l 3 --report false --max-matches 16 \
  --complexity-ranking --json \
  > docs/benchmarks/artifacts/2026-02-25-level3-parallel.json
```

## Results Summary

| Mode | Threads | Elapsed (ms) | Generation (ms) | Search (ms) | Peak RSS (MiB) | Exprs Generated | Candidate Pairs |
|------|---------|--------------|-----------------|-------------|----------------|-----------------|-----------------|
| Sequential deterministic | 1 | 91,577.07 | 1,027.08 | 90,147.02 | 317.8 | 1,380,377 | 13,342,871,648 |
| Parallel (default feature) | 8 | 84,492.40 | 1,025.28 | 83,073.58 | 318.0 | 1,380,377 | 13,342,871,648 |

Observed speedup (sequential deterministic / parallel): **1.084x**

## Notes

- Both runs generated the same expression counts and tested the same number of candidate pairs.
- The measured difference here is modest; this workload is useful as a baseline artifact, not as a best-case parallel scaling demo.
- Peak RSS comes from runtime `getrusage` sampling (`ru_maxrss`) and is included in the JSON outputs.

## Raw Artifacts

- Environment metadata: `docs/benchmarks/artifacts/2026-02-25-environment.txt`
- Sequential JSON: `docs/benchmarks/artifacts/2026-02-25-level3-seq-deterministic.json`
- Parallel JSON: `docs/benchmarks/artifacts/2026-02-25-level3-parallel.json`
