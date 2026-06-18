# F15 — Editor Integration

> **Status:** Draft
>
> **Version:** 0.1   ·   **Last updated:** 2026-06-17
>
> **Purpose:** How `sqlalchemy-lsp` reaches its first-class editors — a Zed extension, plus Helix, Neovim, and VS Code configuration — and the shared launch contract every one of them uses, layered alongside the user's Python language server.
>
> **Depends on:** [constitution](../constitution.md), [E01-architecture](../foundations/E01-architecture.md), [E03-tech-stack](../foundations/E03-tech-stack.md)   ·   **Related:** [E17-testing](../foundations/E17-testing.md), [E29-e2e-testing](../foundations/E29-e2e-testing.md), [F16-release-ci](F16-release-ci.md), [ADR-002](../decisions/ADR-002-tower-lsp-server-fork.md), [ADR-007](../decisions/ADR-007-companion-to-python-lsp.md)

> Requirement tag: **EDIT**

---

## 1. Purpose & Scope

The server is editor-agnostic by construction (constitution P2); this spec covers the last mile per editor. It says how `sqlalchemy-lsp` launches, which file types it attaches to, which roots define a workspace, and how it sits *beside* the user's Python LSP rather than in place of it (constitution P5, [ADR-007](../decisions/ADR-007-companion-to-python-lsp.md)).

Four editors are first-class targets: Zed, Helix, Neovim, and VS Code. Each gets copy-pasteable config in this spec and the README. Every other LSP-capable editor reaches the server through the same generic stdio path.

This spec covers:

- The shared launch contract — `sqlalchemy-lsp lsp --stdio`, the advertised capabilities, and the file-detection table every editor shares.
- A WASM Zed extension that ships in-repo under `editors/zed/`, plus its `install-`/`package-` scripts.
- Helix (`languages.toml` merge), Neovim (`vim.lsp.config` Lua), and a bespoke VS Code (TypeScript/npm) extension.
- The companion layering rule: our server always runs alongside the user's Python LSP, never instead of it.
- The full Zed marketplace submission checklist for the `zed-industries/extensions` registry.

## 2. Non-Goals / Out of Scope

- The release workflow, cross-compiled binaries, and OS-package publishing (AUR/Homebrew) — owned by [F16-release-ci](F16-release-ci.md). This spec only describes the editor artifacts a release ships.
- Remote transports — stdio is the only transport in v1 ([ADR-005](../decisions/ADR-005-stdio-only-transport.md), [E01](../foundations/E01-architecture.md)). No `--tcp`/`--http` config appears here.
- Generic Python intelligence — ordinary completion, type checking, imports, and generic refactors stay with the user's Python LSP ([ADR-007](../decisions/ADR-007-companion-to-python-lsp.md)).
- What each capability actually returns — owned by the `F01`–`F14` feature specs; this spec only says *which* capabilities the server advertises and *where* it attaches.

## 3. Background & Rationale

`sqlalchemy-lsp` is a specialist, not a generalist. It links a model's foreign keys, relationships, and `back_populates` counterparts across files, and it understands an Alembic migration chain. None of that replaces what Pyright, `pylsp`, or Ruff already do for plain Python. So every editor config here does one thing the sibling babel-lsp config also does: it runs our server *in addition to* the Python language server already in the editor (constitution P5).

That makes the layering load-bearing, not cosmetic. In editors that route a request to a single server (Helix, notably), the ordering decides whose hover and goto-definition win. We default to keeping the Python LSP primary, because its generic intelligence is what a developer reaches for most — and our non-diagnostic features only fire inside SQLAlchemy/Alembic constructs anyway ([ADR-007](../decisions/ADR-007-companion-to-python-lsp.md)).

The server speaks LSP over stdio and needs nothing else to run (REQ-ARCH-01). The per-editor work is therefore configuration, not code — except Zed, which needs a tiny WASM extension to register a third-party server at all.

## 4. Concepts & Definitions

- **First-class target** — an editor `sqlalchemy-lsp` ships ready-to-use config for and checks each release: Zed, Helix, Neovim, VS Code.
- **Generic stdio path** — launching `sqlalchemy-lsp lsp --stdio` from any LSP client that can spawn a command. The lowest common denominator, available to every editor.
- **Companion layering** — running our server *alongside* the user's Python LSP, never instead of it (constitution P5). (Canonical rationale in [ADR-007](../decisions/ADR-007-companion-to-python-lsp.md).)
- **Root marker** — a file or directory whose presence marks a workspace root, so the server scans from the right folder. Canonical config lives in [E15](../foundations/E15-app-config.md).

## 5. Detailed Specification

### 5.1 The shared launch contract

Every editor starts the server the same way and attaches it to the same files. The contract is the constant; the per-editor syntax below just expresses it.

**REQ-EDIT-01 — One launch command everywhere.**

Every editor launches the server by running `sqlalchemy-lsp lsp --stdio`. The binary must be on `PATH`, or the editor must be given an absolute path to it. Bare `sqlalchemy-lsp` with no subcommand is shorthand for `lsp --stdio` (REQ-ARCH-01). No editor passes a different subcommand or a different transport for normal use — stdio is the only transport in v1 ([ADR-005](../decisions/ADR-005-stdio-only-transport.md)).

**REQ-EDIT-02 — Attach to Python files; resolve roots from the shared markers.**

Our facts live in Python source — models in `models/*.py`, migrations under `migrations/versions/*.py` — so the server attaches to the `python` language and nothing else. There is no separate catalog file type to register (unlike babel-lsp's `.po`). Workspace roots resolve from the markers below, nearest first.

| File type | Editor language name | Why the server attaches |
|---|---|---|
| Python | `python` | Models (`mapped_column`, `relationship`, `__tablename__`) and Alembic migrations (`op.*`, `down_revision`) — [F01](F01-orm-correctness-diagnostics.md)/[F13](F13-alembic-support.md). |

| Marker | Meaning |
|---|---|
| `pyproject.toml` | The Python project root; carries `[tool.sqlalchemy-lsp]` config ([E15](../foundations/E15-app-config.md)). |
| `sqlalchemy-lsp.toml` | A dedicated config file when present ([E15](../foundations/E15-app-config.md)). |
| `alembic.ini` | The Alembic root, when the project uses migrations ([F13](F13-alembic-support.md)). |
| `.git` | Repository root, the fallback when no project file is found. |

**REQ-EDIT-03 — Advertise exactly the capabilities the features implement.**

On `initialize`, the server advertises the capability set the `F01`–`F14` features back: incremental text sync, push **and** pull diagnostics, completion (trigger characters include `.` for `op.`), hover, signature help, go-to-definition, find-references, rename (with prepare), document and workspace symbols, inlay hints, code actions, and the `executeCommand` for the schema view ([F12](F12-schema-visualization.md)). It negotiates `positionEncoding`, preferring UTF-8 (REQ-ARCH-10). The advertised set is the contract editors bind against; the [E29](../foundations/E29-e2e-testing.md) lifecycle journey pins it.

**REQ-EDIT-04 — Coexist with the primary Python language server.**

`sqlalchemy-lsp` runs *alongside* the user's Python LSP, never instead of it (constitution P5, [ADR-007](../decisions/ADR-007-companion-to-python-lsp.md)). Its diagnostics are namespaced (`source: "sqlalchemy-lsp"`, codes in the `SQLA-` family), it claims no formatting and no full-file ownership, and its non-diagnostic features fire only inside SQLAlchemy/Alembic constructs so they never shadow a plain-Python suggestion. In editors that need an opt-in to keep the default servers running, every config below preserves them.

### 5.2 Zed (first-class, ships in-repo)

Zed is the only target needing code. Zed cannot launch a third-party language server from settings alone — a thin WASM extension must register it. We ship that extension under `editors/zed/`, built against the `zed_extension_api` crate and compiled to `wasm32-wasip2` (the same toolchain babel-lsp uses; pin the crate version to the user's Zed, see the note below).

The manifest registers the server for the `Python` language, which Zed ships built-in — so unlike babel-lsp's PO case, the extension defines no new language. It carries the registry-required `repository` and `authors` fields ([F16](F16-release-ci.md) and the §5.7 checklist depend on these being exact):

```toml
# editors/zed/extension.toml
id = "sqlalchemy-lsp"
name = "SQLAlchemy LSP"
version = "0.1.0"
schema_version = 1
authors = ["Alex Oleshkevich <alex.oleshkevich@gmail.com>"]
description = "Language server for SQLAlchemy and Alembic: diagnostics, completion, hover, navigation"
repository = "https://github.com/alex-oleshkevich/sqlalchemy-lsp"

[language_servers.sqlalchemy_lsp]
languages = ["Python"]
```

The Rust glue implements one hook — `language_server_command` — returning the command Zed should spawn. It locates the binary with `worktree.which` and passes the worktree's shell environment through, so the server inherits the user's `PATH` and virtualenv:

```rust
// editors/zed/src/lib.rs
fn language_server_command(
    &mut self,
    _language_server_id: &zed::LanguageServerId,   // ignore the id; we serve one server
    worktree: &zed::Worktree,
) -> Result<zed::Command> {
    let env = worktree.shell_env();
    let binary = worktree
        .which("sqlalchemy-lsp")
        .ok_or_else(|| "sqlalchemy-lsp not found in PATH".to_string())?;
    Ok(zed::Command {
        command: binary,
        args: vec!["lsp".into()],
        env,
    })
}
```

**REQ-EDIT-05 — The Zed extension registers the server for Python and locates the binary robustly.**

The extension carries no features beyond two essentials, each a fix for a real failure mode:

1. **It registers the server for the built-in `Python` language** in `extension.toml`. Python is built-in, so the extension defines no new language — it only declares the `language_servers.sqlalchemy_lsp` entry that binds our server to Python buffers.
2. **It locates the binary with `worktree.which`** rather than a bare command name, and ignores the `language_server_id` so the `sqlalchemy_lsp`/`sqlalchemy-lsp` underscore-vs-hyphen mismatch between the manifest and the binary name can never trip it up. It passes the worktree's shell environment through so the spawned server inherits the user's `PATH` and project virtualenv.

> **Note:** the `zed_extension_api` crate version (and the Wasm target, `wasip1` vs `wasip2`) must track the user's Zed version — Zed's extension API moves, and a mismatch fails to load. Pin it in `editors/zed/Cargo.toml` and bump it on Zed API releases.

Declaring a server in a Zed extension does not make Zed run it beside the default Python server. The user opts in by naming it in settings; the `"..."` entry keeps the built-in servers running (constitution P5):

```jsonc
// ~/.config/zed/settings.json
{
  "languages": {
    "Python": { "language_servers": ["sqlalchemy_lsp", "..."] }
  }
}
```

Without this snippet the extension installs but the server never starts. The README shows it next to the install steps, flagged as the most common Zed mistake.

Zed has no command palette entry for arbitrary LSP commands, so the schema-view command of [F12](F12-schema-visualization.md) surfaces in Zed as a **code action** on the relevant range — the only LSP trigger Zed exposes for it.

#### Local install and packaging scripts

Two scripts under `scripts/` support the Zed extension. `install-zed-extension.sh` builds the WASM and copies it into Zed's local `extensions/installed/sqlalchemy-lsp` directory for development, registering it in Zed's `index.json`. `package-zed-extension.sh` builds the WASM for `wasm32-wasip2` and zips `extension.toml` plus the compiled `extension.wasm` into a release artifact named for the crate version; the release job calls it ([F16](F16-release-ci.md)). The packaging step also copies the root `LICENSE` into `editors/zed/`, because the marketplace validator checks the extension directory (§5.7).

### 5.3 Helix

Helix configures the server in `languages.toml` and attaches it to Python alongside the user's existing servers. Merge this into `~/.config/helix/languages.toml`:

```toml
# ~/.config/helix/languages.toml
[language-server.sqlalchemy-lsp]
command = "sqlalchemy-lsp"
args = ["lsp", "--stdio"]

[[language]]
name = "python"
language-servers = ["pyright", "sqlalchemy-lsp"]
```

Order matters in Helix, and it is the heart of the companion rule here. Helix routes hover, goto-definition, and references to the *first* listed server that advertises the capability; only diagnostics, completion, code actions, and symbols merge across servers. With `pyright` first, its hover and goto stay primary — exactly what we want, since the Python LSP owns generic Python ([ADR-007](../decisions/ADR-007-companion-to-python-lsp.md)). Our diagnostics, completion, code actions, and symbols still run and merge in. To make our hover and FK/relationship goto primary instead, list `sqlalchemy-lsp` first and take the reverse trade. The schema command ([F12](F12-schema-visualization.md)) surfaces in Helix as a code action.

### 5.4 Neovim

Neovim needs no plugin beyond the built-in LSP config (Neovim 0.11+). The snippet sets the launch command, the Python filetype, and the root markers, then enables the server:

```lua
-- init.lua (Neovim 0.11+ with built-in vim.lsp.config)
-- or place in after/plugin/sqlalchemy_lsp.lua
vim.lsp.config('sqlalchemy_lsp', {
  cmd = { 'sqlalchemy-lsp', 'lsp', '--stdio' },
  filetypes = { 'python' },
  root_markers = { 'pyproject.toml', 'sqlalchemy-lsp.toml', 'alembic.ini', '.git' },
})
vim.lsp.enable('sqlalchemy_lsp')
```

Neovim attaches every enabled server to a matching buffer, so this config naturally runs alongside the user's Python LSP — no ordering trade like Helix's, because Neovim merges results from all attached servers (constitution P5). The schema command ([F12](F12-schema-visualization.md)) runs from `:lua vim.lsp.buf.execute_command(...)` or appears as a code action via `vim.lsp.buf.code_action()`.

### 5.5 VS Code (bespoke extension, new vs. the siblings)

VS Code is where this suite diverges from babel-lsp, which ships no VS Code extension. We ship a small **TypeScript/npm** extension under `editors/vscode/` that launches the server over stdio and attaches it to Python. It is the only first-class target written in TypeScript rather than configured in the editor's own format.

**REQ-EDIT-06 — A bespoke VS Code extension launches the server over stdio.**

The extension uses `vscode-languageclient` to spawn `sqlalchemy-lsp lsp --stdio` and binds it to the `python` document selector. It resolves the binary from `PATH` (or an absolute path set in a `sqlalchemy-lsp.serverPath` setting), and it does **not** register itself as a Python formatter or take full-file ownership, so it coexists with Pylance/Pyright (constitution P5). The client config is the standard `LanguageClient` shape:

```typescript
// editors/vscode/src/extension.ts
import { LanguageClient, ServerOptions, TransportKind } from "vscode-languageclient/node";

export function activate(context: vscode.ExtensionContext) {
  const server: ServerOptions = {
    command: getServerPath(),          // "sqlalchemy-lsp" on PATH, or the configured absolute path
    args: ["lsp", "--stdio"],
    transport: TransportKind.stdio,
  };
  const client = new LanguageClient(
    "sqlalchemy-lsp",
    "SQLAlchemy LSP",
    server,
    { documentSelector: [{ language: "python" }] },
  );
  context.subscriptions.push(client.start());
}
```

The schema command ([F12](F12-schema-visualization.md)) is registered as a VS Code command in the palette, since VS Code — unlike Zed — exposes one. VS Code also remains reachable through the generic stdio path for users who prefer a community generic-LSP bridge instead of installing this extension.

### 5.6 The `editors/` layout

The four integrations live side by side under `editors/`, mirroring the babel-lsp layout the suite adopts:

```
editors/
  zed/            # WASM extension: extension.toml, Cargo.toml, src/lib.rs, LICENSE (copied in at package time)
  helix/          # languages.toml snippet to merge
  neovim/         # sqlalchemy_lsp.lua snippet
  vscode/         # TypeScript/npm extension: package.json, src/extension.ts
```

The Helix and Neovim files are copy-paste snippets, not installable packages; the Zed and VS Code directories are buildable extensions the release ships ([F16](F16-release-ci.md)).

### 5.7 Zed marketplace submission checklist

Publishing to the `zed-industries/extensions` registry has non-obvious requirements; the validator rejects an extension that misses any of them. This is the runbook.

**REQ-EDIT-07 — The Zed extension meets the marketplace validator's requirements.**

The validator checks the extension *directory*, not just the repo, so the LICENSE and manifest fields must be exactly right:

1. **`LICENSE` (MIT) at the repository root.** The crate already declares `license = "MIT"` ([E03 REQ-STACK-02](../foundations/E03-tech-stack.md)); the full MIT text lives in the root `LICENSE`.
2. **`LICENSE` copied into `editors/zed/`.** The validator checks the extension directory, not the repo root, so `package-zed-extension.sh` copies the root `LICENSE` into `editors/zed/` (§5.2).
3. **`editors/zed/extension.toml` carries the exact metadata** — `repository = "https://github.com/alex-oleshkevich/sqlalchemy-lsp"` and `authors = ["Alex Oleshkevich <alex.oleshkevich@gmail.com>"]` (§5.2).

Submission goes through a fork of the `zed-industries/extensions` repo:

```bash
# in a clone of your fork of zed-industries/extensions
git submodule add https://github.com/alex-oleshkevich/sqlalchemy-lsp \
    extensions/sqlalchemy-lsp
# add an entry to extensions.toml pointing at the submodule's Zed dir:
#   [sqlalchemy-lsp]
#   submodule = "extensions/sqlalchemy-lsp"
#   path = "editors/zed"
#   version = "0.1.0"
pnpm sort-extensions          # CI enforces alphabetical order in extensions.toml
gh pr create --head "alex-oleshkevich:<branch>" --base main
```

The `path = "editors/zed"` key is what points the registry at our in-repo extension subdirectory, and `pnpm sort-extensions` is mandatory — the registry's CI rejects an unsorted `extensions.toml`.

### 5.8 Generic stdio path

Any editor that can spawn a command and speak LSP can run the server through the generic path: it launches `sqlalchemy-lsp lsp --stdio` and attaches it to Python with the §5.1 root markers. There is no extension to install. This is the contract every non-first-class editor uses, and the fallback VS Code path for users who prefer a community generic-LSP bridge over the bespoke extension.

## 7. Visualizations

The table below maps each first-class editor to its integration kind and how the schema command ([F12](F12-schema-visualization.md)) surfaces there.

| Editor | Integration kind | Lives in | Companion layering | Schema command surfaces as |
|---|---|---|---|---|
| Zed | WASM extension (`zed_extension_api`) | `editors/zed/` | `"..."` opt-in in settings keeps default servers | code action |
| Helix | `languages.toml` merge | `editors/helix/` | server order (`pyright` first) | code action |
| Neovim | `vim.lsp.config` Lua | `editors/neovim/` | all attached servers merge | execute-command / code action |
| VS Code | TypeScript/npm extension | `editors/vscode/` | no formatter/ownership claim | palette command |
| any other | generic stdio path | — | client-dependent | client-dependent |

## 9. Examples & Use Cases

A backend engineer on the `clean-blog` project uses Neovim with Pyright already configured. They drop the §5.4 snippet into `init.lua`. Opening `models/post.py`, both servers attach: Pyright handles ordinary Python, and `sqlalchemy-lsp` flags a `ForeignKey("user.id")` that names no real table (`SQLA-E301`) and shows the FK inlay hint `‹→ User.id›` ([F10](F10-inlay-hints.md)). The two never fight — our diagnostics carry `source: "sqlalchemy-lsp"` and our hover fires only on the FK string.

A teammate on Zed installs the in-repo extension with `scripts/install-zed-extension.sh`, then adds the §5.2 settings opt-in so our server runs beside the default Python server. To render the schema ([F12](F12-schema-visualization.md)), they trigger it as a code action, since Zed exposes no command palette for LSP commands. A third teammate on VS Code installs the `editors/vscode/` extension; the schema view appears as a palette command, and Pylance keeps owning hover and type checking.

## 10. Edge Cases & Failure Modes

- Binary missing from `PATH` → each editor surfaces its own "server failed to start" error; the README troubleshooting section covers setting an absolute path (or `sqlalchemy-lsp.serverPath` in VS Code).
- Zed extension installed but no settings opt-in → the server is registered but never starts; the README flags this as the most common Zed mistake (§5.2).
- Helix with the Python server listed first → our hover and goto are silently unavailable on Python (diagnostics/completion/actions/symbols still run); expected and documented in §5.3.
- VS Code extension claiming formatting → would fight Pylance; deliberately avoided (REQ-EDIT-06).
- Two servers fighting over diagnostics → cannot happen; we namespace diagnostics with `source: "sqlalchemy-lsp"` and the `SQLA-` codes (REQ-EDIT-04).
- Unsorted `extensions.toml` in a marketplace PR → the registry CI rejects it; run `pnpm sort-extensions` before opening the PR (§5.7).

## 11. Testing

This spec is delivery and packaging, so most of its plan is integration and manual checks: launching the binary, verifying the Zed extension's manifest, confirming each editor attaches to Python, and asserting the marketplace fields are present.

### 11.1 Scope & coverage

Target: **100% of this feature's behavior is covered.** Every `REQ-EDIT-NN` maps to at least one test or a documented manual check. There are no `sqlalchemy-lsp`-rendered surfaces of our own (no §6), so the coverage here is the launch path, the attach path, the config snippets, and the marketplace metadata. See the policy in [E17 §2](../foundations/E17-testing.md#2-coverage-policy).

### 11.2 Test plan

Each row is a behavior under test. The stdio smoke test is automated ([E29](../foundations/E29-e2e-testing.md)); the editor-config and extension checks are integration or manual, since they exercise third-party editors we do not ship.

| Behavior / scenario | Type | Notes | Verifies |
|---|---|---|---|
| `sqlalchemy-lsp lsp --stdio` launches and answers `initialize` with the advertised capabilities | integration (E29 smoke) | automated over stdio | REQ-EDIT-01, REQ-EDIT-03 |
| `initialize` advertises the full capability set (diagnostics push+pull, completion `.` trigger, hover, signature help, definition, references, rename, symbols, inlay hints, code actions, executeCommand) | integration (E29) | assert each capability in the result | REQ-EDIT-03 |
| The Zed `extension.toml` registers the server for `Python` and carries the exact `repository`/`authors` | integration | parse the shipped `extension.toml`; assert fields | REQ-EDIT-05, REQ-EDIT-07 |
| The Zed glue resolves the binary via `worktree.which` and ignores `language_server_id` | integration | drive `language_server_command` with PATH states and a mismatched id | REQ-EDIT-05 |
| `package-zed-extension.sh` builds `wasm32-wasip2`, zips `extension.toml` + `extension.wasm`, and copies the root `LICENSE` into `editors/zed/` | integration | parse the script (mirrors the F16 `release_assets` checks) | REQ-EDIT-07 |
| Helix / Neovim / VS Code configs launch `sqlalchemy-lsp lsp --stdio` and attach to `python` | manual | apply each config; open a model file and confirm attach | REQ-EDIT-01, REQ-EDIT-02 |
| The server coexists with the primary Python server (namespaced `source`, no formatting claim) | manual | run beside Pyright/Pylance/`pylsp`; confirm both attach | REQ-EDIT-04, REQ-EDIT-06 |
| Helix ordering keeps the Python LSP's hover/goto primary | manual | `pyright` first; confirm Python hover is Pyright's, diagnostics merge | REQ-EDIT-04 |

### 11.3 Fixtures

The launch and attach checks read the [clean-blog](../foundations/E17-testing.md#5-fixtures-registry) workspace from the [E17 fixtures registry](../foundations/E17-testing.md#5-fixtures-registry) — it carries the `models/*.py` and `migrations/versions/*.py` files each editor config must attach to. No feature-local fixtures are needed.

### 11.4 Requirement coverage

Every load-bearing requirement maps to a test or a documented manual check — this table is the proof.

| Requirement | Covered by |
|---|---|
| REQ-EDIT-01 | stdio launch + `initialize` smoke test (E29); per-editor config manual checks |
| REQ-EDIT-02 | Python-attach manual check across editors; root-marker resolution |
| REQ-EDIT-03 | `initialize` capability-set assertion (E29) |
| REQ-EDIT-04 | coexistence manual check beside the Python server; Helix ordering check |
| REQ-EDIT-05 | Zed `extension.toml` parse; binary-discovery + id-ignore tests |
| REQ-EDIT-06 | VS Code extension launch + no-formatter coexistence manual check |
| REQ-EDIT-07 | Zed marketplace metadata parse; `package-zed-extension.sh` LICENSE/zip checks |

## 12. End-to-End Test Plan

The end-to-end surface of this spec is small but load-bearing: the server actually starts over stdio and answers `initialize` with the capabilities every editor binds against. These journeys run against the built binary the way an editor does.

### 12.1 Coverage target

**100% of the feature's scope, end to end** — the canonical launch journey per the editor contract, plus the reachable error path (binary not found). See the policy in [E29 §2](../foundations/E29-e2e-testing.md#2-coverage-policy).

### 12.2 Scenarios

The canonical scenario is the stdio launch-and-`initialize` smoke test that every editor contract reduces to; the others confirm the Python attach and the missing-binary error a user hits on a fresh machine.

| # | Journey | Path | Expected outcome |
|---|---|---|---|
| E2E-01 | Launch `sqlalchemy-lsp lsp --stdio` and send `initialize` per the editor contract | happy | The server answers with the advertised capabilities (REQ-EDIT-03) and negotiates UTF-8. |
| E2E-02 | Open a Python model buffer in a client bound to the `python` language | happy | The server attaches and the buffer receives a diagnostics publish. |
| E2E-03 | Resolve the launch when no `sqlalchemy-lsp` binary is on `PATH` | error | The editor surfaces a clear "server failed to start" error; no server starts. |

### 12.3 Acceptance criteria & Definition of Done

The §12.2 scenarios, written Given/When/Then, are this feature's acceptance criteria:

| # | Given | When | Then |
|---|---|---|---|
| AC-01 | The built `sqlalchemy-lsp` binary | the client launches `sqlalchemy-lsp lsp --stdio` and sends `initialize` | the server replies with the advertised capabilities and negotiates UTF-8. |
| AC-02 | A `clean-blog` workspace and an editor bound to `python` | the client opens `models/post.py` | the server attaches and publishes diagnostics for it, beside the Python LSP. |
| AC-03 | A machine with no `sqlalchemy-lsp` on `PATH` | the editor tries to start the server | it surfaces a clear "server failed to start" error and starts nothing. |

**Definition of Done:** every `REQ-EDIT-NN` has a passing test or documented manual check (§11.4), every acceptance scenario above passes, and the §13.1 security review holds.

## 13. Non-Functional Requirements

### 13.1 Security & Privacy

The trust boundary here is narrow: the extensions and configs only spawn a local binary they locate on the user's own machine.

- **Local launch only** — every editor starts the server by spawning a `sqlalchemy-lsp` binary found on the user's machine; nothing is fetched or executed from the network, so this feature adds no network surface. The server itself reads only local workspace files and runs no user code (constitution P1).
- **Bounded binary discovery** — the Zed extension resolves the binary through `worktree.which`, the VS Code extension through `PATH` or an explicit `sqlalchemy-lsp.serverPath` setting; neither takes an arbitrary path from untrusted input.
- **Environment pass-through** — the spawned server inherits the worktree's own shell environment so it can find the project virtualenv; no new credentials or secrets are introduced, and nothing beyond the user's existing environment is exposed.
- **No data leaves the machine** — the extensions download nothing and phone home nowhere; all work is local stdio between editor and server. No telemetry is sent (the inherited baseline; constitution §4.6).

## 14. Open Questions & Decisions

- **Decision (OQ-ARCH-2, owned by [E01](../foundations/E01-architecture.md))** — resolved to **stdio only** for v1; no `--tcp`/`--http`. Every launch command in this spec uses stdio.
- **Decision** — ship a **bespoke VS Code extension** (`editors/vscode/`), unlike babel-lsp which uses the generic bridge. SQLAlchemy users skew toward VS Code, so a first-class extension is worth the maintenance.
- **Decision** — the Zed extension is LSP-only and locates the binary on `PATH` via `worktree.which`; a configurable binary path is a later enhancement, not v1.

## 15. Cross-References

- **Depends on:** [constitution](../constitution.md) — P2 editor-agnostic and P5 companion; [E01-architecture](../foundations/E01-architecture.md) — the stdio transport (REQ-ARCH-01), advertised capabilities, and encoding negotiation (REQ-ARCH-10); [E03-tech-stack](../foundations/E03-tech-stack.md) — the MIT license and the `wasm32-wasip2` Zed toolchain.
- **Related:** [F16-release-ci](F16-release-ci.md) — packages and publishes the Zed/VS Code artifacts and the OS packages; [F12-schema-visualization](F12-schema-visualization.md) — the command each editor surfaces; [F10-inlay-hints](F10-inlay-hints.md)/[F01](F01-orm-correctness-diagnostics.md) — the facts editors attach to; [ADR-002](../decisions/ADR-002-tower-lsp-server-fork.md) — the `tower-lsp-server` framing crate; [ADR-007](../decisions/ADR-007-companion-to-python-lsp.md) — the companion-not-replacement decision; [E17-testing](../foundations/E17-testing.md)/[E29-e2e-testing](../foundations/E29-e2e-testing.md) — the smoke test and the `clean-blog` fixture.

## 16. Changelog

- **2026-06-17** — Initial draft: the shared launch contract (`sqlalchemy-lsp lsp --stdio`, advertised capabilities, Python-only attach with the project/`alembic.ini`/`.git` root markers), the WASM Zed extension and its install/package scripts, the Helix `languages.toml` merge with its companion-ordering note, the Neovim `vim.lsp.config` snippet, the bespoke VS Code TypeScript extension (new vs. the siblings), the `editors/` layout, the full Zed marketplace submission checklist (root + extension-dir LICENSE, exact `repository`/`authors`, the `zed-industries/extensions` submodule + `pnpm sort-extensions` + `gh pr create` flow), and the §11 Testing + §12 E2E + §13.1 Security plans. No §6 UI Mockups and no §13.2 Accessibility — F15 ships packaging and config, not a rendered surface, and accessibility is the editor's (constitution §4.6).
</content>
</invoke>
