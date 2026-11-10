#!/usr/bin/env bash
# Compare root accuracy on a shared target, not expression ordering.
# Usage: ./tests/compare_with_original.sh [target] [level] [max_matches] [original_bin]
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(dirname "$SCRIPT_DIR")"
TARGET="${1:-2.5063}"
LEVEL="${2:-2}"
MAX_MATCHES="${3:-6}"
MAX_DISTANCE=1e-8
ORIGINAL_BIN="${4:-${RIES_ORIGINAL_BIN:-$REPO_ROOT/../ries-original/ries}}"
RIES_RS_BIN="${RIES_RS_BIN:-$REPO_ROOT/target/release/ries-rs}"

for bin in "$ORIGINAL_BIN" "$RIES_RS_BIN"; do
    if [[ ! -x "$bin" ]]; then
        echo "Binary not found/executable: $bin" >&2
        exit 1
    fi
done

tmpdir="$(mktemp -d)"
trap 'rm -rf "$tmpdir"' EXIT
# The original uses EXIT_NO_ERROR=-9998, which appears as 242 on Unix.
status=0
"$ORIGINAL_BIN" "-l$LEVEL" --max-matches "$MAX_MATCHES" \
    --max-match-distance "$MAX_DISTANCE" "$TARGET" >"$tmpdir/original" || status=$?
if [[ "$status" != 0 && "$status" != 242 ]]; then
    echo "Original RIES failed with status $status" >&2
    exit 1
fi
"$RIES_RS_BIN" "$TARGET" --classic --report false -l "$LEVEL" --max-matches "$MAX_MATCHES" \
    --max-match-distance "$MAX_DISTANCE" >"$tmpdir/rust"
cat "$tmpdir/original" "$tmpdir/rust"
python3 "$SCRIPT_DIR/validate_parity.py" "$MAX_DISTANCE" "$tmpdir/original" "$tmpdir/rust"
