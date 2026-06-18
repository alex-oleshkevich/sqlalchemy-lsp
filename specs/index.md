# SQLAlchemy LSP — Specification Index

> **Status:** Living (continuously maintained)
>
> **Last updated:** 2026-06-17
>
> **Purpose:** The map of the whole specification suite — every spec, what it defines, when to load it, and how finished it is. Start here.

`sqlalchemy-lsp` is a Rust language server giving editors SQLAlchemy- and Alembic-aware intelligence, with a matching headless CLI. The suite is organized foundation-first: meta-docs set the rules, foundations describe **how** the server is built, and features describe **what** each capability does.

**Foundation specs describe _how_ the app is built. Feature specs describe _what_ each feature does** and own concrete diagnostics, commands, and editor surfaces.

## Status legend

✅ Approved · 📝 In Review · ✏️ Draft · ♻️ Deprecated · ⛔ Rejected

## Tier 1 — Meta

| Spec | Purpose | Load this when | Status |
|---|---|---|---|
| [constitution](constitution.md) | Governing principles and authoring conventions | Writing or reviewing any spec | ✅ |
| [glossary](glossary.md) | Canonical definition of every domain term | A term is unclear | ✅ |

## Tier 2 — Product

| Spec | Purpose | Load this when | Status |
|---|---|---|---|
| [01-overview](01-overview.md) | What the server is, in plain language | Onboarding to the project | ✅ |
| [roadmap](roadmap.md) | Build order — milestones M0–M9 | Planning what to build next | ✅ |

## Tier 3 — Foundations

| Spec | Purpose | Load this when | Status |
|---|---|---|---|
| [E01-architecture](foundations/E01-architecture.md) | Process model, two-pass pipeline, protocol conduct, perf budgets | Understanding how it all fits | ✏️ |
| [E02-folder-structure](foundations/E02-folder-structure.md) | `src/` layout, layering rule, test split | Adding a module | ✏️ |
| [E03-tech-stack](foundations/E03-tech-stack.md) | Crates, versions, MSRV, license | Touching dependencies | ✏️ |
| [E07-data-model](foundations/E07-data-model.md) | Model/column/relationship/Alembic types, workspace index | Touching extracted data | ✏️ |
| [E15-app-config](foundations/E15-app-config.md) | Config sources, keys, `# noqa` suppression | Adding a setting or rule toggle | ✏️ |
| [E16-conventions](foundations/E16-conventions.md) | Error/resilience contract, lint/layering rules | Writing any handler | ✏️ |
| [E17-testing](foundations/E17-testing.md) | Coverage policy, categories, fixtures registry, parity | Writing any feature's test plan | ✏️ |
| [E29-e2e-testing](foundations/E29-e2e-testing.md) | E2E coverage policy, harness, protocol-conformance journeys | Writing a feature's E2E plan | ✏️ |
| [E30-extraction-and-indexing](foundations/E30-extraction-and-indexing.md) | Tree-sitter extraction, `Annotated`/forward-ref/base resolution | Touching parsing/indexing | ✏️ |

## Tier 4 — Features

| Spec | Purpose | Load this when | Status |
|---|---|---|---|
| [F00-template](features/F00-template.md) | Boilerplate for new feature specs | Starting a new feature | — |
| [F01-orm-correctness-diagnostics](features/F01-orm-correctness-diagnostics.md) | Correctness findings (`SQLA-1xx`–`4xx`) | Working on diagnostics | ✏️ |
| [F02-best-practice-lints](features/F02-best-practice-lints.md) | Best-practice lints (`SQLA-5xx`/`6xx` + more) | Working on lints | ✏️ |
| [F03-completions](features/F03-completions.md) | Context-aware completions + snippets | Working on completion | ✏️ |
| [F04-hover](features/F04-hover.md) | Model/column/relationship hover cards | Working on hover | ✏️ |
| [F05-go-to-definition](features/F05-go-to-definition.md) | Jump to model/column/counterpart | Working on navigation | ✏️ |
| [F06-find-references](features/F06-find-references.md) | Find model/column/relationship refs | Working on references | ✏️ |
| [F07-rename](features/F07-rename.md) | Workspace rename of models/columns/relationships | Working on rename | ✏️ |
| [F08-symbols](features/F08-symbols.md) | Document + workspace symbols | Working on symbols | ✏️ |
| [F09-signature-help](features/F09-signature-help.md) | Signatures for FK/relationship/mapped_column/op/constructor | Working on signature help | ✏️ |
| [F10-inlay-hints](features/F10-inlay-hints.md) | Inline FK/relationship hints | Working on inlay hints | ✏️ |
| [F11-code-actions](features/F11-code-actions.md) | Quick fixes (parity with `--fix`) | Working on code actions | ✏️ |
| [F12-schema-visualization](features/F12-schema-visualization.md) | Schema diagram (Mermaid/Graphviz/ASCII) | Working on schema view | ✏️ |
| [F13-alembic-support](features/F13-alembic-support.md) | Migration diagnostics, op completions, jump-to-model | Working on Alembic | ✏️ |
| [F14-cli-linter](features/F14-cli-linter.md) | Headless `check`/`schema`/`stats` | Working on the CLI | ✏️ |
| [F15-editor-integration](features/F15-editor-integration.md) | Zed/Helix/Neovim/VS Code + marketplace | Packaging for an editor | ✏️ |
| [F16-release-ci](features/F16-release-ci.md) | Cross-compile, AUR, Homebrew, version gate | Working on release/CI | ✏️ |

## Decisions

| ADR | Decision | Date | Status |
|---|---|---|---|
| [ADR-001](decisions/ADR-001-adopt-babel-lsp-architecture.md) | Adopt babel-lsp's architecture/skeleton | 2026-06-17 | ✅ |
| [ADR-002](decisions/ADR-002-tower-lsp-server-fork.md) | Use `tower-lsp-server` 0.23 | 2026-06-17 | ✅ |
| [ADR-003](decisions/ADR-003-comprehensive-lints-defaults.md) | Lints configurable, default-on except 2 heuristics | 2026-06-17 | ✅ |
| [ADR-004](decisions/ADR-004-exclude-rawsql-and-drift.md) | Exclude raw-SQL linting + schema drift | 2026-06-17 | ✅ |
| [ADR-005](decisions/ADR-005-stdio-only-transport.md) | stdio-only transport for v1 | 2026-06-17 | ✅ |
| [ADR-006](decisions/ADR-006-debounce-and-generation-counter.md) | Debounce + generation counter | 2026-06-17 | ✅ |
| [ADR-007](decisions/ADR-007-companion-to-python-lsp.md) | Companion to a Python LSP, not a replacement | 2026-06-17 | ✅ |
| [ADR-008](decisions/ADR-008-default-off-missing-column-comment.md) | `SQLA-I207` ships off by default (amends ADR-003) | 2026-06-18 | ✅ |

## Deprecated

| Spec | Superseded by | Status |
|---|---|---|
| <none yet> | | |

## Rejected

| Spec | Why rejected | Status |
|---|---|---|
| <none yet> | | |

## Out of scope

Raw-SQL linting in `text()`/`execute()`, schema-drift detection against a live database, and migration autogeneration are deliberately excluded — see [ADR-004](decisions/ADR-004-exclude-rawsql-and-drift.md). Generic Python intelligence belongs to the user's Python LSP — see [ADR-007](decisions/ADR-007-companion-to-python-lsp.md).

## Maintenance rule

When you author or change a spec, update its row here in the same edit. When a spec is **deprecated**, move it to `deprecated/` and list it; when a proposal is **rejected**, move it to `rejected/` and list it.

## Changelog

- **2026-06-18** — `SQLA-I207` (missing-column-comment) now ships **off by default** ([ADR-008](decisions/ADR-008-default-off-missing-column-comment.md), amending ADR-003): the off-by-default set is now three rules (`H416`/`H602` as shaky heuristics, `I207` as opt-in style). Propagated through `F02` (v0.4) and `E15` (v0.4).
- **2026-06-18** — Suite refinements: dropped the `naming_convention` config key (read from code); generalized alias resolution in `E30` (+ alias test matrix/fixtures in `E17`); adapted seven patterns from Biome — safe/unsafe fixes (`F11`/`F14`), single-traversal diagnostics engine + lazy code-action resolve (`E01`), the diagnostic model with structured advices, tags, and `FixKind` (`E16`), config `overrides`/group-tokens/presets + central code registry (`E15`), and `Deprecated` LSP tags on modernization lints (`F02`). Version bumps across E01/E15/E16/E17/E30 and F01/F02/F11/F14.
- **2026-06-17** — Initial index: meta + product approved; foundations (incl. appended `E30`), 16 features, and 7 ADRs registered as Draft.
