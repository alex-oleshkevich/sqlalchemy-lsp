#!/usr/bin/env bash
# Build the Zed extension WASM binary and package it for a GitHub Release.
# Usage: ./scripts/package-zed-extension.sh [dist-dir]
set -euo pipefail

DIST="${1:-.}"
ZED_SRC="editors/zed"
WASM_TARGET="wasm32-wasip2"
VERSION=$(cargo metadata --no-deps --format-version 1 | jq -r '.packages[0].version')
OUTPUT="$DIST/babel-lsp-zed-$VERSION.zip"

mkdir -p "$DIST"

rustup target add "$WASM_TARGET"

(
  cd "$ZED_SRC"
  cargo build --release --target "$WASM_TARGET"
)

STAGE=$(mktemp -d)
trap 'rm -rf "$STAGE"' EXIT

cp "$ZED_SRC/extension.toml" "$STAGE/"
cp -r "$ZED_SRC/languages" "$STAGE/"
cp "$ZED_SRC/target/$WASM_TARGET/release/babel_lsp_zed.wasm" "$STAGE/extension.wasm"

(cd "$STAGE" && zip -r "$OLDPWD/$OUTPUT" .)

echo "Packaged Zed extension → $OUTPUT"
