#!/bin/bash
#
# Build and run end-to-end Python tests for the ries-py extension in an
# isolated virtual environment.
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

if [[ -z "${PYTHON_BIN}" ]]; then
    echo "python3 is required to run ries-py Python integration tests." >&2
    exit 1
fi

"$PYTHON_BIN" -m venv "$VENV_DIR"

source "$VENV_DIR/bin/activate"

export PYO3_USE_ABI3_FORWARD_COMPATIBILITY="${PYO3_USE_ABI3_FORWARD_COMPATIBILITY:-1}"

python -m pip install --upgrade pip
python -m pip install maturin pytest

rm -rf "$DIST_DIR"
mkdir -p "$DIST_DIR"

(
    cd "$REPO_DIR/ries-py"
    maturin build --interpreter "$VENV_DIR/bin/python" --out "$DIST_DIR"
)

python -m pip install --force-reinstall "$DIST_DIR"/ries_rs-*.whl

python -m pytest "$REPO_DIR/ries-py/tests" "$@"
