# sqlalchemy-lsp

Language server for SQLAlchemy ORM and Alembic. Diagnostics, completions, hover, navigation, and a headless CLI linter — one Rust binary, any LSP-capable editor.

## Features

| | |
|---|---|
| **Diagnostics** | missing tablename, duplicate tablename/column, unknown FK table/column, FK type mismatch, relationship wiring errors, `back_populates` mismatch, uselist mismatch, orphan cascades, circular relationships, Alembic migration checks |
| **Completions** | FK strings, `relationship()` kwargs and values, `mapped_column()` kwargs, `__table_args__` column names, model-constructor keywords, Alembic `op.*` names and arguments, snippets |
| **Hover** | model card, column card (type, nullability, FK target, indexes), FK resolution, relationship card, cascade tokens, `back_populates` counterpart |
| **Navigation** | go-to-definition, find references, rename — models, columns, relationships, FK targets |
| **Signature help** | model constructors, `relationship()` kwargs |
| **Symbols** | models and Alembic revisions in the symbol picker |
| **Inlay hints** | resolved FK targets inline |
| **Code actions** | generate `__tablename__`, fix `back_populates`, add missing FK column |
| **Schema view** | ER diagram of the workspace models |
| **`check` CLI** | same diagnostics as the LSP — `sqlalchemy-lsp check .` |

### Diagnostics

| Code | Severity | What it catches |
|------|----------|----------------|
| `SQLA-W101` | warning | missing `__tablename__` |
| `SQLA-E102` | error | duplicate `__tablename__` |
| `SQLA-E103` | error | duplicate column name |
| `SQLA-E105` | error | `__table_args__` references unknown column |
| `SQLA-W201` | warning | `nullable=True` column typed as non-optional |
| `SQLA-E301` | error | FK references unknown table |
| `SQLA-E302` | error | FK references unknown column |
| `SQLA-W303` | warning | FK type mismatch |
| `SQLA-E401` | error | relationship target not found |
| `SQLA-W402` | warning | `back_populates` points at wrong attribute |
| `SQLA-W403` | warning | `back_populates` target not found |
| `SQLA-W404` | warning | `uselist` disagrees with cardinality |
| `SQLA-W405` | warning | relationship target mismatch |
| `SQLA-H406` | hint | missing FK for relationship |
| `SQLA-H407` | hint | one-to-one missing `unique=True` |
| `SQLA-W408` | warning | unknown cascade token |
| `SQLA-W409` | warning | `orphan` cascade without `delete` |
| `SQLA-H410` | hint | circular relationship |

Best-practice lints (`SQLA-W1xx`–`SQLA-H4xx`) default on and cover modernization, nullable hygiene, and Alembic migration checks.

## Installation

```bash
uv tool install sqlalchemy-lsp
```

Or with pip:

```bash
pip install sqlalchemy-lsp
```

Or download a pre-built binary from the [releases page](https://github.com/alex-oleshkevich/sqlalchemy-lsp/releases).

## Configuration

Zero config for standard projects. Place a `sqlalchemy-lsp.toml` or `[tool.sqlalchemy-lsp]` section in `pyproject.toml` at the project root.

```toml
# sqlalchemy-lsp.toml

# Path to the declarative base (auto-detected if absent)
# base = "app/database.py"

# Extra source roots for import resolution
# source_roots = ["src"]

[diagnostics]
# Run only these codes
# select = ["SQLA-E301", "SQLA-E401"]

# Suppress codes
ignore = ["SQLA-H406"]

# Override severity per code
[diagnostics.severity]
"SQLA-W303" = "error"
```

Suppress a single line with `# noqa` or `# noqa: SQLA-W303`.

## CLI

```bash
# Run diagnostics
sqlalchemy-lsp check .
sqlalchemy-lsp check app/models/ --select SQLA-E3xx --ignore SQLA-H406
sqlalchemy-lsp check --format json

# Print the workspace ER schema
sqlalchemy-lsp schema

# Print model statistics
sqlalchemy-lsp stats
```

## Editor Setup

### Zed

Install from the Zed extensions panel (`Cmd+Shift+X`) — search for **sqlalchemy-lsp** and click Install. It activates automatically for Python files.

To control server order alongside other language servers, add to `~/.config/zed/settings.json`:

```json
{
  "languages": {
    "Python": { "language_servers": ["sqlalchemy-lsp", "..."] }
  }
}
```

### Helix

Merge `editors/helix/languages.toml` into `~/.config/helix/languages.toml`.

### Neovim

Requires Neovim 0.11+. Use the snippet from `editors/neovim/sqlalchemy_lsp.lua` or add to your `init.lua`:

```lua
vim.lsp.config('sqlalchemy_lsp', {
  cmd = { 'sqlalchemy-lsp', 'lsp', '--stdio' },
  filetypes = { 'python' },
  root_markers = { 'pyproject.toml', 'sqlalchemy-lsp.toml', 'alembic.ini', '.git' },
})
vim.lsp.enable('sqlalchemy_lsp')
```

### Other editors

The server speaks standard LSP over stdio:

```bash
sqlalchemy-lsp lsp --stdio
```

## License

MIT
