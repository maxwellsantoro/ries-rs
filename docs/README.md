# Documentation map

Read the smallest set that answers the task. Code and regression tests define
current runtime behavior; the roadmap records delivered packets, acceptance
criteria, and deferred scope.

## Planning and development

- [ROADMAP.md](ROADMAP.md): delivery status, dependencies, agent packet ownership,
  acceptance criteria, and execution order
- [VALIDATION.md](VALIDATION.md): change-scoped checks, evidence reuse, integration,
  CI policy, and release validation
- [CONTRIBUTING.md](../CONTRIBUTING.md): setup and contribution workflow
- [AGENTS.md](../AGENTS.md): short agent entrypoint
- [Tests](../tests/README.md): suite inventory and historical comparison tooling
- [RELEASING.md](../RELEASING.md): immutable candidate and publication checklist

## Current reference

| Need | Read |
|------|------|
| Overview, install, common examples | [README.md](../README.md) |
| Module ownership and search pipeline | [ARCHITECTURE.md](ARCHITECTURE.md) |
| Search, numerical acceptance, ranking, determinism | [SEARCH_MODEL.md](SEARCH_MODEL.md) |
| Symbol weights and budgets | [COMPLEXITY.md](COMPLEXITY.md) |
| Python API and local development | [PYTHON_BINDINGS.md](PYTHON_BINDINGS.md) |
| JavaScript / WASM API | [WASM_BINDINGS.md](WASM_BINDINGS.md) |
| Browser build, hosting, smoke flow | [web/README.md](../web/README.md) |
| Independent correctness evidence and its limits | [QUALIFICATION.md](QUALIFICATION.md) |
| Decimal discovery, reproducible export, and selected-hypothesis verification | [DISCOVERY_WORKFLOW.md](DISCOVERY_WORKFLOW.md) |
| Benchmark methodology and profiling | [PERFORMANCE.md](PERFORMANCE.md) |
| Historical compatibility scope | [PARITY_STATUS.md](PARITY_STATUS.md) |

## Versioned evidence and history

- [benchmarks/](benchmarks/README.md): dated measurements and raw artifacts;
  results apply to the recorded source and environment
- `releases/`: versioned release notes
- `archive/plans/`: completed or superseded implementation plans
- `archive/parity/`: earlier compatibility investigations
- `archive/artifacts/`: retained debug and generated artifacts
- `../reviews/`: earlier code assessments; findings must be checked against current code

Archived plans are not an active backlog. Preserve measured evidence and release
history; fix current guidance in the references above. Published package versions
are established by crates.io (`ries`), PyPI (`ries-rs`), and GitHub Releases,
not by an unreleased local checkout.
