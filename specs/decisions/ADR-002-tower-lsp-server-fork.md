<!--
  ADR — append-only. Never edit Context, Decision, or Consequences once Accepted.
  When reversed, mark Superseded and write a new ADR.
-->

# ADR-002 — Use the `tower-lsp-server` fork

> **Status:** Accepted
>
> **Date:** 2026-06-17
>
> **Supersedes:** —   ·   **Superseded by:** —

## Context

We needed a Rust crate to handle the LSP protocol loop: framing, request routing, and the JSON-RPC wiring underneath every feature. The original `tower-lsp` crate was the obvious starting point, but its maintenance had stalled, and unmerged fixes and protocol updates were piling up. The community had converged on `tower-lsp-server`, an actively maintained fork that tracks newer `lsp-types` and keeps pace with the spec. Having committed to babel-lsp's skeleton in [ADR-001](ADR-001-adopt-babel-lsp-architecture.md), we wanted the same protocol foundation it already runs on.

## Decision

We use `tower-lsp-server` 0.23, the maintained fork, instead of the original `tower-lsp`.

## Consequences

We get a maintained protocol layer with current `lsp-types` and ongoing fixes, and we stay aligned with babel-lsp so patterns and code transfer cleanly between the two. The fork pins our toolchain floor: it requires Rust edition 2024 and an MSRV of 1.85, which we adopt across the project and pin in [E03](../foundations/E03-tech-stack.md). That floor is high enough that very old toolchains can't build us, but 1.85 is widely available and the CI matrix tests against it.

The trade-off is dependence on a fork rather than the canonical crate. If `tower-lsp-server` itself stalls, we inherit that risk. We judged an actively maintained fork to be a lower risk than the stalled original, and the abstraction is thin enough that a future migration to another LSP layer would touch only `src/server.rs`, not the pure-function features.

## Alternatives considered

| Alternative | Why not chosen |
|---|---|
| Original `tower-lsp` | Maintenance stalled, with unmerged fixes and lagging `lsp-types`. Adopting it would mean inheriting bugs the fork already fixed. |
| `lsp-server` + `lsp-types` directly | Drops the Tower service abstraction and async ergonomics, forcing us to hand-write request routing and lifecycle plumbing that the fork gives us. Also diverges from babel-lsp. |

## Changelog

- **2026-06-17** — Created.
