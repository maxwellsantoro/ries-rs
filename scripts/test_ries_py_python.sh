#!/bin/bash
#
# Build and run end-to-end Python tests for the ries-py extension in an
# isolated virtual environment.
#
# Environment management uses `uv` (per portfolio standard); the native
# extension itself is built with `maturin`.
#
# Usage:
#   ./scripts/test_ries_py_python.sh
#   ./scripts/test_ries_py_python.sh -k smoke

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_DIR="$(dirname "$SCRIPT_DIR")"
VENV_DIR="${RIES_PY_TEST_VENV:-$REPO_DIR/.venv-ries-py-test}"
PYTHON_BIN="${PYTHON_BIN:-$(command -v python3)}"
DIST_DIR="${RIES_PY_TEST_DIST:-$REPO_DIR/.tmp-ries-py-dist}"

if ! command -v uv >/dev/null 2>&1; then
    echo "uv is required to run ries-py Python integration tests." >&2
    echo "Install it from https://docs.astral.sh/uv/ (e.g. 'pipx install uv')." >&2
    exit 1
fi

if [[ -z "${PYTHON_BIN}" ]]; then
    echo "python3 is required to run ries-py Python integration tests." >&2
    exit 1
fi

# Create an isolated environment with uv, seeded from the resolved interpreter.
# --clear replaces any stale environment from a previous run.
uv venv --clear --python "$PYTHON_BIN" "$VENV_DIR"

source "$VENV_DIR/bin/activate"

export PYO3_USE_ABI3_FORWARD_COMPATIBILITY="${PYO3_USE_ABI3_FORWARD_COMPATIBILITY:-1}"

uv pip install maturin pytest

rm -rf "$DIST_DIR"
mkdir -p "$DIST_DIR"

(
    cd "$REPO_DIR/ries-py"
    maturin build --interpreter "$VENV_DIR/bin/python" --out "$DIST_DIR"
)

uv pip install --reinstall "$DIST_DIR"/ries_rs-*.whl

python -m pytest "$REPO_DIR/ries-py/tests" "$@"
