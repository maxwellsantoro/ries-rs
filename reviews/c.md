I did a deeper second pass and found a few more things that are worth looking at closely. Here’s the consolidated report.

## Overall assessment

This is a strong project. The codebase is thoughtfully layered, the engine is shared across CLI/library/Python/WASM, and the repo shows real release, benchmarking, compatibility, and regression discipline rather than just feature accumulation. The architecture docs are also unusually clear about ownership and source-of-truth rules. 

What keeps it from feeling fully polished is not lack of engineering quality, but a handful of seam bugs and contract drifts at the edges: CLI symbol filtering with user-defined symbols, surface-to-surface docs drift, silent truncation of user-defined symbol capacity, and a reproducibility schema mismatch. Those are exactly the kinds of issues that appear in mature multi-surface projects. 

## What’s strong

The architecture is solid. The repo explicitly shares one core search engine across Rust CLI, Rust library, Python bindings, and WASM/browser UI, with a clean split between engine code, CLI glue, reporting/analysis, and packaging surfaces. It also explicitly says code and tests win when docs diverge, which is the right governance rule. 

The project also has good quality signals. There are regression tests around adaptive search behavior, user-defined functions surviving full search/refinement, and CI parity checks against the original RIES binary. That is a better test posture than most repos at this stage.   

Operationally, the repo has clearly invested in reproducibility and packaging. There is deterministic mode, manifest support, benchmark artifacts, release automation, and a safety fallback from bounded batch generation to streaming when the expression count would get too large.  

## Findings

### 1) High severity: `--only-symbols` appears to leak user-defined symbols

This is the clearest correctness issue I found.

In `build_gen_config`, built-in constants, unary ops, and binary ops are filtered first through `filter_symbols(...)`. After that, user constants and user functions are appended into the active pools unless they are explicitly excluded. That means an allowlist-style configuration such as `--only-symbols` can still admit user-defined constants/functions that were never allowed in the first place. The nearby tests cover built-in `only_symbols` behavior, but not the interaction with user-defined constants/functions. 

Why it matters: this breaks the contract of a restrictive symbol configuration, and it does so in a way that will be hard for users to notice unless they inspect the resulting equations carefully.

Suggested resolution:

* Build the full symbol universe first, including user-defined symbols, then apply allow/exclude filtering once.
* Mirror that logic for RHS-specific symbol overrides.
* Add tests for:

  * `--only-symbols` with user constants
  * `--only-symbols` with user functions
  * RHS-only allowlists with user symbols
  * combinations of `--only-symbols`, `--exclude`, and `--enable`

### 2) Medium-high severity: docs/API drift around “level” semantics

The repo’s own search model says the CLI level mapping is intentionally different from the lighter programmatic API helper mapping, and that those should not be conflated. The document states CLI complexity grows as `35 + 10*L` per side, while the library helper uses a much lighter mapping of `10 + 4*L` / `12 + 4*L`. 

At the same time, the CLI arg docs include very large “level means X equations” claims like “Level 0 ~ 89M, Level 2 ~ 11B, Level 5 ~ 15T.” That kind of language is risky once you expose both CLI and programmatic surfaces, because users will naturally assume the same “level” means the same search scale everywhere. 

Why it matters: this is a user-facing contract problem. It can mislead people about performance, reproducibility, and what to expect from the Python/library surfaces.

Suggested resolution:

* Rename/document these as distinct concepts: “CLI level” versus “API level helper.”
* Remove absolute equation-count claims from shared/public docs unless they are surface-specific and benchmark-backed.
* Add one small compatibility test or doc test that asserts the documented CLI mapping and API mapping separately so they cannot drift silently.

### 3) Medium-high severity: silent truncation at 16 user constants/functions

The symbol table explicitly pre-allocates placeholders only for `UserConstant0..15` and `UserFunction0..15`, and `from_profile` stops applying names/weights after 16 entries.  

Separately, profile/CLI parsing clearly accepts adding constants and functions, but the surfaced parsing snippets do not show any visible hard cap or error path for going beyond that capacity.  

Why it matters: silent truncation is worse than a hard error here. A user can believe they configured 20 constants/functions while the engine only honors the first 16.

Suggested resolution:

* Hard-error once the count exceeds 16, everywhere.
* Make the error surface consistent across CLI, profile loading, Python, and WASM.
* Document the cap prominently if you intend to keep it.
* Longer term, consider dynamic symbol registration rather than fixed placeholder slots.

### 4) Medium severity: manifest schema appears out of sync with runtime representation

The JSON schema for `run-manifest-v1.json` requires every result to contain a non-null numeric `stability` field. 

In my code review pass, the runtime manifest model uses an optional stability field, which means code and schema can disagree about whether `stability` is always present. I’d treat this as a real schema drift unless proven otherwise in the serializer path.

Why it matters: reproducibility tooling is only as strong as its contracts. A manifest schema mismatch breaks downstream validators and undermines trust in archival/replay workflows.

Suggested resolution:

* Pick one contract and enforce it:

  * either always emit numeric `stability`, or
  * make the schema allow `null` / omission.
* If the schema changes, version it rather than silently mutating `v1`.
* Add a CI test that serializes a manifest and validates it against the schema.

### 5) Medium-high severity: search-stage scalability still looks like the bottleneck

Your benchmark story is honest, but it points to a specific weakness. The generation-only benchmark shows strong parallel scaling, around 3.18x median speedup. The end-to-end level-3 CLI benchmark, though, shows only 1.084x speedup, with the same 13.34B candidate pairs in both modes. 

The thresholds module also documents a minimum search radius factor of `0.5 * |derivative|`, and the search module imports `calculate_adaptive_search_radius`, suggesting there is or was an attempt to make the window smarter.  

My review takeaway is that the search/matching phase is still dominating runtime, and the candidate-window policy is the first place I would look harder. There is already good regression coverage on adaptive over-generation behavior, which is encouraging, but that does not solve the matching-window cost in the main search path. 

Suggested resolution:

* Audit whether `calculate_adaptive_search_radius` is actually wired into the dominant path.
* Add instrumentation for:

  * candidate window width
  * candidates tested per accepted match
  * Newton call success rate
  * pool acceptance ratio as the search progresses
* Benchmark search-stage improvements separately from generation-stage improvements.
* Be careful in public messaging to distinguish “parallel generation scales well” from “full search scales well.”

### 6) Medium severity: compatibility/UX surface is getting large enough that cross-feature tests need to increase

The CLI has accumulated a very broad compatibility and control surface: presets, profiles, user constants/functions, stability checks, deterministic mode, one-sided mode, solve/no-solve, canonical reduction toggles, memory hints, trig/rationality filters, and more.  

This is not inherently bad. In fact, it is part of the project’s strength. But the bigger this surface gets, the more likely the real bugs are to live in interactions, not individual flags.

Suggested resolution:

* Expand matrix tests around cross-feature combinations rather than adding more single-feature tests.
* Prioritize seams:

  * custom symbols × filtering
  * deterministic mode × ranking mode × output mode
  * CLI level × API level docs/examples
  * manifest emission × stability/high-precision modes

### 7) Low-medium severity: local profiling script is not portable

`scripts/profile_comparison.sh` uses `/usr/bin/time -l` in verbose mode, which is BSD/macOS-specific. The repo otherwise targets multiple OSes and ships multi-platform artifacts, so this script is weaker than the rest of the tooling. 

Suggested resolution:

* Detect GNU vs BSD `time`.
* Or split this into `profile_comparison_macos.sh` and a portable variant.
* Or fall back to `/usr/bin/time -p` only.

## Things I looked for and did not find as major problems

I specifically checked whether this repo looked like a “docs-first” project with weak implementation backing. It does not. The architecture and release docs line up with a codebase that actually has multiple runtime surfaces, parity testing, benchmark artifacts, and safety fallbacks in the search path.  

I also checked whether the adaptive search path looked completely unguarded. It does not. There is explicit regression coverage aimed at preventing a prior bounds-clamp bug from causing wild expression-count overshoot. 

And I checked whether user-defined functions looked like an afterthought. They do not; parsing and stack validation are real, and there is a regression test that they survive the full search/refinement path.  

## Prioritized remediation plan

First, fix the user-symbol filtering bug. That is the highest-confidence correctness issue and the easiest to explain and test. 

Second, resolve the 16-symbol truncation behavior by turning it into an explicit contract instead of silent behavior.  

Third, reconcile the manifest schema with the runtime output contract. Reproducibility features lose value quickly when the schema drifts. 

Fourth, clean up level/documentation semantics across CLI and programmatic surfaces.  

Fifth, invest in search-stage profiling and candidate-window tuning, since the benchmark evidence says that is where the runtime story is still weakest.  

## Final verdict

This is a very good project. It already has the hard parts that many repositories never get: a coherent architecture, shared engine across surfaces, serious tests, compatibility thinking, and honest benchmark artifacts.  

The issues I found are real, but they are “polish and contract integrity” problems in a mature codebase, not signs of a weak foundation. My concise judgment is:

**Strong codebase, real release-quality potential, but not done tightening the seams.**

The next best step would be to convert this report into a patch plan ordered by risk and effort.
