<!--
  ADR — append-only. Never edit Context, Decision, or Consequences once Accepted.
  When reversed, mark Superseded and write a new ADR.
-->

# ADR-005 — stdio-only transport for v1

> **Status:** Accepted
>
> **Date:** 2026-06-17
>
> **Supersedes:** —   ·   **Superseded by:** —

## Context

A language server has to choose how the editor talks to it. The Language Server Protocol allows several transports: standard input/output, a TCP socket, even an HTTP wrapper. We weighed whether v1 should offer a choice. Every editor we target — Zed, Helix, Neovim, and VS Code — launches a server as a child process and speaks to it over stdio. None of them needs a socket. Offering more transports would mean more flags, more lifecycle code, and more error paths to test, for capability nobody on our list would use.

## Decision

Version 1 ships stdio transport only. There is no `--tcp` flag and no `--http` wrapper; `sqlalchemy-lsp lsp --stdio` (and the bare `sqlalchemy-lsp`) is the single way the editor connects.

## Consequences

We have one transport to build, document, and test, which keeps the launch contract in [F15](../features/F15-editor-integration.md) simple and identical across all four editors. stdio reaches every target editor today, so this restriction costs real users nothing. The protocol-conformance E2E harness drives the binary over stdio, matching exactly how editors run it — the tests exercise the only path that ships.

The trade-off is that a few niche setups become impossible for now: attaching a debugger to a long-lived server over a socket, or running the server on a different host than the editor. Neither is a v1 requirement. Because the transport is isolated in `src/server.rs`, adding a socket later would be an additive change behind a new flag, not a rewrite — so this decision is cheap to revisit if a real need appears.

## Alternatives considered

| Alternative | Why not chosen |
|---|---|
| TCP socket transport (`--tcp`) | No target editor needs it; adds lifecycle and error-handling code for a path none of our users would exercise. |
| HTTP wrapper | Heavier still, with its own server lifecycle and security surface, for zero benefit to the editors we support. |

## Changelog

- **2026-06-17** — Created.
