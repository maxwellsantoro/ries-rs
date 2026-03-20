#!/bin/bash
#
# Run the Rust-side test suite for the PyO3 binding crate without building the
# extension-module flavor. This keeps the test binary linkable while still
# validating the Rust helper logic used by the Python surface.
#
# Usage:
#   ./scripts/test_ries_py_rust.sh
#   ./scripts/test_ries_py_rust.sh test_build_gen_config_includes_preset_user_constants

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_DIR="$(dirname "$SCRIPT_DIR")"

if ! command -v python3 >/dev/null 2>&1; then
    echo "python3 is required to run ries-py Rust-side tests." >&2
    exit 1
fi

PYTHON_BIN="${PYO3_PYTHON:-$(command -v python3)}"

export PYO3_PYTHON="$PYTHON_BIN"

# PyO3 0.22 officially supports up to Python 3.13. Allow the stable ABI test
# path to proceed when the local interpreter is newer (for example Python 3.14).
export PYO3_USE_ABI3_FORWARD_COMPATIBILITY="${PYO3_USE_ABI3_FORWARD_COMPATIBILITY:-1}"

exec cargo test \
    --manifest-path "$REPO_DIR/ries-py/Cargo.toml" \
    --locked \
    --no-default-features \
    --features parallel \
    "$@"
