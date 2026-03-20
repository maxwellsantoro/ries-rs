# Cleanup Remediation Plan

Scope: manifest/schema alignment, CLI level documentation cleanup, and profiling script portability.

## Confirmed Work Items

1. Align the run-manifest contract with the runtime serializer behavior.
2. Remove misleading level-count claims from CLI-facing docs.
3. Make `scripts/profile_comparison.sh` work on both macOS and Linux.

## Implementation Order

1. Update `src/manifest.rs` and `schema/run-manifest-v1.json` together so the schema matches the serialized manifest contract.
2. Rewrite the `Args.level` help text in `src/cli/args.rs` to point readers at the authoritative level-mapping docs instead of hardcoded equation counts.
3. Make `scripts/profile_comparison.sh` select the correct verbose `time` flag for the host platform.
4. Refresh `docs/PERFORMANCE.md` with a short note describing the portable profiling script behavior.

## Verification

1. Add a manifest serialization test in `src/manifest.rs` for optional `stability` handling.
2. Run the focused Rust tests that cover manifest serialization and CLI help text compilation.
3. Smoke-test the profiling script on the current platform with `--quick` and `--verbose`.

## Notes

- This cleanup stays intentionally separate from the search-path and binding fixes in the review reports.
- The runtime CLI already emits `stability` for current manifests, so the schema change is a contract relaxation rather than a behavioral change.
