#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
SITE_DIR="$ROOT_DIR/dist/web-site"

if [ ! -f "$ROOT_DIR/pkg/ries_rs.js" ]; then
  echo "error: pkg/ries_rs.js not found; run 'npm run build' first" >&2
  exit 1
fi

rm -rf "$SITE_DIR"
mkdir -p "$SITE_DIR/pkg"

cp "$ROOT_DIR/web/index.html" "$SITE_DIR/index.html"
cp -R "$ROOT_DIR/pkg/." "$SITE_DIR/pkg/"

echo "Prepared static site bundle in $SITE_DIR"
