---
name: ries-delivery
description: Coordinate or implement a bounded ries-rs roadmap packet with parallel agents, exclusive file ownership, focused validation, and evidence reuse. Use for this repository's roadmap delivery and integration, not general Rust tasks or release publication.
---

# Deliver a ries-rs packet

Read [AGENTS.md](../../../AGENTS.md), then the assigned packet in
[ROADMAP.md](../../../docs/ROADMAP.md) and the relevant rows of
[VALIDATION.md](../../../docs/VALIDATION.md). Read API/reference docs only for the
surface being changed. The roadmap records packet scope and delivery status;
code/tests define current behavior.

## Coordinator

- Capture existing dirty changes and their owners before dispatch. Assign bounded
  acceptance criteria, exclusive files, dependencies, and focused checks. Keep
  shared build/schema/navigation files with one owner.
- Start independent ready packets together. Freeze the shared selection interface
  before Python/WASM adapter implementation; let independent fixtures and docs
  proceed earlier. Use the roadmap's ownership handoffs for browser files.
- Do not make each worker build every target. Serialize heavy builds sharing a
  target directory; use separate targets only for necessary concurrent builds.
- Review each completed patch promptly. Integrate once, select affected checks
  from the validation matrix, and reuse worker evidence only with a known unchanged
  relevant input set. Have an independent reviewer inspect integration risks and
  targeted counterexamples instead of duplicating the whole test matrix.

## Worker handoff

Provide the packet/acceptance status, changed paths, public contract/doc impact,
commands and results, source/input identity including untracked fixtures, artifact
paths/hashes where relevant, and unresolved limitations. Preserve others' edits;
request ownership before changing shared files. A focused test passing is evidence
for that test, not the whole suite.

## Closeout

The coordinator records integrated evidence, explicit skips/unavailable checks,
and remaining work. Update roadmap status only when acceptance is met. Keep
release publication separate: [RELEASING.md](../../../RELEASING.md) requires a
clean immutable candidate, full CI, and fresh installed-consumer provenance.
This skill does not authorize tagging, publishing, or deployment.
