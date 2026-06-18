<!--
  ADR — append-only. Never edit Context, Decision, or Consequences once Accepted.
  When reversed, mark Superseded and write a new ADR.
-->

# ADR-007 — A companion to a Python LSP, not a replacement

> **Status:** Accepted
>
> **Date:** 2026-06-17
>
> **Supersedes:** —   ·   **Superseded by:** —

## Context

Every Python developer who would use us already runs a Python language server — Pyright or Pylance, `python-lsp-server` (`pylsp`), or Ruff's LSP. Those servers are excellent at generic Python: ordinary symbol completion, import resolution, type checking, and general refactors. We faced a choice about our scope. We could try to be a full Python LSP that also understands SQLAlchemy, competing head-on with mature tools. Or we could do one thing the others don't: deep SQLAlchemy and Alembic intelligence, and let the Python LSP keep owning generic Python. Trying to be everything would mean duplicating — and inevitably losing to — Pyright on its home turf, while shadowing its suggestions with worse ones.

## Decision

We are a SQLAlchemy specialist companion, not a general Python language server.

Our non-diagnostic features — completions, code actions, hover, signature help, navigation, and inlay hints — fire only inside SQLAlchemy and Alembic constructs: inside `relationship(...)`, `mapped_column(...)`, `ForeignKey("…")`, `__table_args__`, `op.*`, and the like. In any plain-Python position we return nothing and let the Python LSP answer. We run alongside `pylsp`, Pyright, and Ruff, never replacing them. This is principle P5 in the [constitution](../constitution.md) and the binding rule of [F15](../features/F15-editor-integration.md).

## Consequences

We do the one thing no general Python server does well, and we do it without fighting the tools developers already trust. A user's hover over a plain variable still comes from Pyright; their hover over a column attribute comes from us, richer than any of them. Editor configs layer the two servers — for example, ordering Pyright first for hover — so our specialist answers supplement rather than shadow. This keeps our surface small and our quality high; we never ship a mediocre version of a feature Pyright already nails.

The cost is strict context-gating discipline. Every non-diagnostic feature must correctly detect whether the cursor sits in a SQLAlchemy/Alembic construct before it responds, and a gating bug shows up as either silence where we should help or noise where we should stay quiet. We accept that burden: the E2E plan in [E29](../foundations/E29-e2e-testing.md) pins it with negative tests — a plain-Python position must yield no completions from us. This is also a Non-Goal restated in every feature spec, so the boundary stays visible.

## Alternatives considered

| Alternative | Why not chosen |
|---|---|
| Build a full Python LSP that also understands SQLAlchemy | Duplicates mature tools (Pyright, `pylsp`, Ruff) and would lose to them on generic Python, while shadowing their suggestions with weaker ones. |
| Ship as an editor plugin only, not a language server | Ties us to one editor's API, breaking editor-agnosticism (P2); a standard LSP reaches all four target editors from one binary. |

## Changelog

- **2026-06-17** — Created.
