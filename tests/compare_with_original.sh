#!/bin/bash
# Compare ries-rs output with original RIES
# Usage: ./tests/compare_with_original.sh [target] [level] [max_matches]

ORIGINAL="/Users/maxwell/Apps/ries/ries/ries-original/ries"
RIES_RS="cargo run --quiet --"

TARGET=${1:-"2.5063"}
LEVEL=${2:-"2"}
MAX_MATCHES=${3:-"6"}

echo "=== Comparing target=$TARGET level=$LEVEL max_matches=$MAX_MATCHES ==="
echo ""
echo "=== Original RIES ==="
$ORIGINAL -l$LEVEL --max-matches $MAX_MATCHES $TARGET 2>/dev/null | head -20

echo ""
echo "=== ries-rs ==="
$RIES_RS $TARGET --classic --report false -l $LEVEL --max-matches $MAX_MATCHES 2>/dev/null | head -20

echo ""
echo "=== Done ==="
