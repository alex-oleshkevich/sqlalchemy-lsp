<!--
  ADR — append-only. Never edit Context, Decision, or Consequences once Accepted.
  When reversed, mark Superseded and write a new ADR.
-->

# ADR-006 — Debounce plus a workspace generation counter

> **Status:** Accepted
>
> **Date:** 2026-06-17
>
> **Supersedes:** —   ·   **Superseded by:** —

## Context

Our pipeline runs in two passes: each edit re-extracts the changed file (pass 1), then the workspace re-links cross-file references and rebuilds its indexes (pass 2). The trouble is timing. A user mid-keystroke fires a burst of `didChange` notifications, and a relink kicked off for an early edit can finish *after* a later edit has already changed the facts. Without coordination, pass 2 can read stale per-file data and publish diagnostics that no longer match the buffer — a flicker of wrong warnings, or worse, warnings the user can't make go away. babel-lsp hit and resolved this exact race; we adopt the same answer.

## Decision

We coordinate the two passes with two mechanisms working together.

A fixed debounce of roughly 300 milliseconds collapses an edit burst into a single relink, so pass 2 runs once the typing settles rather than on every keystroke. A monotonic workspace generation counter increments on every change; a relink stamps the generation it started from and, before publishing, checks that the workspace hasn't moved on. If a newer edit has bumped the counter, the stale relink's results are discarded rather than published.

## Consequences

Pass 2 always reads a consistent snapshot, and diagnostics published to the editor always match the current state of the workspace — no flicker, no ghost warnings from an outdated relink. The debounce keeps us from re-linking the whole workspace on every keystroke, which protects the performance budgets in [E01](../foundations/E01-architecture.md) on large projects. The generation check is the safety net the debounce alone can't provide: even when a relink does fire, a result that lost the race is dropped.

The cost is a small, deliberate latency. After the last keystroke, diagnostics appear up to about 300 milliseconds later. That delay is imperceptible in practice and is the price of correctness. We fix the debounce window rather than expose it as a setting — one less knob to misconfigure, and 300ms is a well-tested sweet spot. This is specified in [E01](../foundations/E01-architecture.md).

## Alternatives considered

| Alternative | Why not chosen |
|---|---|
| No debounce — relink on every `didChange` | Re-links the whole workspace per keystroke, blowing the performance budget on large projects, and leaves the stale-publish race wide open. |
| Configurable debounce window | An extra knob with no clear right value for users to pick; 300ms works well enough that exposing it invites misconfiguration. |
| Incremental index instead of debounced full relink | A larger, more complex change for v1; debounce plus generation counter solves the consistency problem now and can coexist with incremental indexing later. |

## Changelog

- **2026-06-17** — Created.
