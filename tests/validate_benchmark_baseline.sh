#!/usr/bin/env bash
# Validate inputs and representative Criterion data before starting comparison.
set -euo pipefail
baseline="${1:?baseline name required}"
run_id="${2:?baseline workflow run ID required}"
root="${3:-target/criterion}"
if [[ ! "$baseline" =~ ^[a-zA-Z0-9_-]+$ || ! "$run_id" =~ ^[0-9]+$ ]]; then
    echo 'Baseline must contain only letters, digits, underscores or hyphens; run ID must be numeric.' >&2
    exit 1
fi
for benchmark in eval_simple search_levels lhs_generation; do
    if ! find "$root/$benchmark" -type f -path "*/$baseline/estimates.json" -print -quit 2>/dev/null | grep -q .; then
        echo "Missing Criterion baseline '$baseline' for $benchmark in $root" >&2
        exit 1
    fi
done
