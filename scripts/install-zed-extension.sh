#!/usr/bin/env bash
# Install the babel-lsp Zed extension for local development.
# Requires: Rust, wasm32-wasip2 target, Zed, python3.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
EXT_DIR="$REPO_ROOT/editors/zed"

echo "Building extension WASM..."
(
  cd "$EXT_DIR"
  cargo build --release --target wasm32-wasip2 2>&1
)

WASM_BIN="$EXT_DIR/target/wasm32-wasip2/release/babel_lsp_zed.wasm"
if [[ ! -f "$WASM_BIN" ]]; then
  echo "Error: WASM binary not found at $WASM_BIN" >&2
  exit 1
fi

if [[ "$(uname)" == "Darwin" ]]; then
  ZED_EXT_BASE="$HOME/Library/Application Support/Zed/extensions"
else
  ZED_EXT_BASE="${XDG_DATA_HOME:-$HOME/.local/share}/zed/extensions"
fi

TARGET="$ZED_EXT_BASE/installed/babel"
INDEX="$ZED_EXT_BASE/index.json"

rm -rf "$TARGET"
mkdir -p "$TARGET"
cp "$EXT_DIR/extension.toml" "$TARGET/"
cp "$WASM_BIN" "$TARGET/extension.wasm"
echo "Copied extension files to $TARGET"

if [[ -f "$INDEX" ]]; then
  python3 - "$INDEX" <<'PYEOF'
import json, sys

index_path = sys.argv[1]
with open(index_path) as f:
    index = json.load(f)

if "extensions" not in index:
    index["extensions"] = {}

index["extensions"]["babel"] = {
    "manifest": {
        "id": "babel",
        "name": "babel",
        "version": "0.1.0",
        "schema_version": 1,
        "description": "LSP-only Zed extension that starts babel-lsp",
        "repository": "https://github.com/alex-oleshkevich/babel-lsp",
        "authors": ["Alex Oleshkevich <techsupport@investerra.ch>"],
        "lib": {"kind": "Rust", "version": "0.7.0"},
        "themes": [],
        "icon_themes": [],
        "languages": [],
        "grammars": {},
        "language_servers": {
            "babel-lsp": {
                "language": None,
                "languages": ["Python"],
                "language_ids": {"Python": "python"},
                "code_action_kinds": None
            }
        },
        "context_servers": {},
        "slash_commands": {},
        "snippets": None,
        "capabilities": []
    },
    "dev": False
}

with open(index_path, "w") as f:
    json.dump(index, f, indent=2)

print(f"Registered babel in {index_path}")
PYEOF
else
  echo "Warning: $INDEX not found — start Zed first, then re-run this script."
fi

echo ""
echo "Done. Restart Zed to activate the extension."
echo "Then add to ~/.config/zed/settings.json:"
echo '  "languages": { "Python": { "language_servers": ["babel-lsp", "..."] } }'
