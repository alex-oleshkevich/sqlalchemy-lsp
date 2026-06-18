# F08 — Migration Symbols

> **Status:** Approved
>
> **Version:** 0.2   ·   **Last updated:** 2026-06-18
>
> **Purpose:** Make Alembic migrations findable by their *database* identity — jump to a revision by its revision id or its human message — through `workspace/symbol`. This is the one symbol surface the Python LSP cannot provide, because a revision id and a migration message are not Python symbols.

> **Depends on:** [constitution](../constitution.md), [E07-data-model](../foundations/E07-data-model.md), [E30-extraction-and-indexing](../foundations/E30-extraction-and-indexing.md)   ·   **Related:** [E01-architecture](../foundations/E01-architecture.md), [F13-alembic-support](F13-alembic-support.md), [E17-testing](../foundations/E17-testing.md), [E29-e2e-testing](../foundations/E29-e2e-testing.md)

> Requirement tag: **SYM**

---

## 1. Purpose & Scope

You know a migration by its revision id (`a1b2c3d4`) or by what it did ("add user table") — never by a Python symbol name, because a migration file's meaningful identity lives in its `revision` string and its message, not in any class or function. So this is the only symbol provider we ship: a `workspace/symbol` contributor that indexes Alembic revisions and lets you jump to one by **id** or by **message**.

This spec covers exactly one thing:

- **Workspace symbols for Alembic revisions** — each indexed migration is a workspace symbol whose name is its revision id and message, located at the migration's `revision` assignment.

## 2. Non-Goals / Out of Scope

This feature was deliberately narrowed to avoid duplicating the Python language server (constitution P5, [ADR-007](../decisions/ADR-007-companion-to-python-lsp.md)).

- **Document symbols / file outlines** — listing a file's classes and attributes is the Python LSP's job. A model class and its `Mapped[...]` attributes already appear in Pyright/`pylsp`'s outline; re-emitting them would only double the outline. We provide **no** `textDocument/documentSymbol`.
- **Model / table / column workspace search** — a model's name *is* a Python class name, which the Python LSP already indexes for `workspace/symbol`. We don't duplicate it. (Searching by `__tablename__` or DB column name was considered and dropped — see the Changelog and [§3](#3-background--rationale).)
- **Alembic op outlines and migration diagnostics** — the chain checks, op completions, and jump-from-op-to-model are owned by [F13-alembic-support](F13-alembic-support.md). F08 only makes revisions *findable*; F13 reasons about them.
- **How revisions are extracted** — the `MigrationFile` facts (revision id, message, location) come from [E30](../foundations/E30-extraction-and-indexing.md) / [E07](../foundations/E07-data-model.md).

## 3. Background & Rationale

Originally this spec mirrored the legacy server's nested model outline and a model-name workspace search. Both turned out to duplicate the Python LSP: the outline repeats what Pyright shows for any class, and model names are Python class names Pyright already indexes. Per the companion principle, duplicating a host-LSP capability is a cost (a doubled outline, a redundant picker) with no benefit, so both were dropped.

What survived is the one search the Python LSP *structurally cannot* do. A migration's revision id and message are strings — in a `revision = "a1b2c3d4"` assignment and the file's docstring/slug — not symbols a Python indexer surfaces. When you're staring at a `down_revision` and want to open its parent, or you remember a migration "added the audit table" but not its id, you want to type that and jump. That is genuinely ours to provide.

It reads straight off the migration index — no re-parse ([E07 REQ-DATA-13](../foundations/E07-data-model.md)).

## 4. Concepts & Definitions

- **Workspace symbol** — a flat entry in the project-wide symbol list (`SymbolInformation`/`WorkspaceSymbol`): a name, a kind, and a `Location`. Editors merge contributions from every active server, so ours sits alongside the Python LSP's.
- **Revision id** — the unique string a migration assigns to `revision` (e.g. `a1b2c3d4`). (Canonical definition in [glossary](../glossary.md).)
- **Revision message** — the human label of a migration: the slug in its filename (`a1b2c3d4_add_user_table.py` → "add user table") and/or the first line of its module docstring. Carried on the `MigrationFile` fact ([E07](../foundations/E07-data-model.md)).

## 5. Detailed Specification

The handler is a pure function of the workspace state and a query string; it scans the migration index and returns matches.

### 5.1 Revisions as workspace symbols

**REQ-SYM-01 — Each indexed migration is a workspace symbol named by its revision id and message.**

For every migration in `revision_index` ([E07](../foundations/E07-data-model.md)), the server can emit one workspace symbol whose label combines the revision id and the message — `a1b2c3d4 · add user table` — with kind `EVENT` (a revision is a point in history, not a class), and a `Location` pointing at the `revision = "…"` assignment so selecting it opens the migration there. The container is the parent revision (`down_revision`) when present, so the picker hints at chain position.

**REQ-SYM-02 — A query matches a revision by id or by message, case-insensitively, as a substring.**

The server returns every migration whose **revision id** *or* **message** contains the query, compared case-insensitively. `a1b2` matches the revision `a1b2c3d4`; `audit` matches the migration messaged "add audit table"; `user` matches `add_user_table`. Both fields are searched so the user can jump by whichever they remember.

**REQ-SYM-03 — An empty query returns every revision; a query matching nothing returns an empty list.**

An empty query lists every indexed migration (some clients pre-populate the picker this way). A query that matches neither an id nor a message returns an empty list — not `null`, not a fuzzy near-match (P4).

### 5.2 What is and isn't a symbol

**REQ-SYM-04 — Only Alembic revisions are F08 symbols; everything else is the Python LSP's.**

F08 contributes revision symbols and nothing else. Model classes, columns, relationships, functions, and module variables are not F08 symbols — the Python LSP indexes those for `workspace/symbol`, and provides the document outline, uncontested. A workspace with no migrations yields no F08 contributions at all.

## 6. UI Mockups

Workspace symbols render in the editor's "go to symbol in workspace" picker, merged with the Python LSP's own results. The sketch shows the rows F08 contributes for a query against the `clean-blog` Alembic history.

### 6.1 Workspace symbol picker — query `user`

Typing `user` in the workspace-symbol picker; F08 contributes the migrations whose id or message matches. Each row jumps to the migration's `revision` assignment.

```
 ❯ user
 ╭────────────────────────────────────────────────────────────────╮
 │  a1b2c3d4 · add user table     ↜ <base>   migrations/versions/… │
 │  f9e8d7c6 · add user audit     ↜ a1b2c3d4 migrations/versions/… │
 ╰────────────────────────────────────────────────────────────────╯
```

States: matches by message (above) · matches by id (query `a1b2`) · empty query — every revision listed · no match — F08 contributes nothing, the picker shows only the Python LSP's results.

## 9. Examples & Use Cases

You're reading a migration whose `down_revision = "a1b2c3d4"` and you want its parent. You open the workspace-symbol picker and type `a1b2`; F08 scans `revision_index`, matches the revision id, and returns `a1b2c3d4 · add user table` at its `revision` line (REQ-SYM-01, REQ-SYM-02). Selecting it opens that migration. Later you recall a migration "added the audit table" but not its id — you type `audit`, and F08 matches on the message and jumps you there. Clearing the query lists the whole history (REQ-SYM-03); typing `ghost` returns nothing from F08, and the picker shows only your Python LSP's class/function matches (REQ-SYM-04). After a new migration lands and the index rebuilds ([E01](../foundations/E01-architecture.md)), it appears in the next query — the list always reflects the current index.

## 10. Edge Cases & Failure Modes

- **Workspace with no migrations** → F08 contributes nothing; the picker shows only the Python LSP's symbols (REQ-SYM-04).
- **A migration with a revision id but no message** (no slug, no docstring) → it's still a symbol, labeled by id alone.
- **Two migrations sharing a message** ("add index") → both appear, each at its own location; the container (`down_revision`) and id disambiguate.
- **Empty query** → every revision returned (REQ-SYM-03).
- **Query matching nothing** → empty list, not `null` (REQ-SYM-03).
- **Partial / `ERROR`-node migration** → if the `revision` assignment extracted, the symbol appears; otherwise it simply isn't indexed yet; no crash (P3).
- **Multi-byte characters in a message** → ranges land on correct UTF-8/UTF-16 positions ([E01](../foundations/E01-architecture.md) encoding negotiation).

## 11. Testing

Symbols are tested by running `workspace/symbol` queries against a fixture with a known Alembic history and asserting the matched revisions.

### 11.1 Scope & coverage

Target: **100% of this feature's behavior is covered.** Every `REQ-SYM-NN` maps to at least one test; every mockup state (§6) and edge case (§10) has a test. See the policy in [E17-testing](../foundations/E17-testing.md#2-coverage-policy).

### 11.2 Test plan

| Behavior / scenario | Type | Fixtures | Verifies |
|---|---|---|---|
| Each migration emitted as a revision symbol (`id · message`, location at `revision=`) | integration | [clean-blog](../foundations/E17-testing.md#clean-blog) | REQ-SYM-01 |
| Query matches by revision id (`a1b2`) | integration | [clean-blog](../foundations/E17-testing.md#clean-blog) | REQ-SYM-02 |
| Query matches by message (`audit`), case-insensitive | integration | [clean-blog](../foundations/E17-testing.md#clean-blog) | REQ-SYM-02 |
| Empty query → every revision | integration | [clean-blog](../foundations/E17-testing.md#clean-blog) | REQ-SYM-03 |
| Query matching nothing → empty list (not null) | integration | [clean-blog](../foundations/E17-testing.md#clean-blog) | REQ-SYM-03 |
| No migrations in workspace → no F08 contributions | unit | [clean-blog](../foundations/E17-testing.md#clean-blog) | REQ-SYM-04 |
| Multi-byte message: ranges correct under UTF-8/UTF-16 | integration | [non-ascii](../foundations/E17-testing.md#non-ascii) | REQ-SYM-01 |

### 11.3 Requirement coverage

| Requirement | Covered by |
|---|---|
| REQ-SYM-01 | revision-symbol emission test, non-ascii ranges |
| REQ-SYM-02 | match-by-id and match-by-message tests |
| REQ-SYM-03 | empty-query and no-match tests |
| REQ-SYM-04 | no-migrations / only-revisions test |

## 12. End-to-End Test Plan

Driven by `pytest-lsp` over stdio, each scenario opens a fixture and issues `workspace/symbol`. Follow the harness in [E29-e2e-testing](../foundations/E29-e2e-testing.md).

### 12.1 Coverage target

**100% of the feature's scope, end to end** — match by id, match by message, empty query, and the empty paths. The shared protocol-conformance journeys ([E29 REQ-E2E-03](../foundations/E29-e2e-testing.md#5-patterns)) are inherited, not re-tested.

### 12.2 Scenarios

| # | Journey | Path | Expected outcome |
|---|---|---|---|
| E2E-01 | `workspace/symbol` query `a1b2` | happy | the matching revision, located at its `revision=` line |
| E2E-02 | `workspace/symbol` query `audit` (by message) | happy | the migration messaged "add audit table" |
| E2E-03 | empty query | happy | every indexed revision |
| E2E-04 | query `ghost` | error | F08 contributes an empty list |
| E2E-05 | non-ascii message file | happy | ranges correct under both encodings |

### 12.3 Acceptance criteria & Definition of Done

| # | Given | When | Then |
|---|---|---|---|
| AC-01 | the clean-blog Alembic history is indexed | I search workspace symbols for `a1b2` | I get the matching revision at its `revision=` line |
| AC-02 | the history is indexed | I search for `audit` | I get the migration whose message contains "audit" |
| AC-03 | a workspace with no migrations | I search workspace symbols | F08 returns nothing; only the Python LSP answers |

**Definition of Done:** every `REQ-SYM-NN` has a passing test (§11.3), every acceptance scenario passes, and the §13.1 security posture holds.

## 13. Non-Functional Requirements

### 13.1 Security & Privacy

- **Access & authorization** — none; a single-user developer tool. Reads only the in-memory migration index built from local files.
- **Input & validation** — a query string is the only input, treated as a literal substring, never executed (P1).
- **Data sensitivity** — none. No network, no telemetry, no secrets. Logs go to stderr or `log_file`, never stdout.
- **Baseline** — stays within the suite-wide envelope stated once in the [constitution](../constitution.md).

## 16. Cross-References

- **Depends on:** [constitution](../constitution.md) — P4 (exact substring matches only) and P5 (only what the Python LSP can't do); [E07-data-model](../foundations/E07-data-model.md) — the `MigrationFile` fact (revision id, message, location) and `revision_index`; [E30-extraction-and-indexing](../foundations/E30-extraction-and-indexing.md) — how the revision id and message are extracted.
- **Related:** [F13-alembic-support](F13-alembic-support.md) — the Alembic diagnostics, op completions, and jump-to-model that reason about the same revisions F08 makes findable; [E01-architecture](../foundations/E01-architecture.md) — the no-stale-data guarantee that keeps the list current and the encoding negotiation behind correct ranges; [E17-testing](../foundations/E17-testing.md) / [E29-e2e-testing](../foundations/E29-e2e-testing.md) — fixtures and harness.

## 17. Changelog

- **2026-06-18** — Approved.
- **2026-06-18** — v0.2: **narrowed F08 to Alembic-revision workspace symbols** (search by revision id or message). Dropped the document-symbol outline (the Python LSP owns file outlines) and the model-name workspace search (model names are Python class names Pyright already indexes) as duplication of the host LSP, per the companion principle ([ADR-007](../decisions/ADR-007-companion-to-python-lsp.md)). Rewrote requirements, mockup, and tests around the revision-symbol surface.
- **2026-06-17** — Initial draft. Ported the legacy `symbols.rs` nested model outline and model-name workspace search (superseded by v0.2).
