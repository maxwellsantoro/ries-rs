#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
SITE_DIR="$ROOT_DIR/dist/web-site"

if [ ! -f "$ROOT_DIR/pkg/ries_rs.js" ]; then
  echo "error: pkg/ries_rs.js not found; run 'npm run build' first" >&2
  exit 1
fi

if [ ! -d "$ROOT_DIR/web/vendor" ]; then
  echo "error: web/vendor not found; restore vendored browser assets" >&2
  exit 1
fi

rm -rf "$SITE_DIR"
mkdir -p "$SITE_DIR/pkg"

cp "$ROOT_DIR/web/index.html" "$SITE_DIR/index.html"
cp "$ROOT_DIR/web/styles.css" "$SITE_DIR/styles.css"
cp "$ROOT_DIR/web/search-worker.js" "$SITE_DIR/search-worker.js"
cp -R "$ROOT_DIR/web/vendor" "$SITE_DIR/vendor"
cp -R "$ROOT_DIR/pkg/." "$SITE_DIR/pkg/"
if [ -f "$ROOT_DIR/pkg-threads/ries_rs.js" ]; then
  mkdir -p "$SITE_DIR/pkg-threads"
  cp -R "$ROOT_DIR/pkg-threads/." "$SITE_DIR/pkg-threads/"
fi

echo "Prepared static site bundle in $SITE_DIR"
