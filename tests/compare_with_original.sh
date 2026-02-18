#!/usr/bin/env bash
# Compare ries-rs output with original RIES
# Usage:
#   ./tests/compare_with_original.sh [target] [level] [max_matches] [original_bin]
# Env:
#   RIES_ORIGINAL_BIN=/path/to/ries

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(dirname "$SCRIPT_DIR")"
DEFAULT_ORIGINAL="$REPO_ROOT/../ries-original/ries"

TARGET="${1:-2.5063}"
LEVEL="${2:-2}"
MAX_MATCHES="${3:-6}"
ORIGINAL_BIN="${4:-${RIES_ORIGINAL_BIN:-$DEFAULT_ORIGINAL}}"

if [[ ! -x "$ORIGINAL_BIN" ]]; then
    echo "Original ries binary not found/executable: $ORIGINAL_BIN" >&2
    echo "Pass it as arg 4 or set RIES_ORIGINAL_BIN." >&2
    exit 1
fi

echo "=== Comparing target=$TARGET level=$LEVEL max_matches=$MAX_MATCHES ==="
echo "=== Original binary: $ORIGINAL_BIN ==="
echo ""
echo "=== Original RIES ==="
"$ORIGINAL_BIN" "-l$LEVEL" --max-matches "$MAX_MATCHES" "$TARGET" 2>/dev/null | head -20

echo ""
echo "=== ries-rs ==="
cargo run --quiet -- "$TARGET" --classic --report false -l "$LEVEL" --max-matches "$MAX_MATCHES" 2>/dev/null | head -20

echo ""
echo "=== Done ==="
