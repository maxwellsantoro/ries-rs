#!/bin/bash
#
# Performance comparison between Rust (ries-rs) and C (ries) implementations
#
# This script compares the performance of the Rust rewrite against the original
# C implementation for various target values.
#
# Usage:
#   ./scripts/profile_comparison.sh [options]
#
# Options:
#   -v, --verbose    Show detailed output
#   -q, --quick      Run quick tests only
#   -h, --help       Show this help message

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
RIES_RS_DIR="$(dirname "$SCRIPT_DIR")"
RIES_ORIGINAL_DIR="$(dirname "$RIES_RS_DIR")/ries-original"

# Default settings
VERBOSE=false
QUICK=false

# Parse arguments
while [[ $# -gt 0 ]]; do
    case $1 in
        -v|--verbose)
            VERBOSE=true
            shift
            ;;
        -q|--quick)
            QUICK=true
            shift
            ;;
        -h|--help)
            head -20 "$0" | tail -15
            exit 0
            ;;
        *)
            echo "Unknown option: $1"
            exit 1
            ;;
    esac
done

# Check for binaries
RIES_RS="$RIES_RS_DIR/target/release/ries-rs"
RIES_C="$RIES_ORIGINAL_DIR/ries"

if [[ ! -f "$RIES_RS" ]]; then
    echo "Building ries-rs in release mode..."
    cd "$RIES_RS_DIR"
    cargo build --release
fi

if [[ ! -f "$RIES_C" ]]; then
    echo "Warning: Original C ries not found at $RIES_C"
    echo "Comparison will only show Rust results."
    COMPARE=false
else
    COMPARE=true
fi

# Test values with expected results
if [[ "$QUICK" == "true" ]]; then
    TEST_VALUES=(2.5 3.14159)
else
    TEST_VALUES=(
        2.5
        3.14159265358979    # π
        1.41421356237310    # √2
        2.71828182845905    # e
        1.61803398874989    # φ (golden ratio)
        0.57721566490153    # γ (Euler-Mascheroni)
    )
fi

echo "=== RIES Performance Comparison ==="
echo "Rust binary: $RIES_RS"
if [[ "$COMPARE" == "true" ]]; then
    echo "C binary:    $RIES_C"
fi
echo ""

# Function to run benchmark
run_benchmark() {
    local name=$1
    local target=$2
    local level=$3

    echo "--- $name (target=$target, level=$level) ---"

    # Run Rust version
    if [[ "$VERBOSE" == "true" ]]; then
        echo "Rust output:"
        /usr/bin/time -l "$RIES_RS" "$target" "-l$level" 2>&1 | head -30
        echo ""
    fi

    RUST_TIME=$(/usr/bin/time -p "$RIES_RS" "$target" "-l$level" 2>&1 | grep "real" | awk '{print $2}')

    if [[ "$COMPARE" == "true" ]]; then
        C_TIME=$(/usr/bin/time -p "$RIES_C" "$target" "-l$level" 2>&1 | grep "real" | awk '{print $2}')
        echo "  Rust: ${RUST_TIME}s"
        echo "  C:    ${C_TIME}s"

        # Calculate ratio using bc
        if command -v bc &> /dev/null; then
            RATIO=$(echo "scale=2; $RUST_TIME / $C_TIME" | bc)
            echo "  Ratio: ${RATIO}x"
        fi
    else
        echo "  Rust: ${RUST_TIME}s"
    fi
    echo ""
}

# Run benchmarks
for target in "${TEST_VALUES[@]}"; do
    run_benchmark "Level 2" "$target" 2

    if [[ "$QUICK" != "true" ]]; then
        run_benchmark "Level 3" "$target" 3
    fi
done

echo "=== Summary ==="
echo "The Rust implementation provides similar functionality to the original C version."
echo ""
echo "Key performance notes:"
echo "  - Expression evaluation uses workspace reuse for zero allocation in hot loops"
echo "  - Parallel generation is available with the 'parallel' feature"
echo "  - Thread-local storage is used for fast evaluation without explicit workspace"
echo ""
echo "Run 'cargo bench' for detailed micro-benchmarks."
