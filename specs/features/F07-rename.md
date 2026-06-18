# F07 — Rename

> **Status:** Approved
>
> **Version:** 0.1   ·   **Last updated:** 2026-06-18
>
> **Purpose:** Workspace-wide rename of a model, column, or relationship — rewriting not just the declaration but every foreign-key string, relationship target, and `back_populates` value that names it, in one atomic edit.
>
> **Depends on:** [constitution](../constitution.md), [E07-data-model](../foundations/E07-data-model.md), [E30-extraction-and-indexing](../foundations/E30-extraction-and-indexing.md)   ·   **Related:** [E01-architecture](../foundations/E01-architecture.md), [E17-testing](../foundations/E17-testing.md), [E29-e2e-testing](../foundations/E29-e2e-testing.md), [F05-go-to-definition](F05-go-to-definition.md), [F06-find-references](F06-find-references.md)

> Requirement tag: **RN**

---

## 1. Purpose & Scope

Rename a `User` model and every `ForeignKey("users.id")`, `relationship("User")`, and `back_populates` that names it should move with it. That cross-file rewrite is what a plain Python LSP can't do — the references live in strings and forward references it treats as opaque. This feature reads them as the structured references they are and rewrites them all in one `WorkspaceEdit`.

This spec covers:

- **`prepareRename`** — validating the cursor sits on a renameable symbol and returning its range and placeholder.
- **Renaming a model** — the class declaration plus every FK string and relationship target across files.
- **Renaming a column** — the attribute plus every foreign key whose target is that column.
- **Renaming a relationship** — the attribute plus every `back_populates` value naming it.
- **Returning `null`/error** when the cursor isn't on a renameable symbol.

## 2. Non-Goals / Out of Scope

- **Generic Python rename** — renaming an ordinary variable, function, or import is the Python LSP's job (constitution P5; [ADR-007](../decisions/ADR-007-companion-to-python-lsp.md)). We rewrite only SQLAlchemy references.
- **Renaming a database table or column *name*** (the `__tablename__` string or a `mapped_column(name=…)` alias) — this feature renames the *Python* identifier; the DB-name strings are a separate concern not covered here.
- **Renaming Alembic revisions** — migration identifiers are owned by [F13-alembic-support](F13-alembic-support.md).
- **Finding references without rewriting them** — owned by [F06-find-references](F06-find-references.md), which shares the same reference graph.

## 3. Background & Rationale

Rename is find-references with edits. The reference graph is identical — the foreign keys, relationship targets, and `back_populates` values that name a symbol ([F06](F06-find-references.md)) — but instead of returning their locations, rename returns a `TextEdit` for each, plus one for the declaration. The whole set ships as a single `WorkspaceEdit` so the editor applies it atomically: either every occurrence moves or none does.

This is where the constitution's **no-stale-data** rule earns its keep ([E01](../foundations/E01-architecture.md)). The edits the server computes are derived from the *current* index. When the client applies them, every file changes on disk, the watcher fires, Pass 1 re-extracts each, the debounced Pass 2 rebuilds the index, and every cross-file reference re-resolves against the new names. So after a rename there is no dangling `ForeignKey("users.id")` pointing at a `User` that's now `Account`, and no hover or diagnostic reading a name that no longer exists. The rename produces the edits; the pipeline guarantees the index that follows is consistent.

The legacy `rename.rs` proved the three rename paths. We carry them forward, fix the one subtlety the legacy code flagged in its own comments (rewriting only the table portion of an FK string, and leaving base-class references it couldn't range correctly), and make the contract explicit: only references we can positively tie to the symbol are rewritten (P4).

## 4. Concepts & Definitions

- **`prepareRename`** — the LSP request that asks, before any edit, whether a position is renameable and what its current text is.
- **`WorkspaceEdit`** — the LSP payload mapping each file URI to a list of `TextEdit`s; applied atomically by the client.
- **Reference graph** — the FK / relationship-target / `back_populates` uses of a symbol, the same set [F06](F06-find-references.md) collects.

## 5. Detailed Specification

Rename is two requests. `prepareRename` validates the position and returns the symbol's range and placeholder; `rename` then computes the `WorkspaceEdit`. Both are pure functions over the workspace state.

### 5.1 prepareRename

Before the editor shows a rename box, it asks whether the position is even renameable.

**REQ-RN-01 — `prepareRename` returns the symbol's range and placeholder when the cursor is on a renameable symbol.**

The server checks the cursor against the ranges in the file's models, in order — model class name, then each column attribute, then each relationship attribute. On a hit it returns a `RangeWithPlaceholder`: the symbol's name range (so the editor highlights exactly the identifier) and the current name as the placeholder (so the rename box pre-fills it). On `class User`, it returns the range of `User` and the placeholder `"User"`.

**REQ-RN-02 — `prepareRename` returns `null` when the cursor is not on a renameable symbol.**

If the cursor sits on plain Python, a string literal, or anything that isn't a model/column/relationship declaration, the server returns `null` and the editor refuses to start a rename — ceding the position to the Python LSP (constitution P5). We never offer to rename a symbol we don't own.

### 5.2 Renaming a model

Renaming a model rewrites its class and every reference to it.

**REQ-RN-03 — Renaming a model rewrites the class declaration and every cross-file reference.**

When the cursor is on a model's class name, the server produces a `WorkspaceEdit` containing:

1. A `TextEdit` over the class-name range, replacing it with the new name.
2. For every foreign key in the workspace whose `table` is this model — a `TextEdit` rewriting the FK to name the new model, preserving the column half (`"users.id"` → `"accounts.id"` keeps `id`).
3. For every relationship whose `target_model` is this model — a `TextEdit` over its `target_range` replacing the written target with the new name.

Take renaming `User` → `Account` in the `clean-blog` cast. The edit touches `class User` in `user.py`, the `ForeignKey("users.id")` on `Post.author_id`, and the `relationship("User", …)` target on `Post.author` — all in one atomic `WorkspaceEdit`.

**REQ-RN-04 — A foreign-key string rewrite replaces only the table portion, never the column.**

A string FK is `"users.id"`. Renaming `User` rewrites the table half and keeps the column half, producing `"accounts.id"`. The server computes the replacement as `"<new-table>.<original-column>"` so the column reference survives the rename untouched. (The legacy code noted this as a sharp edge; the rule makes it explicit.)

### 5.3 Renaming a column

Renaming a column rewrites the attribute and every foreign key that targets it.

**REQ-RN-05 — Renaming a column rewrites the attribute and every foreign key whose target is that column.**

When the cursor is on a column attribute, the server produces a `WorkspaceEdit` with a `TextEdit` over the column's attribute range and a `TextEdit` for every foreign key in the workspace whose target matches *both* halves — the FK's `column` equals the old name *and* its `table` equals this column's model's table name or class name. Each FK rewrite preserves the table half and replaces the column half (`"users.id"` → `"users.uuid"`). Renaming `User.id` → `User.uuid` therefore rewrites the attribute and the `ForeignKey("users.id")` on `Post.author_id`.

Matching on both halves is what keeps the rename precise — a FK to `posts.id` is never touched when you rename `User.id`, even though both columns are named `id` (the same two-halves rule [F06 REQ-REF-05](F06-find-references.md) uses).

### 5.4 Renaming a relationship

Renaming a relationship rewrites the attribute and every `back_populates` value naming it.

**REQ-RN-06 — Renaming a relationship rewrites the attribute and every `back_populates` value that names it.**

When the cursor is on a relationship attribute, the server produces a `WorkspaceEdit` with a `TextEdit` over the relationship's attribute range and a `TextEdit` over the `back_populates_range` of every relationship in the workspace that (a) targets this relationship's owning model and (b) carries a `back_populates` equal to this relationship's name. Renaming `User.posts` → `articles` rewrites the attribute and the `back_populates="posts"` on `Post.author` to `back_populates="articles"`, keeping the bidirectional pair consistent.

### 5.5 Atomicity and freshness

The whole rename is one edit, and the index that follows is consistent.

**REQ-RN-07 — All edits ship in one `WorkspaceEdit`, grouped by file.**

The server returns a single `WorkspaceEdit` whose `changes` map keys each affected file URI to its list of `TextEdit`s. The client applies them atomically — all occurrences move together or the rename is rejected. The server never applies edits itself; it only computes them.

**REQ-RN-08 — After the client applies the edit, the index re-resolves with no stale data.**

Applying the `WorkspaceEdit` writes every touched file. The pipeline ([E01](../foundations/E01-architecture.md)) re-extracts each on the resulting change, rebuilds the index, and re-resolves every cross-file reference against the new names. So immediately after a rename, go-to-definition, find-references, hover, and diagnostics all read the renamed symbol — no occurrence is left pointing at the old name, in any file, opened or not. This is the constitution's no-stale-data rule applied end to end.

### 5.6 Only positively-resolved references are rewritten

**REQ-RN-09 — References the server can't positively tie to the symbol are left untouched.**

Rename rewrites a reference only when it provably names the symbol under the cursor — a FK whose table and column both match, a relationship whose target resolves to this model, a `back_populates` that names this attribute on the right model. A string that merely resembles the name, an unresolved forward reference to a different model, a base-class use the server can't range precisely — none is rewritten. We would rather leave a reference for the user to fix by hand than corrupt code by guessing (P4). A reference to a symbol the index can't resolve is simply not part of the edit.

## 7. Visualizations

The table below maps each rename target to the edits it produces. Every edit is one `TextEdit`; the whole set is one atomic `WorkspaceEdit`.

| Rename target | Declaration edit | Reference edits |
|---|---|---|
| Model `User` → `Account` | class-name range | each FK `table` (table half only) + each relationship `target_range` |
| Column `User.id` → `uuid` | attribute range | each FK matching `table`+`column` (column half only) |
| Relationship `User.posts` → `articles` | attribute range | each `back_populates="posts"` on the pair's other side |
| cursor not on a symbol | — | `prepareRename` → `null`; rename → no edit |

## 9. Examples & Use Cases

Walk the `clean-blog` cast. You put your cursor on `class User` and trigger rename. First the editor calls `prepareRename`; the server returns the range of `User` and the placeholder `"User"`, so the rename box opens pre-filled (REQ-RN-01). You type `Account`. The server walks the workspace and builds one `WorkspaceEdit`: rewrite `class User` → `class Account` in `user.py`; rewrite `ForeignKey("users.id")` → `ForeignKey("accounts.id")` on `Post.author_id`, preserving the `id` (REQ-RN-04); and rewrite `relationship("User", …)` → `relationship("Account", …)` on `Post.author` (REQ-RN-03). The editor applies all three atomically (REQ-RN-07).

The moment the files change, the watcher fires, Pass 1 re-extracts `user.py` and `post.py`, Pass 2 rebuilds the index, and `Post.author`'s FK and target re-resolve to `Account` (REQ-RN-08). A jump from `author` now lands on `class Account`; a hover on `author_id` shows `→ accounts.id`. Nothing dangles. Had `Post.author` instead carried a `back_populates="posts"` while you renamed the *relationship* `User.posts` to `articles`, that string would have moved too, keeping the pair wired (REQ-RN-06).

## 10. Edge Cases & Failure Modes

- **Cursor not on a renameable symbol** → `prepareRename` returns `null`; the editor won't start a rename (REQ-RN-02).
- **FK string rename** → only the table half changes; the column half is preserved (REQ-RN-04).
- **Two columns named `id`** → renaming `User.id` touches only FKs matching both table and column, never a FK to `posts.id` (REQ-RN-05).
- **Self-referential relationship** (`Comment.parent` ↔ `Comment.children`) → the attribute and its in-file `back_populates` counterpart both move (REQ-RN-06).
- **A reference the server can't resolve** → left untouched, not corrupted (REQ-RN-09).
- **New name collides with an existing model/column** → the server still emits the edits; detecting the resulting duplicate (`SQLA-E102`/`SQLA-E103`) is the diagnostics' job after re-index, not rename's to pre-empt.
- **Partial / `ERROR`-node file** → only the references that extracted are rewritten; no crash (P3).
- **Empty or whitespace new name** → the server emits the edit as requested; the editor's own rename UI is expected to reject empties before sending.

## 11. Testing

Rename is tested by triggering `prepareRename` and `rename` on each symbol kind in a known fixture and asserting the exact `WorkspaceEdit` — every `TextEdit`'s URI, range, and replacement text — plus the `null` paths.

### 11.1 Scope & coverage

Target: **100% of this feature's behavior is covered.** Every `REQ-RN-NN` maps to at least one test; every edge case (§10) has a test. See the policy in [E17-testing](../foundations/E17-testing.md#2-coverage-policy).

### 11.2 Test plan

Each row is a behavior under test. Shared fixtures live in [E17-testing](../foundations/E17-testing.md#5-fixtures-registry).

| Behavior / scenario | Type | Fixtures | Verifies |
|---|---|---|---|
| `prepareRename` on `class User` → range + placeholder `"User"` | unit | [clean-blog](../foundations/E17-testing.md#clean-blog) | REQ-RN-01 |
| `prepareRename` on plain Python → `null` | unit | [clean-blog](../foundations/E17-testing.md#clean-blog) | REQ-RN-02 |
| Rename `User` rewrites class + FK string + relationship target | integration | [clean-blog](../foundations/E17-testing.md#clean-blog) | REQ-RN-03 |
| FK string rewrite preserves the column half | integration | [clean-blog](../foundations/E17-testing.md#clean-blog) | REQ-RN-04 |
| Rename `User.id` rewrites attribute + matching FK column half | integration | [clean-blog](../foundations/E17-testing.md#clean-blog) | REQ-RN-05 |
| Rename `User.id` leaves a FK to `posts.id` untouched | integration | [clean-blog](../foundations/E17-testing.md#clean-blog) | REQ-RN-05 |
| Rename `User.posts` rewrites attribute + `back_populates="posts"` | integration | [clean-blog](../foundations/E17-testing.md#clean-blog) | REQ-RN-06 |
| Self-referential relationship rename moves the in-file counterpart | integration | [clean-blog](../foundations/E17-testing.md#clean-blog) | REQ-RN-06 |
| All edits arrive in one `WorkspaceEdit`, grouped by file | integration | [clean-blog](../foundations/E17-testing.md#clean-blog) | REQ-RN-07 |
| After applying, re-index resolves the new name (no stale ref) | integration | [clean-blog](../foundations/E17-testing.md#clean-blog) | REQ-RN-08 |
| An unresolvable reference is not rewritten | integration | [rel-target-not-found](../foundations/E17-testing.md#rel-target-not-found) | REQ-RN-09 |
| Rename into a multi-byte identifier: edit ranges land correctly | integration | [non-ascii](../foundations/E17-testing.md#non-ascii) | REQ-RN-03, REQ-RN-05 |

### 11.3 Fixtures

All fixtures are the shared ones in [E17-testing](../foundations/E17-testing.md#5-fixtures-registry); this feature defines none of its own.

- **clean-blog** — the cross-file FK, relationship-target, and `back_populates` references a rename must rewrite.
- **rel-target-not-found** — an unresolvable target that rename must leave untouched (REQ-RN-09).
- **non-ascii** — pins UTF-8/UTF-16 edit-range correctness for renames touching multi-byte identifiers.

### 11.4 Requirement coverage

| Requirement | Covered by |
|---|---|
| REQ-RN-01 | "`prepareRename` on `class User`" |
| REQ-RN-02 | "`prepareRename` on plain Python → `null`" |
| REQ-RN-03 | "Rename `User` rewrites class + FK + target", "non-ascii ranges" |
| REQ-RN-04 | "FK string rewrite preserves the column half" |
| REQ-RN-05 | "Rename `User.id` rewrites matching FK", "leaves FK to `posts.id` untouched", "non-ascii ranges" |
| REQ-RN-06 | "Rename `User.posts` rewrites `back_populates`", "self-referential rename" |
| REQ-RN-07 | "All edits in one `WorkspaceEdit`" |
| REQ-RN-08 | "After applying, re-index resolves the new name" |
| REQ-RN-09 | "An unresolvable reference is not rewritten" |

## 12. End-to-End Test Plan

Driven by `pytest-lsp` over stdio against the built binary, each scenario opens a fixture, issues `textDocument/prepareRename` then `textDocument/rename`, applies the returned `WorkspaceEdit`, and asserts the resulting file contents and the re-indexed state. Follow the harness and patterns in [E29-e2e-testing](../foundations/E29-e2e-testing.md).

### 12.1 Coverage target

**100% of the feature's scope, end to end** — every rename kind, the atomic-edit shape, the post-rename freshness, and the `null` paths. See the policy in [E29-e2e-testing](../foundations/E29-e2e-testing.md#2-coverage-policy).

### 12.2 Scenarios

| # | Journey | Path | Expected outcome |
|---|---|---|---|
| E2E-01 | `prepareRename` on `class User` | happy | range + placeholder `"User"` returned |
| E2E-02 | `prepareRename` on a plain import | error | `null` returned |
| E2E-03 | Rename `User` → `Account` | happy | class, FK string, and relationship target all rewritten in one edit |
| E2E-04 | Apply the `User` rename, then re-query definition from `post.py` | happy | resolves to `class Account`; no stale `users.id` reference |
| E2E-05 | Rename `User.id` → `uuid` | happy | attribute and matching FK column half rewritten; unrelated `id`s untouched |
| E2E-06 | Rename `User.posts` → `articles` | happy | attribute and `back_populates="posts"` both rewritten |
| E2E-07 | Rename where one reference is unresolvable | error | resolvable references rewritten; the unresolvable one left as-is |
| E2E-08 | Rename touching a multi-byte identifier (non-ascii) | happy | edit ranges correct under both UTF-8 and UTF-16 |

### 12.3 Acceptance criteria & Definition of Done

The §12.2 scenarios, written Given/When/Then, are this feature's acceptance criteria:

| # | Given | When | Then |
|---|---|---|---|
| AC-01 | my cursor is on `class User` | I trigger `prepareRename` | I get the `User` range and placeholder |
| AC-02 | my cursor is on a plain import | I trigger `prepareRename` | the server returns `null` |
| AC-03 | the clean-blog workspace is indexed | I rename `User` to `Account` | the class, `ForeignKey("users.id")`, and `relationship("User")` are all rewritten in one atomic edit |
| AC-04 | I just renamed `User` to `Account` | I jump to definition from `Post.author` | I land on `class Account`, with no dangling `users.id` reference anywhere |
| AC-05 | the clean-blog workspace is indexed | I rename the relationship `User.posts` to `articles` | the `back_populates="posts"` on `Post.author` becomes `back_populates="articles"` |

**Definition of Done:** every `REQ-RN-NN` has a passing test (§11.4), every acceptance scenario above passes, and the §13.1 security posture is verified.

## 13. Non-Functional Requirements

### 13.1 Security & Privacy

- **Access & authorization** — none; a single-user developer tool. Rename reads the in-memory index and returns edits; the client, not the server, writes any file.
- **Input & validation** — the cursor position and the new name are the only inputs. An out-of-range position yields `null`; the server never writes to disk and never executes the new name (P1, P3).
- **Data sensitivity** — none. The feature opens no network connection, sends no telemetry, and handles no secrets. Logs go to stderr or the configured `log_file`, never stdout.
- **Baseline** — stays within the suite-wide envelope stated once in the [constitution](../constitution.md); it computes a `WorkspaceEdit` over cached facts and nothing more.

## 16. Cross-References

- **Depends on:** [constitution](../constitution.md) — P4 (rewrite only positively-resolved references) and P5 (companion to the Python LSP); [E07-data-model](../foundations/E07-data-model.md) — the name ranges, FK `table`/`column`, relationship `target_range`, and `back_populates_range` that anchor every edit; [E30-extraction-and-indexing](../foundations/E30-extraction-and-indexing.md) — the reference reconciliation rename relies on to find every occurrence.
- **Related:** [E01-architecture](../foundations/E01-architecture.md) — the no-stale-data guarantee that makes the post-rename index consistent (REQ-RN-08); [F06-find-references](F06-find-references.md) — the same reference graph, listed instead of rewritten; [F05-go-to-definition](F05-go-to-definition.md) — confirms a rename landed by resolving the new name; [E17-testing](../foundations/E17-testing.md) / [E29-e2e-testing](../foundations/E29-e2e-testing.md) — the fixtures and harness behind the test plans.

## 17. Changelog

- **2026-06-18** — Approved.
- **2026-06-18** — Removed [F08-symbols](F08-symbols.md) from the Related list: F08 is now narrowed to Alembic-revision workspace symbols and no longer relates to model navigation.
- **2026-06-17** — Initial draft. Ported the legacy `rename.rs` paths (model, column, relationship) into nine requirements, made explicit the FK-table-half-only rewrite (REQ-RN-04), the both-halves column match (REQ-RN-05), the single-atomic-`WorkspaceEdit` rule (REQ-RN-07), and cross-linked the [E01](../foundations/E01-architecture.md) no-stale-data guarantee for the post-rename index (REQ-RN-08). Added the testing and E2E plans against the `clean-blog` cast.
