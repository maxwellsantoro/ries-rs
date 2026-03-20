# 2026-03-20 Level-3 CLI Baseline (Non-Exact Workload)

This benchmark regenerates the repository's level-3 non-exact CLI baseline
using the newer search heuristics metrics now included in JSON output and the
benchmark capture workflow.

## Workload

- Target: `2.506314`
- Search level: `3`
- Ranking mode: `complexity`
- Output: `--json`
- Max matches: `16`
- Report mode: disabled (`--report false`)

## Results Summary

| Mode | Threads | Elapsed (ms) | Generation (ms) | Search (ms) | Peak RSS (MiB) | Exprs Generated | Candidate Pairs | Window Avg | Window Max | Gate Rejects | Cand/Insert | Newton Success | Pool Acceptance |
|------|---------|--------------|-----------------|-------------|----------------|-----------------|-----------------|------------|------------|--------------|-------------|----------------|-----------------|
| Sequential deterministic | 1 | 88944.63 | 1280.99 | 87220.36 | 258.2 | 1379957 | 13461359437 | 14978.84 | 437751 | 13393657767 | 236.40 | 92.1% | 100.0% |
| Parallel | 8 | 108737.51 | 1153.44 | 107115.62 | 258.1 | 1379957 | 13461359437 | 14978.84 | 437751 | 13393657767 | 236.40 | 92.1% | 100.0% |

Observed speedup (sequential deterministic / parallel): **0.818x**

## Notes

- This workload remains dominated by matching/Newton refinement rather than generation.
- The new counters show extremely wide candidate windows at level 3
  (`candidate_window_avg ~= 14978.84`, `candidate_window_max = 437751`).
- Strict pre-Newton gating rejects almost all coarse candidates before
  refinement (`strict_gate_rejections = 13393657767`), which means future
  tuning should focus on window sizing and candidate admission rather than on
  Newton convergence itself.
- On this machine and workload, the parallel run was slower than the
  deterministic sequential baseline despite slightly faster generation.

## Raw Artifacts

- Environment metadata: `docs/benchmarks/artifacts/2026-03-20-level3-baseline-environment.txt`
- Sequential JSON: `docs/benchmarks/artifacts/2026-03-20-level3-baseline-seq-deterministic.json`
- Parallel JSON: `docs/benchmarks/artifacts/2026-03-20-level3-baseline-parallel.json`
