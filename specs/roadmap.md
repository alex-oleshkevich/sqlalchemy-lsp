# SQLAlchemy LSP — Roadmap

> **Status:** Living (continuously maintained)
>
> **Last updated:** 2026-06-17
>
> **Purpose:** The order we build in — milestones, foundation-first, each linked to the spec that defines it.

---

## Sequencing principle

Foundation-first: the shared infrastructure (architecture, data model, extraction/indexing, config, conventions) comes before the features that depend on it. Within a milestone, items are in dependency order. Each milestone closes only when its specs are Approved and their Definition of Done (constitution §4.6, §12.3) is met.

## M0 — Foundations

The groundwork every feature stands on.

- [ ] [E01-architecture](foundations/E01-architecture.md) — process model, two-pass pipeline, protocol conduct, performance budgets.
- [ ] [E02-folder-structure](foundations/E02-folder-structure.md) · [E03-tech-stack](foundations/E03-tech-stack.md) — module layout; crates, MSRV, MIT license.
- [ ] [E07-data-model](foundations/E07-data-model.md) — model/column/relationship/Alembic types and the workspace index.
- [ ] [E30-extraction-and-indexing](foundations/E30-extraction-and-indexing.md) — tree-sitter extraction, `Annotated`/forward-ref/base resolution.
- [ ] [E15-app-config](foundations/E15-app-config.md) · [E16-conventions](foundations/E16-conventions.md) — config + suppression; error/resilience + lint rules.
- [ ] [E17-testing](foundations/E17-testing.md) · [E29-e2e-testing](foundations/E29-e2e-testing.md) — fixtures registry, parity, protocol-conformance journeys.

## M1 — Correctness diagnostics

- [ ] [F01-orm-correctness-diagnostics](features/F01-orm-correctness-diagnostics.md) — the legacy correctness core (`SQLA-1xx`–`4xx`).

## M2 — Navigation, hover, symbols

- [ ] [F04-hover](features/F04-hover.md) · [F05-go-to-definition](features/F05-go-to-definition.md) · [F06-find-references](features/F06-find-references.md) · [F08-symbols](features/F08-symbols.md).

## M3 — Completions & signature help

- [ ] [F03-completions](features/F03-completions.md) · [F09-signature-help](features/F09-signature-help.md).

## M4 — Code actions & inlay hints

- [ ] [F11-code-actions](features/F11-code-actions.md) · [F10-inlay-hints](features/F10-inlay-hints.md) · [F07-rename](features/F07-rename.md).

## M5 — Best-practice lints

- [ ] [F02-best-practice-lints](features/F02-best-practice-lints.md) — the new `SQLA-5xx`/`6xx` rules and additions to `1xx`–`4xx`.

## M6 — Alembic

- [ ] [F13-alembic-support](features/F13-alembic-support.md) — migration diagnostics, op completions, jump-to-model.

## M7 — CLI & schema

- [ ] [F14-cli-linter](features/F14-cli-linter.md) · [F12-schema-visualization](features/F12-schema-visualization.md).

## M8 — Editor extensions & marketplace

- [ ] [F15-editor-integration](features/F15-editor-integration.md) — Zed, Helix, Neovim, VS Code + Zed marketplace submission.

## M9 — Release automation

- [ ] [F16-release-ci](features/F16-release-ci.md) — cross-compile matrix, AUR, Homebrew, version gate.

## Future / out of current scope

Parked deliberately, not forgotten:

- **Code Lens** (per-model "N relationships / referenced by M") and **Document Links** (FK string → target model) — babel-lsp has analogues; candidate capabilities, not yet specced.
- **Raw-SQL linting** in `text()`/`execute()` and **schema-drift detection** — explicit Non-Goals; see [ADR-004](decisions/ADR-004-exclude-rawsql-and-drift.md).

## Cross-References

- **Related:** [01-overview](01-overview.md), [index](index.md), [constitution](constitution.md).

## Changelog

- **2026-06-17** — Initial roadmap (M0–M9 + future scope).
