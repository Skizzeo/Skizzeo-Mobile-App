#!/usr/bin/env bash
# Baut die Web-Version nach dist/ (Rust -> WebAssembly).
set -euo pipefail
cd "$(dirname "$0")/.."
BUILD="${1:-$(git rev-parse --short HEAD 2>/dev/null || date +%s)}"
cargo build -p skizzeo-app --release --target wasm32-unknown-unknown
rm -rf dist
mkdir -p dist
wasm-bindgen --target web --no-typescript --out-dir dist \
  target/wasm32-unknown-unknown/release/skizzeo-app.wasm
if command -v wasm-opt >/dev/null; then
  wasm-opt -O2 --enable-bulk-memory --enable-nontrapping-float-to-int \
    dist/skizzeo-app_bg.wasm -o dist/skizzeo-app_bg.wasm
fi
cp web/manifest.webmanifest web/icon-*.png dist/
sed "s/__BUILD__/${BUILD}/g" web/index.html > dist/index.html
echo "${BUILD}" > dist/version.txt
echo "Fertig: dist/ (Version ${BUILD})"
