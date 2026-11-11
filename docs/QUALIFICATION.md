# Search qualification and bounded evidence

The corpus and tools here test a finite set of claims. Passing them does not
establish universal search completeness, historical ranking parity, or formal
mathematical correctness. Coverage percentages likewise measure executed lines,
not depth of mathematical evidence.

## Reproduce the matcher corpus

```bash
cargo build --release --locked
python3 scripts/qualify_search.py --output output/qualification --repeats 3
```

`tests/corpus/search-contract.json` fixes targets, grammars, weights, and stop
conditions. Each case runs serial, streaming, parallel-requested, and
turbo-requested searches against an empty explicit profile. The report records
the effective matcher, full ordered equation identities, missing/extra equations,
root/error and exact-classification agreement, known-answer coverage, grammar checks, wall time, CLI peak
RSS, candidate-window distribution, Newton work, and independent polynomial
certificates where supported. Raw CLI JSON, the empty profile, and run manifests are retained.
The environment, binary hash, commit, working diff hash, and per-source file
hashes identify the run. Explicit-profile manifests are not self-contained inputs
to `tools/replay_manifest.py`; rerun the corpus to reconstruct those searches.
Process wall time includes startup; compare repeated release runs on the same
machine. A nonzero exit means qualification failed or was incomplete.

CI runs the corpus and uploads its artifacts for 90 days. Download evidence from
the exact commit used for a release. Local evidence from an uncommitted tree is
development evidence and does not qualify a subsequently tagged commit.

The stop-contract Rust oracle separately exhausts tiny length-1/3 grammars,
solves linear/quadratic LHS expressions analytically, and tests a fixed sequence
of randomized symbol restrictions, positive custom weights, and per-side budgets.
It does not use production generation, evaluation, or Newton refinement to
construct expected matches. Run it with `cargo test --test stop_contract`.

## Historical differential

The existing `tests/compare_with_original.sh` remains a historical root-accuracy
smoke test. It checks printed distances; it does not establish expression or
ranking parity.

```bash
python3 scripts/compare_original.py --original /path/to/original-ries \
  --output output/historical
```

The separate differential suite runs four restricted known-answer cases. It
parses the original's first equation, normalizes both equations into exact
rational polynomials, compares them with an independent expected polynomial,
records both costs and spellings, and proves a rational root bound for the shared
root set. Printed distances are not used as its oracle. Different spellings and
costs remain visible in the report: the original and Rust generation schedules,
level budgets, and ranking policies differ. This suite makes no claim about
historical full top-k ordering, and rejects unsupported syntax rather than
silently guessing its meaning. CI uses the checksum-pinned original source.

## Experimental polynomial certificates

```bash
ries-rs 1.4142135623730951 -l0 --classic --json > output/search.json
python3 scripts/algebraic_certificate.py output/search.json \
  --target-literal 1.4142135623730951 --output output/certificates.json
python3 scripts/algebraic_certificate.py output/certificates.json --verify
```

CLI JSON preserves `target_literal` separately from the parsed `target` float.
Keep the original literal when using the experimental certificate tool; do not
reconstruct it from a rounded display value.

The supported grammar is digits 1–9, `x`, `+`, `-`, `*`, negation, square,
and division/reciprocal of nonzero constant polynomials. Every intermediate
polynomial must have degree at most two. Named constants, variable denominators,
transcendental functions, higher degrees, and identities return `unavailable`
with a reason. This conservative grammar avoids branch and pole assumptions.

Certificates contain the original expressions, exact rational polynomial
coefficients, the exact residual at the numerical root, the original decimal
literal, a rational root interval, and a
maximum distance from that literal. The checker recomputes coefficients from the
expressions. It checks either an exact rational root or opposite endpoint signs
plus a strictly signed derivative across the interval. For degree at most two,
the derivative is linear, so its endpoint signs establish monotonicity and root
uniqueness. This is an independent arithmetic check using Python `Fraction`, not
a Lean kernel certificate. Its trust base includes the parser and Python's
arithmetic. It does not prove that the typed decimal equals an irrational root,
or certify a named mathematical constant behind a decimal approximation.

## Enforced resource limits on Linux

`--max-memory` remains a strategy hint. For an OS-enforced process limit use:

```bash
python3 scripts/run_bounded.py --memory-mib 1024 --seconds 300 -- \
  target/release/ries-rs 3.301 -l3 --turbo --json
```

The Linux kernel enforces a virtual-address-space limit with `RLIMIT_AS`, not an
RSS ceiling. Mappings, libraries, and thread stacks count as well as heap data;
allocation failure may abort the program. The supervisor kills the process group
on timeout and disables core dumps. Nonzero exits, signals, timeouts, and partial
stdout always mean an incomplete search. They must not be interpreted as zero
matches. This mode intentionally fails on unsupported operating systems rather
than pretending to enforce a limit. Linux CI tests success, allocation rejection,
and timeout. Avoid materializing large turbo workloads when the process budget
is tight; streaming still stores a RHS database and is not constant-memory.

## Candidate-window measurements

`search_stats.candidate_window_histogram` has 18 counters: width 0, width 1,
widths 2–3, 4–7, …, 32768–65535, and 65536+. Counters merge across turbo workers.
Stop searches can scan twice; both phases count. This exposes the tail beyond
average/max values alongside the widest-window LHS and its derivative. It is
instrumentation, not a narrower numerical gate: no candidate coverage policy is
changed. Future gate changes should run the known-answer and exhaustive suites,
report missing equations, and compare time and memory on this fixed corpus.

The [November 11 candidate baseline](benchmarks/2026-11-11-review-qualification.md)
retains local measured evidence and its explicit scope.

A follow-up review adds an exact decimal acceptance oracle
(`scripts/check_decimal_boundary.py`), actual adaptive-budget checks, and an
adaptive corpus case restricted to its supported serial/parallel modes. Earlier
November 11 artifacts predate these fixes and remain historical development
evidence; the final release must qualify the final tagged commit.

See the [follow-up review evidence](benchmarks/2026-11-11-followup-review.md) for
the decimal oracle, adaptive-bound conformance, explicit verification fallback,
and collision-free Python wheel validation.

## Release-candidate replay and arithmetic boundaries

All target-consuming entry points, including PSLQ, validate the resolved decimal
before dispatch. Match-all-digits means that the binary target represents the
requested decimal precision within half the last typed place; it does not mean
binary/decimal equality. The independent oracle explicitly covers integer loss
at `9007199254740993`, true zero `0e-400`, nonzero underflow `1e-400`, signed `-0`,
overflow/very large exponents, and long trailing zeros. Inputs longer than 4096
characters are rejected as a resource boundary.

Manifests store `config.target_literal` separately from `config.target` and replay
the exact original spelling. Legacy manifests lacking that spelling say that the
original decimal precision contract cannot be reconstructed.

`tools/replay_manifest.py` defaults to exact numeric replay: roots and errors must
have identical IEEE-754 f64 bit patterns, including signed zero. This detects
numerical differences; it does not prove a common compiler/environment. To request
cross-platform tolerant comparison explicitly, use
`--comparison=tolerant --abs-tolerance=1e-12`. The tool reports the comparison mode
and absolute tolerance. Symbol identities, ordering, complexity, exactness, and
recorded configuration must still match.

Polynomial arithmetic has a fixed 4096-bit numerator/denominator budget. Operations
are conservatively preflighted before multiplication or denominator expansion;
over-budget discovery returns unavailable and certificate verification returns
false. Rational input text is also bounded before conversion. Cancellation can
make a reduced result small even when the preflight declines it; acceptance of
every reducible expression is not promised by this experimental tool.

`--pslq --match-all-digits` is explicitly unsupported and returns a configuration
error before results. Decimal validation still applies to PSLQ targets, but a
PSLQ residual tolerance is not a general root-distance guarantee. Use ordinary
numerical search for the match-all-digits result contract.
