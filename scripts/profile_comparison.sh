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
PYTHON_BIN="${PYTHON_BIN:-$(command -v python3 || true)}"

# Default settings
VERBOSE=false
QUICK=false
TIME_VERBOSE_FLAG="-p"

case "$(uname -s)" in
    Darwin|FreeBSD|OpenBSD|NetBSD)
        TIME_VERBOSE_FLAG="-l"
        ;;
    Linux)
        TIME_VERBOSE_FLAG="-v"
        ;;
esac

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

print_rust_json_metrics() {
    local json_file=$1

    if [[ -z "$PYTHON_BIN" ]]; then
        return
    fi

    "$PYTHON_BIN" - "$json_file" <<'PY'
import json
import sys

with open(sys.argv[1], "r", encoding="utf-8") as fh:
    payload = json.load(fh)

stats = payload.get("search_stats", {})
metrics = [
    ("search_ms", f"{stats.get('search_ms', 0.0):.3f}"),
    ("candidate_window_avg", f"{stats.get('candidate_window_avg', 0.0):.2f}"),
    ("candidate_window_max", str(stats.get("candidate_window_max", 0))),
    ("strict_gate_rejections", str(stats.get("strict_gate_rejections", 0))),
    (
        "candidates_per_pool_insertion",
        f"{stats.get('candidates_per_pool_insertion', 0.0):.2f}",
    ),
    ("newton_success_rate", f"{100.0 * stats.get('newton_success_rate', 0.0):.1f}%"),
    ("pool_acceptance_rate", f"{100.0 * stats.get('pool_acceptance_rate', 0.0):.1f}%"),
]

for key, value in metrics:
    print(f"  Rust {key}: {value}")
PY
}

# Function to run benchmark
run_benchmark() {
    local name=$1
    local target=$2
    local level=$3
    local json_file
    local time_file
    json_file="$(mktemp)"
    time_file="$(mktemp)"
    local rust_args=("$target" "-l$level" "--json" "--report" "false")
    trap 'rm -f "$json_file" "$time_file"' RETURN

    echo "--- $name (target=$target, level=$level) ---"

    # Run Rust version
    if [[ "$VERBOSE" == "true" ]]; then
        echo "Rust timing output:"
        /usr/bin/time "$TIME_VERBOSE_FLAG" "$RIES_RS" "${rust_args[@]}" >/dev/null
        echo ""
    fi

    { /usr/bin/time -p "$RIES_RS" "${rust_args[@]}" >"$json_file"; } 2>"$time_file"
    RUST_TIME=$(awk '/^real / { print $2 }' "$time_file")
    print_rust_json_metrics "$json_file"

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

    rm -f "$json_file" "$time_file"
    trap - RETURN
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
