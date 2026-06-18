# Constitution

> **Status:** Approved
>
> **Version:** 1.0   ·   **Last updated:** 2026-06-17
>
> **Purpose:** The governing rules for both the product and its specs — the principles the SQLAlchemy LSP must honor, and the conventions every spec in this suite must follow.

---

## 1. Purpose & Scope

This document is the single source of authority for the suite. It governs two things: the **non-negotiable principles** the server must honor (§2–§3), and the **authoring rules** every spec obeys (§4). When any other spec conflicts with this one, this one wins.

The product is `sqlalchemy-lsp` — a Language Server that gives editors SQLAlchemy- and Alembic-aware intelligence: diagnostics, completions, hover, navigation, refactors, inlay hints, a schema view, and a headless CLI for CI. It is a **companion** to a general Python language server, not a replacement.

## 2. Product Principles

The non-negotiable beliefs the product is built on. Specs cite these as "per P3".

| # | Principle | What it means |
|---|---|---|
| P1 | **Static analysis only** | We read source through tree-sitter; we never import user modules, never call `MetaData`, never run the app. Every fact is derived from the syntax tree, so a broken or half-typed file is safe to analyze. |
| P2 | **Editor-agnostic** | We speak standard LSP over stdio and rely on no proprietary editor API. A feature that only works in one editor doesn't ship. |
| P3 | **Never panic on partial code** | The user is mid-keystroke most of the time. Extractors walk whatever tree-sitter produced, `ERROR` nodes and all, and return what they can. A handler that hits a bug returns an empty result, never a crash. |
| P4 | **Only diagnose what's positively wrong** | We stay silent on anything we can't resolve. A relationship target we can't see, a dynamic table name, an unparsed type — none of these produce a guess or a false warning. No speculation. |
| P5 | **Companion, not replacement** | The user's Python LSP (Pyright, `pylsp`, Ruff) owns generic Python — ordinary completion, type checking, imports. We fire only inside SQLAlchemy/Alembic constructs and never shadow plain-Python suggestions. |
| P6 | **Fast enough to forget it's there** | Latency is a feature. Edits re-analyze a single file; the workspace relink is debounced; requests answer from an in-memory index. The budgets in [E01](foundations/E01-architecture.md) are testable, not aspirational. |

## 3. Engineering Principles

The values that guide technical decisions, stated as intent rather than numbered law:

- **One source of truth for every fact.** A model, column, or revision is extracted once and stored once in the workspace index; features read it, never re-parse.
- **Dependencies flow downward.** Features depend on foundations; foundations don't depend on features. Features never call each other.
- **Features are pure functions.** A capability takes the shared state plus a position and returns an LSP response. No hidden state, no cross-feature coupling.
- **One engine, two front-ends.** The editor server and the `check` CLI run the *same* diagnostics engine, so they can never disagree.
- **Degrade, don't fail.** Partial input yields partial results (P3); an unresolvable reference yields silence (P4).

## 4. Authoring Conventions

### 4.1 Document template

Every spec follows `templates/spec-template.md` (scaffolded as `features/F00-template.md`): the metadata header, then the numbered sections. The always-required sections are **Purpose & Scope, Detailed Specification, Cross-References, and Changelog**; every feature also carries **Testing** and **Security & Privacy (§13.1)**.

### 4.2 Naming & ID schemes

- **Files:** prefix + number + kebab slug. `E##` engineering foundations, `F##` features. There is **no `D##` design band** — a headless LSP has no UI design system. Meta-docs are `index.md`, `constitution.md`, `glossary.md`; the overview is `01-overview.md`.
- **Reserved names:** foundation and meta names follow the spec-writer **reserved-names registry**. We use `E01` architecture, `E02` folder-structure, `E03` tech-stack, `E07` data-model, `E15` app-config, `E16` conventions, `E17` testing, `E29` e2e-testing. Extraction/indexing has no reserved slot, so we **appended `E30-extraction-and-indexing`** to the registry before using it. Editor-integration, CLI, and release/CI are **features**, not foundations.
- **Requirement IDs:** each spec declares a short uppercase tag (e.g. `ARCH`, `DIAG`); load-bearing rules are `REQ-ARCH-01`, open questions `OQ-ARCH-01`.
- **Diagnostic codes:** user-facing findings use `SQLA-<SEV><CLASS><NN>` — the `SQLA-` namespace avoids collisions with pycodestyle/flake8 (`E`/`W`/`F`) and flake8-sqlalchemy (`SQA`). `SEV` is the default severity letter (`E`/`W`/`I`/`H`), the hundreds digit is the diagnostic class, the last two digits the rule. The code is a **stable identifier** — it never changes when a user overrides the severity. The catalog lives in [F01](features/F01-orm-correctness-diagnostics.md)/[F02](features/F02-best-practice-lints.md)/[F13](features/F13-alembic-support.md).

### 4.3 Crosslinking & the index

Specs link to each other inline and list every connection in their Cross-References section. [index.md](index.md) is updated in the **same edit** as any spec change.

### 4.4 Testing & coverage

Every feature ships a test plan in its **Testing** section covering **100% of its scope** — each `REQ-<TAG>-NN` maps to a test, and every editor-surface state and edge case is covered. Shared rules, tools, and the reusable **fixtures registry** live in [E17-testing](foundations/E17-testing.md); the end-to-end harness and the shared protocol-conformance journeys live in [E29-e2e-testing](foundations/E29-e2e-testing.md). User-facing features add an **End-to-End Test Plan** covering the happy path and every reasonable error path. The `check` CLI and the server must emit identical findings (CLI/server parity).

### 4.5 Status lifecycle & changelog

A spec moves `Draft → In Review → Approved`, and can end in one of two terminal states:

- **Deprecated** — was Approved, now superseded. Set the status and move the file to `deprecated/`.
- **Rejected** — considered and turned down. Set the status and move the file to `rejected/`.

Archived specs keep their name; the index lists them so the trail stays visible. Every change gets a dated changelog entry; versions bump on meaningful change. ADRs in `decisions/` are append-only — supersede, never edit.

### 4.6 Non-functional & operational scope

Decided once here; every feature spec includes exactly the sections enabled below.

| Concern | Spec section | Status |
|---|---|---|
| Security & Privacy | §13.1 | **Required** |
| Accessibility | §13.2 | **N/A** — headless server; the editor renders all UI. The one content rule we keep: diagnostic severity is conveyed in words/codes, never color alone. |
| Permissions & Roles | §13.3 | **N/A** — single-user developer tool, no auth surface. |
| Performance & Scale | §13.4 | **Enabled, defined once** — budgets live in [E01 §8](foundations/E01-architecture.md) and are regression-tested via the `large-workspace` fixture in [E17](foundations/E17-testing.md). Feature specs do **not** restate them. |
| Observability | §13.5 | **Enabled, lightweight** — structured `tracing` to stderr or `log_file` only; never stdout (it carries JSON-RPC). Per-feature sections are N/A. |
| Rollout & Migration | §14 | **N/A** — versioned binaries with no persistent server state; upgrading is replacing a binary. |
| Acceptance criteria & DoD | §12.3 | **Enabled** — each feature's E2E scenarios double as Given/When/Then acceptance criteria. |

## 5. The Recurring Example Cast

To keep examples concrete and comparable, every spec draws from the same small SQLAlchemy 2.0 schema — the **`clean-blog`** workspace (it is also the baseline test fixture in [E17](foundations/E17-testing.md)):

- **`User`** — table `users`; `id` (PK), `full_name` (mapped from attribute `name`, `String(120)`, unique), `email`, `created_at`. Has many `Post`s and one `Profile` (one-to-one).
- **`Post`** — table `posts`; `id` (PK), `author_id` → `users.id`, `title`, `body`. `author` relationship `back_populates="posts"`; many `Comment`s; many `Tag`s via the `post_tags` association table.
- **`Comment`** — table `comments`; `id` (PK), `post_id` → `posts.id`, self-referential `parent_id` → `comments.id` (a threaded reply).
- **`Tag`** — table `tags`; many-to-many with `Post` via `post_tags`.
- **The migration chain** — an Alembic history under `migrations/versions/` whose `down_revision` links form one clean line ending in a single head.

When a spec needs something broken (a bad FK, a deprecated `backref`), it mutates this cast minimally — see the per-code fixtures in [E17](foundations/E17-testing.md).

## 6. Visualization Style Guide

- **ASCII mockups** (~78 columns) for every rendered surface — hover cards, inlay-hint lines, the schema view, completion/signature popovers, and `check` console output. They live in each spec's **UI Mockups** section, following the skill's `references/ascii-mockups.md`.
- **Mermaid** for flows and graphs — the two-pass pipeline ([E01](foundations/E01-architecture.md)), the extraction→index flow ([E30](foundations/E30-extraction-and-indexing.md)), a migration-chain DAG ([F13](features/F13-alembic-support.md)), an example ER diagram ([F12](features/F12-schema-visualization.md)). They live in the Visualizations section.
- **Tables** for catalogs and decision matrices — the diagnostic catalog, config keys, capability lists.

## 7. Cross-References

- **Related:** [index](index.md), [glossary](glossary.md), [01-overview](01-overview.md), [roadmap](roadmap.md).

## 8. Changelog

- **2026-06-17** — Initial constitution: P1–P6 product principles, engineering principles, authoring conventions, the `SQLA-` diagnostic-code scheme, the §4.6 non-functional scope, and the `clean-blog` example cast.
