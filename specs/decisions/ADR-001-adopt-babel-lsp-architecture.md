<!--
  ADR — append-only. Never edit Context, Decision, or Consequences once Accepted.
  When reversed, mark Superseded and write a new ADR.
-->

# ADR-001 — Adopt babel-lsp's architecture and skeleton

> **Status:** Accepted
>
> **Date:** 2026-06-17
>
> **Supersedes:** —   ·   **Superseded by:** —

## Context

We started this server as a greenfield repo with two siblings to learn from. `babel-lsp` was a polished, spec-driven Rust language server with mature plumbing: negotiated UTF-8/UTF-16 offsets, a `ropey`-backed document store, a two-pass index pipeline, a headless `check` CLI, and a `pytest-lsp` E2E harness. `sqlalchemy-lsp-legacy` was feature-rich but Zed-only, hand-rolled its test client, and carried none of that protocol maturity. We wanted the legacy project's SQLAlchemy intelligence without inheriting its plumbing gaps, and we wanted this server to feel like a member of the same LSP family.

## Decision

We graft this server onto babel-lsp's architecture and skeleton rather than evolving the legacy codebase.

We adopt its crate set and shape wholesale: `tower-lsp-server` plus `tokio` for the protocol loop, `ropey` for document text, `dashmap` for shared state, and `tree-sitter` for parsing. We adopt its structural patterns: pure-function feature handlers under `src/features/`, a two-pass pipeline (per-file extract, then workspace index), `clap` CLI subcommands, and the `pytest-lsp` stdio E2E harness. We port the legacy server's SQLAlchemy and Alembic behavior as new code written against this skeleton, not as copied files.

## Consequences

Every feature handler is a pure function of shared state plus a request, which makes them trivial to unit-test and keeps features from coupling to each other (constitution Engineering Principles). The two-pass pipeline gives us one place to resolve cross-file references, so a foreign key in one file can find its model in another. We inherit babel-lsp's solved problems for free: correct multi-byte offset handling, debounced relinks, and CLI/server parity from a single engine.

The cost is conformance. We must match babel-lsp's MSRV and edition (see [ADR-002](ADR-002-tower-lsp-server-fork.md)), and we accept its idioms even where the legacy code did something different. Porting behavior as fresh code is slower than copying, but it lets us drop legacy shortcuts and write each feature to the new resilience contract. The architecture is fixed in [E01](../foundations/E01-architecture.md); the crate list in [E03](../foundations/E03-tech-stack.md).

## Alternatives considered

| Alternative | Why not chosen |
|---|---|
| Fork the legacy `sqlalchemy-lsp` code and evolve it | Carries the plumbing gaps we wanted to escape: hand-rolled test client, no offset negotiation, no `check` CLI, Zed-only packaging. Diverges from the LSP family. |
| Start fully from scratch | Throws away babel-lsp's hard-won, battle-tested solutions for UTF-8 offsets, debouncing, and CLI parity. Slower and riskier than grafting onto a proven skeleton. |

## Changelog

- **2026-06-17** — Created.
