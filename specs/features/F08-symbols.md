# F08 — Symbols

> **Status:** Draft
>
> **Version:** 0.1   ·   **Last updated:** 2026-06-17
>
> **Purpose:** Surface a file's models as a nested outline — each model with its columns and relationships beneath it — and answer query-based workspace-symbol searches across every model in the project.
>
> **Depends on:** [constitution](../constitution.md), [E07-data-model](../foundations/E07-data-model.md), [E30-extraction-and-indexing](../foundations/E30-extraction-and-indexing.md)   ·   **Related:** [E01-architecture](../foundations/E01-architecture.md), [E17-testing](../foundations/E17-testing.md), [E29-e2e-testing](../foundations/E29-e2e-testing.md), [F05-go-to-definition](F05-go-to-definition.md), [F06-find-references](F06-find-references.md), [F07-rename](F07-rename.md)

> Requirement tag: **SYM**

---

## 1. Purpose & Scope

Symbols give you the shape of your models at a glance. Open the outline of `models/post.py` and see `Post` with `id`, `author_id`, `title`, `body` and the relationships `author`, `comments`, `tags` nested under it. Hit your editor's "go to symbol in workspace" and type `Pos`, and `Post` and `Profile` come back from across the project.

This spec covers the two LSP symbol requests:

- **Document symbols** — a nested outline of one file: each `Model` is a class node with its `Column`s and `Relationship`s as children.
- **Workspace symbols** — a flat, query-filtered list of every model across the workspace, each pointing at its class location.

## 2. Non-Goals / Out of Scope

- **Generic Python symbols** — ordinary functions, plain classes, and module-level variables are the Python LSP's outline to provide (constitution P5; [ADR-007](../decisions/ADR-007-companion-to-python-lsp.md)). We surface only mapped models and their members.
- **Alembic outlines** — listing a migration file's operations as symbols is out of scope; the Alembic facts are owned by [F13-alembic-support](F13-alembic-support.md).
- **Hierarchical *call* or *type* hierarchies** — those are separate LSP features, not specced here.
- **How symbols are extracted** — the model, column, and relationship facts come from [E30](../foundations/E30-extraction-and-indexing.md) / [E07](../foundations/E07-data-model.md).

## 3. Background & Rationale

A SQLAlchemy model file reads as a flat list of class bodies, but it *is* a small hierarchy: a model owns columns and relationships. The Python LSP's outline shows the class and its attributes generically — a column and a relationship look the same, and a `Mapped[int]` annotation reads as just another assignment. We do better, because we already know which attributes are columns, which are relationships, and what each points at.

So document symbols render the hierarchy the model actually has: the model as a `Class` node, its columns and relationships as `Field` children, each annotated with the detail we extracted — a column's type, a relationship's target. Workspace symbols answer the project-wide "jump to model `X`" query by scanning the indexed models, case-insensitively, and returning each match's class location.

Both are read straight off the index — no re-parse ([E07 REQ-DATA-13](../foundations/E07-data-model.md)). The behavior is ported from the legacy `symbols.rs`, which already produced the nested document outline and the query-filtered workspace list.

## 4. Concepts & Definitions

- **Document symbol** — a node in a file's outline: a `DocumentSymbol` with a name, kind, detail, full range, selection range, and optional children.
- **Workspace symbol** — a flat entry in the project-wide symbol list: a name, kind, and a `Location`.
- **Selection range vs. full range** — the selection range is the identifier the editor highlights (the class name); the full range is the whole construct (the class body). [E07](../foundations/E07-data-model.md) records both.

## 5. Detailed Specification

Both handlers are pure functions over the workspace state. Document symbols take a URI and return that file's outline; workspace symbols take a query string and return matches across every file.

### 5.1 Document symbols — the nested outline

A file's outline is its models, each with its members nested beneath.

**REQ-SYM-01 — Each model is a `Class` symbol whose detail is its table name.**

For each model in the requested file, the server emits a `DocumentSymbol` with kind `CLASS`, the model's class name, and its `__tablename__` as the detail (so the outline shows `Post` · `posts`). The symbol's full range is the whole class; its selection range is the class-name identifier, so selecting the symbol highlights just the name.

**REQ-SYM-02 — A model's columns are nested `Field` children, detailed with their type.**

Each column becomes a child `DocumentSymbol` of kind `FIELD`, named by its Python attribute, with the column's rendered `MappedType` as the detail — `id` · `int`, `title` · `str`, `author_id` · `int`. The child's selection range is the attribute identifier and its full range is the whole statement.

**REQ-SYM-03 — A model's relationships are nested `Field` children, detailed with their target.**

Each relationship becomes a child `DocumentSymbol` of kind `FIELD`, named by its attribute, with `→ <target_model>` as the detail — `author` · `→ User`, `comments` · `→ Comment`. Like columns, the selection range is the attribute and the full range is the statement.

**REQ-SYM-04 — Children are ordered by their position in the source.**

Within a model, columns and relationships are sorted by their start line and column, so the outline reads top-to-bottom in the same order the user wrote the class body — not in hash-map order. A reader scanning the outline sees the members where they actually are.

**REQ-SYM-05 — A model with no columns or relationships has no children.**

When a model declares neither a column nor a relationship, its `DocumentSymbol` carries no children rather than an empty child list. The outline shows the bare class.

### 5.2 Workspace symbols — the project-wide query

Workspace symbols answer "find model `X`" across the whole project.

**REQ-SYM-06 — A workspace-symbol query returns every model whose name contains the query, case-insensitively.**

The server scans every indexed model and returns a flat `SymbolInformation` (kind `CLASS`) for each whose class name contains the query string, compared case-insensitively. The query `pos` matches `Post`; `USER` matches `User`. Each entry carries the model's class-name `Location` and its table name as the container, so the editor can group and jump.

**REQ-SYM-07 — An empty query returns every model.**

When the query is the empty string, every indexed model is returned. Some clients send an empty query to populate the picker before the user types; the server obliges with the full model list.

**REQ-SYM-08 — A query matching no model returns an empty list.**

When no model name contains the query, the server returns an empty list — not `null`, not a fuzzy near-match. We return exactly the models we can resolve by the substring rule (P4).

### 5.3 What is and isn't a symbol

The outline contains models and their members, nothing else.

**REQ-SYM-09 — Only mapped models and their columns/relationships are symbols.**

A file may hold plain classes, helper functions, Pydantic schemas, and module constants. None of these appears in our outline — only classes the index recognizes as models ([E30 REQ-EXTRACT-04](../foundations/E30-extraction-and-indexing.md)) and their extracted columns and relationships. A file with no models yields an empty outline, ceding the file's generic structure to the Python LSP (constitution P5).

## 6. UI Mockups

Document symbols render as the editor's outline tree (or breadcrumb). The sketch below shows how `models/post.py`'s outline looks once the server's nested `DocumentSymbol`s are drawn — the model as the top node, columns and relationships nested in source order, each with its detail.

### 6.1 Document outline — `models/post.py`

The tree a user sees in the editor's outline panel after requesting document symbols for the `clean-blog` `Post` model.

```
 OUTLINE — post.py
 ▾ Post                              posts        (class)
     id                              int          (field)
     author_id                       int          (field)
     title                           str          (field)
     body                            str          (field)
     author              → User                   (field)
     comments            → Comment                (field)
     tags                → Tag                     (field)
```

States: populated (above) · empty — a file with no models shows nothing from us, only the Python LSP's generic outline.

### 6.2 Workspace symbol picker — query `Pos`

The flat, query-filtered list the editor shows when you search workspace symbols and type `Pos`. Each row jumps to the model's class on selection.

```
 ❯ Pos
 ╭──────────────────────────────────────────────────────────╮
 │  Post            posts        models/post.py:9            │
 │  Profile         profiles     models/user.py:41           │
 ╰──────────────────────────────────────────────────────────╯
```

States: matches (above) · empty query — every model listed · no match — empty list, the picker shows nothing from us.

## 9. Examples & Use Cases

Open `models/post.py` from the `clean-blog` cast and ask for document symbols. The server reads the file's models from the index and emits one `Post` `CLASS` symbol detailed `posts` (REQ-SYM-01). Beneath it, in source order (REQ-SYM-04), come the columns `id` · `int`, `author_id` · `int`, `title` · `str`, `body` · `str` (REQ-SYM-02), then the relationships `author` · `→ User`, `comments` · `→ Comment`, `tags` · `→ Tag` (REQ-SYM-03). The editor draws the §6.1 tree, and selecting `author` highlights just the attribute name because that's its selection range.

Now press your editor's workspace-symbol shortcut and type `Pos`. The server scans every indexed model and matches `Post` and `Profile` (substring, case-insensitive), returning each with its class location and table name (REQ-SYM-06); the editor draws the §6.2 picker. Clearing the query to empty lists every model (REQ-SYM-07); typing `Ghost` returns nothing (REQ-SYM-08). After a teammate renames `Post` to `Article` and the index rebuilds ([E01](../foundations/E01-architecture.md)), the same `Pos` query no longer returns it — the workspace list reflects the current index, never a stale name.

## 10. Edge Cases & Failure Modes

- **File with no models** → empty document-symbol list; the Python LSP supplies the generic outline (REQ-SYM-09).
- **Model with no members** → a childless class symbol (REQ-SYM-05).
- **Model with no `__tablename__`** (inherits a base) → the class symbol has no detail string; it still appears.
- **Empty workspace query** → every model returned (REQ-SYM-07).
- **Query matching nothing** → empty list, not `null` (REQ-SYM-08).
- **Duplicate model names in two files** → both appear in workspace symbols, each with its own location; the picker lets the user choose.
- **Partial / `ERROR`-node file** → the models that extracted appear; a half-typed class simply isn't a symbol yet; no crash (P3).
- **Multi-byte identifiers** → symbol ranges land on correct UTF-8/UTF-16 positions ([E01](../foundations/E01-architecture.md) encoding negotiation).

## 11. Testing

Symbols are tested by requesting document symbols for a known file and asserting the nested tree (names, kinds, details, order, ranges), and by running workspace-symbol queries and asserting the matched set.

### 11.1 Scope & coverage

Target: **100% of this feature's behavior is covered.** Every `REQ-SYM-NN` maps to at least one test; every mockup state (§6) and edge case (§10) has a test. See the policy in [E17-testing](../foundations/E17-testing.md#2-coverage-policy).

### 11.2 Test plan

Each row is a behavior under test. Shared fixtures live in [E17-testing](../foundations/E17-testing.md#5-fixtures-registry).

| Behavior / scenario | Type | Fixtures | Verifies |
|---|---|---|---|
| `Post` emitted as a `CLASS` symbol detailed `posts` | unit | [clean-blog](../foundations/E17-testing.md#clean-blog) | REQ-SYM-01 |
| Columns nested as `FIELD` children with type detail | unit | [clean-blog](../foundations/E17-testing.md#clean-blog) | REQ-SYM-02 |
| Relationships nested as `FIELD` children with `→ target` detail | unit | [clean-blog](../foundations/E17-testing.md#clean-blog) | REQ-SYM-03 |
| Children ordered by source position | unit | [clean-blog](../foundations/E17-testing.md#clean-blog) | REQ-SYM-04 |
| Member-less model → childless symbol | unit | [clean-blog](../foundations/E17-testing.md#clean-blog) | REQ-SYM-05 |
| Workspace query `Pos` (case-insensitive) → `Post`, `Profile` | integration | [clean-blog](../foundations/E17-testing.md#clean-blog) | REQ-SYM-06 |
| Empty workspace query → every model | integration | [clean-blog](../foundations/E17-testing.md#clean-blog) | REQ-SYM-07 |
| Workspace query matching nothing → empty list | integration | [clean-blog](../foundations/E17-testing.md#clean-blog) | REQ-SYM-08 |
| File with no models → empty document-symbol list | unit | [clean-blog](../foundations/E17-testing.md#clean-blog) | REQ-SYM-09 |
| Multi-byte identifiers: symbol ranges land on correct UTF positions | integration | [non-ascii](../foundations/E17-testing.md#non-ascii) | REQ-SYM-01, REQ-SYM-02 |

### 11.3 Fixtures

All fixtures are the shared ones in [E17-testing](../foundations/E17-testing.md#5-fixtures-registry); this feature defines none of its own.

- **clean-blog** — the multi-model workspace whose models, columns, and relationships populate both outlines.
- **non-ascii** — pins UTF-8/UTF-16 range correctness for symbols on multi-byte identifiers.

### 11.4 Requirement coverage

| Requirement | Covered by |
|---|---|
| REQ-SYM-01 | "`Post` emitted as a `CLASS` symbol", "non-ascii ranges" |
| REQ-SYM-02 | "Columns nested as `FIELD` children", "non-ascii ranges" |
| REQ-SYM-03 | "Relationships nested as `FIELD` children" |
| REQ-SYM-04 | "Children ordered by source position" |
| REQ-SYM-05 | "Member-less model → childless symbol" |
| REQ-SYM-06 | "Workspace query `Pos` → `Post`, `Profile`" |
| REQ-SYM-07 | "Empty workspace query → every model" |
| REQ-SYM-08 | "Workspace query matching nothing → empty list" |
| REQ-SYM-09 | "File with no models → empty list" |

## 12. End-to-End Test Plan

Driven by `pytest-lsp` over stdio against the built binary, each scenario opens a fixture, issues `textDocument/documentSymbol` or `workspace/symbol`, and asserts the returned structure. Follow the harness and patterns in [E29-e2e-testing](../foundations/E29-e2e-testing.md).

### 12.1 Coverage target

**100% of the feature's scope, end to end** — the nested document outline, the query-filtered workspace list, and every empty path. See the policy in [E29-e2e-testing](../foundations/E29-e2e-testing.md#2-coverage-policy).

### 12.2 Scenarios

| # | Journey | Path | Expected outcome |
|---|---|---|---|
| E2E-01 | Document symbols for `post.py` | happy | `Post` class with columns + relationships nested, in source order |
| E2E-02 | Document symbols for a file with no models | error | empty list |
| E2E-03 | Workspace symbols query `Pos` | happy | `Post` and `Profile` with their class locations |
| E2E-04 | Workspace symbols with an empty query | happy | every model returned |
| E2E-05 | Workspace symbols query `Ghost` | error | empty list |
| E2E-06 | Cross-file freshness: rename `Post` → `Article`, re-query `Pos` | happy | `Post` no longer matches; `Article` matches `Art` |
| E2E-07 | Document symbols on a multi-byte identifier file (non-ascii) | happy | ranges correct under both UTF-8 and UTF-16 |

### 12.3 Acceptance criteria & Definition of Done

The §12.2 scenarios, written Given/When/Then, are this feature's acceptance criteria:

| # | Given | When | Then |
|---|---|---|---|
| AC-01 | the clean-blog `post.py` is open | I request document symbols | I get `Post` with its columns and relationships nested in source order |
| AC-02 | a file with no models is open | I request document symbols | I get an empty list |
| AC-03 | the clean-blog workspace is indexed | I search workspace symbols for `Pos` | I get `Post` and `Profile` with their locations |
| AC-04 | the clean-blog workspace is indexed | I search workspace symbols for `Ghost` | I get an empty list |
| AC-05 | `Post` was just renamed to `Article` | I search workspace symbols for `Pos` | `Post` no longer appears |

**Definition of Done:** every `REQ-SYM-NN` has a passing test (§11.4), every acceptance scenario above passes, and the §13.1 security posture is verified.

## 13. Non-Functional Requirements

### 13.1 Security & Privacy

- **Access & authorization** — none; a single-user developer tool. Symbols read only the in-memory index built from local workspace files.
- **Input & validation** — a URI (document symbols) or a query string (workspace symbols) is the only input; an unknown URI yields an empty list and any query is treated as a literal substring, never executed (P1, P3).
- **Data sensitivity** — none. The feature opens no network connection, sends no telemetry, and handles no secrets. Logs go to stderr or the configured `log_file`, never stdout.
- **Baseline** — stays within the suite-wide envelope stated once in the [constitution](../constitution.md); it reads cached facts and returns an outline or a match list.

## 16. Cross-References

- **Depends on:** [constitution](../constitution.md) — P4 (exact substring matches only) and P5 (only model symbols, the rest is the Python LSP's); [E07-data-model](../foundations/E07-data-model.md) — the model/column/relationship facts and the full-range vs. selection-range distinction each symbol carries; [E30-extraction-and-indexing](../foundations/E30-extraction-and-indexing.md) — the model-recognition rule that decides what becomes a symbol.
- **Related:** [E01-architecture](../foundations/E01-architecture.md) — the no-stale-data guarantee that keeps the workspace list current and the encoding negotiation behind correct ranges; [F05-go-to-definition](F05-go-to-definition.md) — a workspace symbol's location is the same class anchor go-to-definition jumps to; [F07-rename](F07-rename.md) — a rename refreshes both outlines on re-index; [E17-testing](../foundations/E17-testing.md) / [E29-e2e-testing](../foundations/E29-e2e-testing.md) — the fixtures and harness behind the test plans.

## 17. Changelog

- **2026-06-17** — Initial draft. Ported the legacy `symbols.rs` behavior (nested `Model → Column/Relationship` document outline, source-ordered children, case-insensitive query-filtered workspace symbols) into nine requirements, added the ASCII outline and workspace-picker mockups, and added the testing and E2E plans against the `clean-blog` cast.
