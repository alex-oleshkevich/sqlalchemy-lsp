# Step 3 — opt in via ~/.config/zed/settings.json:
#   "languages": { "Python": { "language_servers": ["sqlalchemy-lsp", "..."] } }

install-zed:
    cargo build --release
    cp target/release/sqlalchemy-lsp ~/.cargo/bin/
    ./scripts/install-zed-extension.sh

build:
    cargo build

test:
    cargo test

lint:
    cargo clippy --all-targets -- -D warnings

fmt:
    cargo fmt

check:
    cargo fmt --check
    cargo clippy --all-targets -- -D warnings
    cargo test
