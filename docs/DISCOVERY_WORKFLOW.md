# From an entered decimal to a checked hypothesis

Discovery preserves the original decimal spelling and resolved search settings.
It is additive: ordinary `search()` and CLI searches retain their raw interfaces.
The discovery CLI uses the interactive API level scale (`40 + 2*level` per side),
not the ordinary CLI search scale. See [SEARCH_MODEL.md](SEARCH_MODEL.md).

Capture one complete run, using the decimal's known precision rather than adding
unjustified digits:

```bash
ries-rs --discover --json --deterministic -l3 -n8 -- 1.41421356 > run.json
```

`run.json` is a `ries-discovery-v1` envelope. It includes `target_literal`, the
parsed f64 target, resolved grammar/weights/budgets and selection policy, distinct
hypotheses, retained supporting equations, and explicit validation availability.
The browser's full-run JSON download uses this same envelope. Keep that file;
copying only the displayed formula loses the run's provenance.

Select a hypothesis by its zero-based position and generate independent evidence:

```bash
python3 scripts/verify_discovery.py run.json --hypothesis-index 0 --output evidence.json
python3 scripts/verify_discovery.py evidence.json --verify
```

The browser can also export a `ries-discovery-selection-v1` wrapper with `run`
and `hypothesis_index`; pass it directly to the tool without a separate index.
An explicit index that disagrees with that wrapper is rejected.

The tool first tries the representative equation. For `x = sqrt(2)`, that
spelling contains an unsupported square-root symbol, so it can instead use a
retained supporting equation such as `x^2 = 2`. It records which retained equation
was used. The existing independent `Fraction` checker constructs and verifies a
unique-root interval for its conservative degree-at-most-two polynomial grammar.
It does not broaden the certifier's grammar or assume that numerically nearby
equations have the same domain or branches. Retained equations must share the
representative's mechanically normalized solved-postfix identity (only outer
double negation and commutative addition/multiplication are normalized), or its
same unsolved equation identity. Clearly different reported roots are rejected.
These are consistency checks, not proofs that a rearrangement preserves domains.

`evidence.json` is a `ries-discovery-evidence-v1` report. It contains the complete
original run, selected index/key, content hash, and either an independently checked
certificate or an explicit unavailable result with per-equation reasons. The
certificate's arithmetic target trims surrounding whitespace; the original
verbatim literal remains in the attached run and report. Rechecking verifies
the selected source equation/root/literal and report bindings as well as the certificate.
It rejects changed certificates, stale hashes, incompatible schemas, and invalid
indices. A content hash detects accidental drift; someone can fabricate a new run
and hash, so this is not authentication of an executed search.

Read the evidence levels separately:

| Evidence | What it establishes |
| --- | --- |
| Input-precision agreement | The reported f64 root distance fits half the last entered decimal place. |
| Optional high-precision reevaluation | Numerical corroboration when supported. Targets/custom constants still originated as f64. Unsupported builds/operators/requests report unavailable. |
| Independently bounded polynomial root | Only the explicit source equation named in the evidence has a unique root inside the exact rational interval. This does not certify the displayed rearrangement, a broader domain equivalence, or decimal equality. |

For optional numerical corroboration, use a `highprec` build and add
`--precision 192` to the discovery command. The CLI rejects unavailable
verification by default; `--allow-unverified` explicitly permits an unavailable
status. A request never silently becomes a successful verification.

For `x = pi`, the polynomial tool reports unavailable because named constants and
transcendental expressions are outside its grammar. `--verify` can still confirm
the report's integrity and reproduce its unavailable status; its printed result
does not claim that a root was independently bounded.

Reproduce the fixed discovery corpus with
`python3 scripts/qualify_discovery.py --binary target/release/ries-rs --output output/discovery`.
Use [QUALIFICATION.md](QUALIFICATION.md) for corpus scope and artifact meanings,
and [VALIDATION.md](VALIDATION.md) to choose affected checks without rebuilding
unchanged artifacts.
