# E03 — Tech Stack

> **Status:** Draft
>
> **Version:** 0.1   ·   **Last updated:** 2026-06-17
>
> **Purpose:** The dependencies, language version, license, and toolchain the server is built on — and why each was chosen. The reference to check before adding a crate.
>
> **Depends on:** [constitution](../constitution.md)   ·   **Related:** [E01-architecture](E01-architecture.md), [E02-folder-structure](E02-folder-structure.md), [F16-release-ci](../features/F16-release-ci.md)

> Requirement tag: **STACK**

---

## 1. Purpose & Scope

This spec pins the crates and toolchain the server is built on. It's the place to check before adding a dependency, and the reference for the version assumptions the rest of the suite makes.

The guiding bias is **a boring, proven shape**. The whole family of LSP servers — babel-lsp and this one — share the same stack, so a fix or a lesson in one carries to the other ([ADR-001](../decisions/ADR-001-adopt-babel-lsp-architecture.md)). Every crate below earns its place against the product's static-analysis principle (P1) and that "boring shape" preference. Where two crates could do the job, we pick the one already proven in the sibling server.

This spec covers:

- The language edition, MSRV, and license.
- Every core dependency, its version, and why it's the one we chose.
- How URIs, tree-sitter parsing, and the async runtime are used.
- The `build.rs` build stamp and the dev/test tooling.

## 2. Non-Goals / Out of Scope

- The *architecture* these crates compose into — the two-pass pipeline, the protocol conduct — is owned by [E01-architecture](E01-architecture.md).
- *Where* each crate is used in the tree is owned by [E02-folder-structure](E02-folder-structure.md).
- The release matrix, packaging, and the tag↔version gate are owned by [F16-release-ci](../features/F16-release-ci.md). This spec defines the toolchain those jobs run.

## 3. Background & Rationale

We are not starting from a blank slate. The sibling babel-lsp server already runs a mature, shipped version of this exact stack — `tower-lsp-server`, `tokio`, `ropey`, `dashmap`, `tree-sitter`, `clap`, `tracing` — and the legacy SQLAlchemy LSP independently arrived at most of the same crates. Adopting the babel-lsp set wholesale ([ADR-001](../decisions/ADR-001-adopt-babel-lsp-architecture.md)) buys us a known-good baseline: the UTF-8/UTF-16 offset math, the headless CLI shape, and the file-watching fallback are all solved problems we inherit rather than rediscover. The only domain-specific swap is the grammar — `tree-sitter-python` instead of babel-lsp's Jinja grammar — and the only genuinely new dependency we add for ourselves is none: the stack is identical minus the gettext parser babel-lsp needs and we don't.

## 4. Concepts & Definitions

- **MSRV** — Minimum Supported Rust Version, the oldest toolchain the crate compiles on. CI tests this floor as well as stable.
- **`Uri`** — the URI type `tower-lsp-server` 0.23 uses (a `fluent_uri` newtype from `ls-types`), not the older opaque `lsp-types` URI.
- **Build stamp** — values baked into the binary at compile time by `build.rs`, such as the build timestamp, surfaced in `--version`.

## 5. Detailed Specification

### 5.1 Language, edition, and MSRV

The server targets a recent-but-stable Rust, matching the floor our LSP framework requires.

**REQ-STACK-01 — Rust edition 2024, MSRV 1.85.**

The crate is edition 2024 with a minimum supported Rust version of 1.85 — the floor `tower-lsp-server` 0.23 requires. CI builds on both 1.85 and stable; `rustfmt` and `clippy -D warnings` gate every push ([F16](../features/F16-release-ci.md), conventions in [E16](E16-conventions.md)).

### 5.2 License

The server ships under a permissive license, stated in one place and copied where packaging needs it.

**REQ-STACK-02 — MIT, declared in `Cargo.toml` and a root `LICENSE`.**

`Cargo.toml` sets `license = "MIT"`, and a `LICENSE` file at the repository root carries the full MIT text with the copyright holder. The README states the license too. This matters beyond good manners: the Zed marketplace validator checks for a `LICENSE` in the *extension directory*, so the Zed packaging step copies the root `LICENSE` into `editors/zed/` ([F15](../features/F15-editor-integration.md), [F16](../features/F16-release-ci.md)).

### 5.3 Core dependencies

Each crate below earns its place against constitution P1 (static analysis — we never run user code) and the "boring, proven shape" preference. The version column is the floor we build against.

The `Cargo.toml` `[dependencies]` block reads, in essence:

```toml
# Cargo.toml — runtime dependencies
[package]
name = "sqlalchemy-lsp"
edition = "2024"
rust-version = "1.85"
license = "MIT"

[dependencies]
tower-lsp-server = "0.23"
tokio = { version = "1", features = ["full"] }
ropey = "1"
dashmap = "6"
tree-sitter = "0.25"
tree-sitter-python = "0.25"
clap = { version = "4", features = ["derive"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
toml = "0.8"
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
tracing-appender = "0.2"
notify = "6"
globset = "0.4"
```

And the role each one plays:

| Crate | Version | Role | Why this one |
|---|---|---|---|
| `tower-lsp-server` | 0.23 | LSP framing, JSON-RPC, the `LanguageServer` trait | The maintained community fork of `tower-lsp`: LSP 3.17, `ls-types` URIs, no `async_trait` macro. See [ADR-002](../decisions/ADR-002-tower-lsp-server-fork.md). |
| `tokio` | 1 (`full`) | Async runtime | What `tower-lsp-server` runs on; `spawn_blocking` carries parse and index work off the protocol thread ([E01](E01-architecture.md)). |
| `ropey` | 1 | Rope-backed document text | Cheap incremental edits and clean UTF-8/UTF-16 offset math for position encoding ([E01](E01-architecture.md)). |
| `dashmap` | 6 | Concurrent maps | Lock-free per-entry reads for the document store and the workspace indexes ([E07](E07-data-model.md)). |
| `tree-sitter` | 0.25 | Source parsing | Error-tolerant parse trees, so a half-typed file still yields facts (P3). |
| `tree-sitter-python` | 0.25 | Python grammar | Resolves models, columns, relationships, and `op.*` calls precisely — aliases and attribute access included. |
| `clap` | 4 (`derive`) | CLI parsing | The `lsp` / `check` / `schema` / `stats` subcommands ([F14](../features/F14-cli-linter.md)). |
| `serde` / `serde_json` / `toml` | 1 / 1 / 0.8 | Config + JSON-RPC payloads | Config deserialization and LSP message shapes ([E15](E15-app-config.md)). |
| `tracing` (+ `-subscriber`, `-appender`) | 0.1 / 0.3 / 0.2 | Structured logging | Logs to stderr or a `log_file`, never to stdout, which carries JSON-RPC ([E16](E16-conventions.md)). |
| `notify` | 6 | Native file watching | The fallback when the client can't register `didChangeWatchedFiles` ([E01](E01-architecture.md)). |
| `globset` | 0.4 | Discovery + watched-file matching | Glob matching for model/Alembic auto-discovery and the watched-file set ([E15](E15-app-config.md)). |

> **Note:** Unlike babel-lsp, we carry no gettext parser (`polib`) and no Jinja grammar — those are babel's domain. The stack is otherwise the same, which is the point.

### 5.4 The `Uri` type

URIs are easy to get subtly wrong — Windows paths, percent-encoding, trailing slashes — so all conversion goes through one door.

**REQ-STACK-03 — URIs go through `UriExt`, never string-munged.**

`tower-lsp-server` 0.23 uses the `ls-types` `Uri` (a `fluent_uri` newtype), not the old opaque `lsp-types` URI. Path conversion uses `UriExt::from_file_path` / `UriExt::to_file_path` through one helper in `util/` ([E02](E02-folder-structure.md)); nothing else builds a URI by string formatting. This keeps Windows paths and percent-encoding correct in exactly one place. When we resolve a FK string in `Post.author_id` to the file that defines `User`, the file→URI conversion runs through this helper, not an ad-hoc `format!`.

### 5.5 Tree-sitter usage

Tree-sitter is the heart of the static-analysis principle — we read syntax trees, never a running interpreter.

**REQ-STACK-04 — One Python parser, error-tolerant, read-only.**

Source is parsed with `tree-sitter` + `tree-sitter-python` 0.25 into a concrete syntax tree. Because tree-sitter recovers from errors, a file the user is mid-edit on still parses into a tree with `ERROR` nodes, and the extractor walks whatever it gets and returns what it can (P3, [E30](E30-extraction-and-indexing.md)). We never import the user's modules or call `MetaData` — every fact about the `clean-blog` schema is read from its source tree (P1). Alembic files use the same grammar; there is no second parser.

### 5.6 Async runtime

The server is async at the edges and synchronous in the hot loop, by design.

**REQ-STACK-05 — Tokio at the boundary, `spawn_blocking` for CPU work.**

`tokio` (with the `full` feature set) drives the JSON-RPC loop that `tower-lsp-server` sits on. The protocol thread must stay responsive, so parsing and indexing — the CPU-bound work — runs under `tokio::task::spawn_blocking` rather than blocking an async task ([E01](E01-architecture.md)). The CLI subcommands ([F14](../features/F14-cli-linter.md)) run the same indexing synchronously, since they have no protocol loop to keep alive.

### 5.7 The build stamp

The binary knows when it was built, so `--version` and bug reports are unambiguous.

**REQ-STACK-06 — `build.rs` stamps `BUILD_TIMESTAMP` into the binary.**

A `build.rs` at the crate root records build-time facts — at minimum a `BUILD_TIMESTAMP` — and exposes them to the crate through `cargo`'s `OUT_DIR`/env mechanism. The `--version` output surfaces the crate version (from `Cargo.toml`) alongside the build timestamp. The release job asserts the tag matches the `Cargo.toml` version ([F16](../features/F16-release-ci.md)), so a published binary's reported version is always trustworthy.

### 5.8 Dev and test tooling

The test and quality tooling is part of the stack, even though it isn't a runtime dependency.

**REQ-STACK-07 — `insta` for snapshots, `cargo llvm-cov` for coverage.**

Rendered output that's tedious to assert by hand — hover cards, the schema diagram, CLI console output — is snapshot-tested with `insta` ([E17](E17-testing.md)). Coverage is measured with `cargo llvm-cov`, gating the per-feature 100% requirement. The end-to-end suite is driven by `pytest-lsp` over stdio against the built binary ([E29](E29-e2e-testing.md)); that's a Python dev dependency, not a Rust one. `tempfile` backs tests that need a throwaway workspace on disk.

## 6. Open Questions & Decisions

- **Decision (records [ADR-002](../decisions/ADR-002-tower-lsp-server-fork.md))** — We use `tower-lsp-server` 0.23, the maintained community fork, over the original `tower-lsp`. The original stalled on LSP 3.17 support and still leans on the `async_trait` macro; the fork tracks current protocol features and the `ls-types` `Uri`, which the rest of this spec assumes.
- **Decision** — `ropey` over plain `String` documents (which the legacy server used): incremental sync and UTF-8/UTF-16 offset math are far cleaner on a rope, and we want them right from the start ([E01](E01-architecture.md)).
- **Decision** — The stack mirrors babel-lsp deliberately ([ADR-001](../decisions/ADR-001-adopt-babel-lsp-architecture.md)); divergence has to justify itself against the value of one shared, proven shape across the LSP family.

## 7. Cross-References

- **Depends on:** [constitution](../constitution.md) — P1 (static analysis only) and the "boring, proven shape" engineering principle that gate every crate choice.
- **Related:**
  - [E01-architecture](E01-architecture.md) — how these crates compose into the two-pass pipeline and the `spawn_blocking` model.
  - [E02-folder-structure](E02-folder-structure.md) — where each crate is used, and the `util/` home of the URI helper.
  - [E07-data-model](E07-data-model.md) — the `dashmap`-backed indexes.
  - [E15-app-config](E15-app-config.md) — the `serde`/`toml` config and `globset` discovery.
  - [E16-conventions](E16-conventions.md) — the `clippy -D warnings`/`rustfmt` gates and the never-log-to-stdout rule.
  - [E17-testing](E17-testing.md) / [E29-e2e-testing](E29-e2e-testing.md) — `insta`, `cargo llvm-cov`, and the `pytest-lsp` harness.
  - [F16-release-ci](../features/F16-release-ci.md) — the toolchain the release and QA jobs run, and the tag↔version gate.
  - [ADR-001](../decisions/ADR-001-adopt-babel-lsp-architecture.md), [ADR-002](../decisions/ADR-002-tower-lsp-server-fork.md) — the architecture-adoption and `tower-lsp-server` decisions this stack rests on.

## 8. Changelog

- **2026-06-17** — Initial draft: edition 2024 / MSRV 1.85, MIT license + root `LICENSE`, the `tower-lsp-server` 0.23 + `tokio` + `ropey` + `dashmap` + `tree-sitter-python` stack, the `UriExt` rule, tree-sitter and async-runtime usage, the `build.rs` `BUILD_TIMESTAMP` stamp, and the `insta`/`cargo llvm-cov` dev tooling. Records the `tower-lsp-server` choice against ADR-002.
