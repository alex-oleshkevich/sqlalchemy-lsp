# F05 — Go to Definition

> **Status:** Approved
>
> **Version:** 0.1   ·   **Last updated:** 2026-06-18
>
> **Purpose:** Jump from a SQLAlchemy or Alembic reference to the thing it names — a foreign key to its target column, a relationship to its target model, a `back_populates` to its counterpart, a `__table_args__` column to its definition, and a migration's table or column to the model behind it.
>
> **Depends on:** [constitution](../constitution.md), [E07-data-model](../foundations/E07-data-model.md), [E30-extraction-and-indexing](../foundations/E30-extraction-and-indexing.md)   ·   **Related:** [E01-architecture](../foundations/E01-architecture.md), [E17-testing](../foundations/E17-testing.md), [E29-e2e-testing](../foundations/E29-e2e-testing.md), [F06-find-references](F06-find-references.md), [F07-rename](F07-rename.md), [F13-alembic-support](F13-alembic-support.md)

> Requirement tag: **DEF**

---

## 1. Purpose & Scope

When your cursor sits on a SQLAlchemy reference, *go to definition* takes you to what it points at. Click `ForeignKey("users.id")` and land on the `id` column of `User`; click a relationship's target and land on the class; click `back_populates="posts"` and land on the `posts` relationship that completes the pair.

This spec covers the definition targets the server resolves:

- A **foreign key** — string (`ForeignKey("users.id")`) or attribute (`ForeignKey(User.id)`) — to its target column.
- A **relationship target** to the model it names.
- A **`back_populates`** value to the counterpart relationship on the target model.
- A **`__table_args__` column reference** to that column's definition in the same model.
- An **Alembic table or column reference** in an `op.*` call to the model (or column) it touches.
- A **bare model-name fallback** — a quoted or written model name anywhere in a SQLAlchemy file resolves to the class.

## 2. Non-Goals / Out of Scope

- **Generic Python go-to-definition** — jumping from an ordinary import, function, or variable to its source belongs to the user's Python LSP, never to us (constitution P5; [ADR-007](../decisions/ADR-007-companion-to-python-lsp.md)).
- **How references are extracted** — the ranges this feature reads (the FK range, the `target_range`, the `back_populates_range`) are produced by [E30](../foundations/E30-extraction-and-indexing.md) and stored by [E07](../foundations/E07-data-model.md).
- **Finding everything that points *at* a definition** — that is the reverse direction, owned by [F06-find-references](F06-find-references.md).
- **Alembic chain navigation** (jumping `down_revision` → parent migration) — owned by [F13-alembic-support](F13-alembic-support.md).

## 3. Background & Rationale

SQLAlchemy threads a model graph through strings and forward references. A foreign key names another table as `"users.id"`; a relationship names its target as `"User"` or a lambda; `back_populates` names a sibling relationship by attribute. Your editor's Python LSP can't follow any of these — to it they're just string literals. That's exactly the gap we fill.

Every jump is a lookup, not a search. The workspace index already joins a table name to its model and a model name to its file and range ([E07](../foundations/E07-data-model.md)). So a foreign key resolves in two hops — `table_index["users"] → "User"`, then `model_index["User"]` → the file and the column's range — without re-parsing anything. The behavior is ported from the legacy server's `definition.rs`, which proved these jumps against real projects.

The one rule that governs every case is the constitution's P4: **only navigate to something we can actually see.** A relationship that names a model we never indexed, a foreign key onto a table that doesn't exist — these return `null`, never a guess.

## 4. Concepts & Definitions

- **Definition target** — the location a reference resolves to: a model's class-name range, a column's attribute range, or a relationship's attribute range.
- **Foreign key (FK)** — a `ForeignKey("users.id")` or `ForeignKey(User.id)` constraint. (Canonical definition in [glossary](../glossary.md).)
- **`back_populates`** — names the reverse relationship on the target model. (Canonical definition in [glossary](../glossary.md).)
- **Workspace index** — the in-memory `model_index` / `table_index` / `revision_index` lookups every jump reads. (Canonical definition in [glossary](../glossary.md).)

## 5. Detailed Specification

The handler is a pure function: it takes the workspace state, a URI, and a cursor position, and returns an LSP `Location` (or `null`). It checks the cursor against each kind of reference in the file's models in turn, returns on the first hit, then falls through to the Alembic and bare-name paths. The order below is the order it checks.

### 5.1 Foreign key → target column

A foreign key is the most common jump: from the string that names another table to the column it points at.

**REQ-DEF-01 — A cursor on a foreign-key reference resolves to the target column's definition.**

When the cursor lands inside a column's `ForeignKeyRef.range`, the server splits the reference into its table and column halves, resolves the table to a model through `table_index`, and returns the location of that model's matching column. Take `Post.author_id` in the `clean-blog` cast: the cursor sits on `ForeignKey("users.id")`, the server resolves `"users"` → `User` and returns the range of `User.id`.

Both spellings resolve the same way. A string FK (`ForeignKey("users.id")`) names a table; an attribute FK (`ForeignKey(User.id)`) names a model — [E30](../foundations/E30-extraction-and-indexing.md) reconciles both into the one `ForeignKeyRef` shape, and this feature resolves it through the index at jump time.

**REQ-DEF-02 — A foreign key whose table resolves but whose column does not falls back to the model.**

If `table_index` resolves the table to a model but that model has no column by the named name, the jump still lands you on the model's class-name range rather than failing. Pointing somewhere useful beats pointing nowhere — you wanted to inspect the target, and the class is the next best anchor.

### 5.2 Relationship → target model

A relationship names another model; the jump lands on that model's class.

**REQ-DEF-03 — A cursor on a relationship's target resolves to the target model's class.**

When the cursor is inside a relationship's `target_range` — the written target, whether `relationship("User")`, `relationship(User)`, or a lambda — the server looks up the resolved `target_model` in `model_index` and returns the model's class-name range. On `Post.author`'s `relationship("User", …)`, the jump lands on `class User`.

The resolved `target_model` is what the index keys on, so a forward reference (`relationship("User")`), a lambda (`relationship(lambda: User)`), and a bare annotation all reach the same class ([E30 REQ-EXTRACT-08c](../foundations/E30-extraction-and-indexing.md)).

### 5.3 `back_populates` → counterpart relationship

`back_populates` names a relationship on the *other* model; the jump lands on that sibling.

**REQ-DEF-04 — A cursor on a `back_populates` value resolves to the counterpart relationship.**

When the cursor is inside a relationship's `back_populates_range`, the server resolves the relationship's `target_model`, finds that model, and returns the range of the relationship whose attribute name matches the `back_populates` string. On `Post.author`'s `back_populates="posts"`, the server resolves the target `User`, finds `User.posts`, and lands you there. This completes the bidirectional pair `Post.author ↔ User.posts` by name.

When the named counterpart doesn't exist on the target model — a mismatch the `SQLA-W402`/`SQLA-W403` diagnostics flag — the jump returns `null` (§5.7).

### 5.4 `__table_args__` column → its definition

A column named inside `__table_args__` (an index, a unique constraint) jumps to that column's `mapped_column` definition.

**REQ-DEF-05 — A cursor on a `__table_args__` column string resolves to the column it names.**

`__table_args__` carries constructs like `Index("ix_posts_title", "title")` and `UniqueConstraint("author_id", "title")`. Each column string has its own range ([E07 REQ-DATA-06](../foundations/E07-data-model.md)). When the cursor is inside one of those ranges, the server looks the name up in the *same model's* columns and returns the column's attribute range. The jump stays inside the file — a table-arg always names a column of its own model.

### 5.5 Alembic table / column → model

Inside a migration, a table or column named in an `op.*` call jumps to the model (or column) it operates on.

**REQ-DEF-06 — A cursor on an Alembic operation's table reference resolves to the model behind that table.**

When the file is a migration and the cursor is inside an `OpCall`'s table reference, the server resolves the table name through `table_index` and returns the target model's class-name range. So in `op.add_column("posts", …)`, clicking `"posts"` lands on `class Post`.

**REQ-DEF-07 — A cursor on an Alembic operation's column reference resolves to that column on the operation's model.**

When the cursor is inside an `OpCall`'s column reference, the server resolves the operation's table to a model and returns that model's matching column. In `op.add_column("posts", sa.Column("subtitle", …))`, clicking `"subtitle"` lands on `Post.subtitle` — but only if `Post` actually has that column. If the column doesn't exist (a brand-new column the migration is adding, or a typo), the jump returns `null` (P4).

### 5.6 Bare model-name fallback

When no structured reference matches, a written model name still resolves.

**REQ-DEF-08 — A cursor on a bare model name in a SQLAlchemy file resolves to its class.**

As a last resort, the server reads the deepest syntax node at the cursor from the cached parse tree, strips surrounding quotes, and — if the resulting text is a key in `model_index` — jumps to that model. This catches a model name written somewhere the structured passes don't cover, such as a quoted name in a context the extractor didn't classify. It never re-parses; it reads the tree the index already cached ([E07 REQ-DATA-13](../foundations/E07-data-model.md)).

### 5.7 Unresolved targets return null

The unifying rule: when the server can't see the target, it stays silent.

**REQ-DEF-09 — Any reference that resolves to nothing returns `null`.**

A relationship target that names no indexed model, a foreign key onto an unknown table, a `back_populates` with no matching counterpart, an Alembic column the model doesn't have — each returns `null`. The server never guesses a "closest" target and never invents a location. This is the constitution's P4 applied to navigation: we navigate only to what we can positively resolve.

### 5.8 Cursor outside any reference returns null

**REQ-DEF-10 — A cursor that isn't on any navigable reference returns `null`.**

If the cursor sits on plain Python — an ordinary import, a function call, a variable — the handler matches no SQLAlchemy reference and returns `null`, ceding the position to the Python LSP (constitution P5). We answer only inside SQLAlchemy and Alembic constructs.

## 7. Visualizations

The table below maps each navigable reference to what it resolves to and the index path it takes. Every path is a lookup over the in-memory index — no file is re-parsed.

| Cursor is on | Resolves to | Index path |
|---|---|---|
| FK string/attr (`ForeignKey("users.id")`) | target column (`User.id`) | `table_index["users"]` → `model_index["User"]` → column |
| FK whose column is missing | target model class | `table_index` → `model_index` |
| Relationship target (`relationship("User")`) | target model class | `model_index["User"]` |
| `back_populates="posts"` | counterpart relationship (`User.posts`) | `model_index[target]` → relationship by name |
| `__table_args__` column string | column in same model | model's own `columns` |
| Alembic `op.*` table ref | model behind the table | `table_index` → `model_index` |
| Alembic `op.*` column ref | column on that model | `table_index` → `model_index` → column |
| Bare model name (fallback) | model class | `model_index[name]` |
| anything unresolved | `null` (P4) | — |

## 9. Examples & Use Cases

Walk the `clean-blog` cast. You're reading `models/post.py` and your cursor is inside `ForeignKey("users.id")` on `author_id`. The server reads the column's `ForeignKeyRef`, splits `"users.id"` into `users` and `id`, resolves `table_index["users"] → "User"` and `model_index["User"]` → `models/user.py`, finds `User`'s `id` column, and returns its range. Your editor opens `user.py` with the cursor on `id` — all from the index, no file touched (REQ-DEF-01).

Now your cursor moves to `back_populates="posts"` on the `author` relationship. The server resolves the relationship's target (`User`), finds `User.posts`, and lands you there, completing the `Post.author ↔ User.posts` pair (REQ-DEF-04). Later a teammate renames `User` to `Account` and re-saves; the index rebuilds ([E01](../foundations/E01-architecture.md)), and the next jump from `author` resolves to `Account` without you reopening anything. But if the rename left `back_populates="posts"` pointing at a `User.posts` that no longer exists under that name, the jump from `back_populates` now returns `null` rather than a stale location (REQ-DEF-09).

## 10. Edge Cases & Failure Modes

- **FK table resolves, column missing** → jump to the model class, not `null` (REQ-DEF-02).
- **Relationship target names no indexed model** (`relationship("Ghost")`) → `null`; the `SQLA-E401` diagnostic may still flag it, but navigation stays silent (REQ-DEF-09).
- **`back_populates` with no matching counterpart** → `null`.
- **Alembic column the model doesn't have** (a column being added) → `null`, not a guess (REQ-DEF-07).
- **Same table name in two files** (`SQLA-E102` duplicate) → resolves to the last writer in `table_index`; the collision is a diagnostic, not a navigation error.
- **Cursor on plain Python** → `null`; the Python LSP owns it (REQ-DEF-10).
- **Partial / `ERROR`-node file** → the handler reads whatever facts extracted; an un-extracted reference simply isn't navigable, and nothing crashes (P3).
- **Target model removed since last index** → `null`; the rebuilt index has no entry, so no dangling jump ([E01](../foundations/E01-architecture.md) no-stale-data).

## 11. Testing

Go-to-definition is tested by placing a cursor on each kind of reference in a known fixture and asserting the returned location's URI and range — plus the negative cases that must return `null`.

### 11.1 Scope & coverage

Target: **100% of this feature's behavior is covered.** Every `REQ-DEF-NN` maps to at least one test; every edge case (§10) has a test. See the policy in [E17-testing](../foundations/E17-testing.md#2-coverage-policy).

### 11.2 Test plan

Each row is a behavior under test. Shared fixtures live in [E17-testing](../foundations/E17-testing.md#5-fixtures-registry).

| Behavior / scenario | Type | Fixtures | Verifies |
|---|---|---|---|
| FK string `ForeignKey("users.id")` → `User.id` range | integration | [clean-blog](../foundations/E17-testing.md#clean-blog) | REQ-DEF-01 |
| Attribute FK `ForeignKey(User.id)` → `User.id` range | integration | [clean-blog](../foundations/E17-testing.md#clean-blog) | REQ-DEF-01 |
| FK whose column is missing → falls back to model class | integration | [fk-column-not-found](../foundations/E17-testing.md#fk-column-not-found) | REQ-DEF-02 |
| Relationship target `relationship("User")` → `class User` | integration | [clean-blog](../foundations/E17-testing.md#clean-blog) | REQ-DEF-03 |
| `back_populates="posts"` → `User.posts` relationship | integration | [clean-blog](../foundations/E17-testing.md#clean-blog) | REQ-DEF-04 |
| `__table_args__` column string → column definition | integration | [clean-blog](../foundations/E17-testing.md#clean-blog) | REQ-DEF-05 |
| Alembic `op.*` table ref → model class | integration | [clean-blog](../foundations/E17-testing.md#clean-blog) | REQ-DEF-06 |
| Alembic `op.*` column ref → column on model | integration | [clean-blog](../foundations/E17-testing.md#clean-blog) | REQ-DEF-07 |
| Bare quoted model name → model class | integration | [clean-blog](../foundations/E17-testing.md#clean-blog) | REQ-DEF-08 |
| Relationship target naming no model → `null` | integration | [rel-target-not-found](../foundations/E17-testing.md#rel-target-not-found) | REQ-DEF-09 |
| FK onto unknown table → `null` | integration | [bad-fk](../foundations/E17-testing.md#bad-fk) | REQ-DEF-09 |
| `back_populates` with no counterpart → `null` | integration | [back-populates-not-found](../foundations/E17-testing.md#back-populates-not-found) | REQ-DEF-09 |
| Cursor on plain Python → `null` | unit | [clean-blog](../foundations/E17-testing.md#clean-blog) | REQ-DEF-10 |
| Multi-byte identifiers: ranges land on correct UTF positions | integration | [non-ascii](../foundations/E17-testing.md#non-ascii) | REQ-DEF-01, REQ-DEF-03 |

### 11.3 Fixtures

All fixtures are the shared ones in [E17-testing](../foundations/E17-testing.md#5-fixtures-registry); this feature defines none of its own.

- **clean-blog** — the happy-path source for every resolving jump.
- **bad-fk / fk-column-not-found / rel-target-not-found / back-populates-not-found** — the per-code broken variants exercising the `null` and fallback paths.
- **non-ascii** — pins UTF-8/UTF-16 range correctness for jumps into multi-byte identifiers.

### 11.4 Requirement coverage

| Requirement | Covered by |
|---|---|
| REQ-DEF-01 | "FK string → `User.id`", "Attribute FK → `User.id`", "non-ascii ranges" |
| REQ-DEF-02 | "FK whose column is missing → model class" |
| REQ-DEF-03 | "Relationship target → `class User`", "non-ascii ranges" |
| REQ-DEF-04 | "`back_populates` → `User.posts`" |
| REQ-DEF-05 | "`__table_args__` column string → definition" |
| REQ-DEF-06 | "Alembic table ref → model class" |
| REQ-DEF-07 | "Alembic column ref → column on model" |
| REQ-DEF-08 | "Bare quoted model name → model class" |
| REQ-DEF-09 | "relationship → `null`", "FK unknown table → `null`", "`back_populates` no counterpart → `null`" |
| REQ-DEF-10 | "Cursor on plain Python → `null`" |

## 12. End-to-End Test Plan

Driven by `pytest-lsp` over stdio against the built binary, each scenario opens a fixture, issues `textDocument/definition` at a position, and asserts the returned `Location` (or `null`). Follow the harness and patterns in [E29-e2e-testing](../foundations/E29-e2e-testing.md).

### 12.1 Coverage target

**100% of the feature's scope, end to end** — every resolving jump and every `null` path. See the policy in [E29-e2e-testing](../foundations/E29-e2e-testing.md#2-coverage-policy).

### 12.2 Scenarios

| # | Journey | Path | Expected outcome |
|---|---|---|---|
| E2E-01 | Definition on `ForeignKey("users.id")` in `post.py` | happy | `Location` in `user.py` at `User.id` |
| E2E-02 | Definition on `relationship("User")` target | happy | `Location` at `class User` |
| E2E-03 | Definition on `back_populates="posts"` | happy | `Location` at `User.posts` |
| E2E-04 | Definition on a `__table_args__` column string | happy | `Location` at that column in the same model |
| E2E-05 | Definition on an Alembic `op.add_column("posts", …)` table | happy | `Location` at `class Post` |
| E2E-06 | Definition on an unresolvable relationship target | error | response is `null` |
| E2E-07 | Definition on a FK onto an unknown table | error | response is `null` |
| E2E-08 | Definition on plain Python (an import) | error | response is `null` (ceded to Python LSP) |
| E2E-09 | Cross-file freshness: rename `User` in `user.py`, then jump from `post.py`'s relationship | happy | resolves to the new class without reopening `post.py` |
| E2E-10 | Definition into a multi-byte identifier (non-ascii) | happy | range correct under both UTF-8 and UTF-16 |

### 12.3 Acceptance criteria & Definition of Done

The §12.2 scenarios, written Given/When/Then, are this feature's acceptance criteria:

| # | Given | When | Then |
|---|---|---|---|
| AC-01 | the clean-blog workspace is indexed | I request definition on `ForeignKey("users.id")` | I land on `User.id` |
| AC-02 | the clean-blog workspace is indexed | I request definition on `back_populates="posts"` | I land on `User.posts` |
| AC-03 | a relationship names a model that doesn't exist | I request definition on its target | the server returns `null` |
| AC-04 | my cursor is on an ordinary Python import | I request definition | the server returns `null` |
| AC-05 | `User` was just renamed in another file | I request definition from a relationship targeting it | I land on the renamed class |

**Definition of Done:** every `REQ-DEF-NN` has a passing test (§11.4), every acceptance scenario above passes, and the §13.1 security posture is verified.

## 13. Non-Functional Requirements

### 13.1 Security & Privacy

- **Access & authorization** — none; a single-user developer tool with no auth surface. Definition reads only the in-memory index built from local workspace files.
- **Input & validation** — the cursor position is the only input; an out-of-range position yields `null`, never a panic (P3).
- **Data sensitivity** — none. The feature executes no user code (P1), opens no network connection, sends no telemetry, and handles no secrets. Logs go to stderr or the configured `log_file`, never stdout.
- **Baseline** — stays within the suite-wide envelope stated once in the [constitution](../constitution.md); this feature reads index data and returns locations, nothing more.

## 16. Cross-References

- **Depends on:** [constitution](../constitution.md) — P4 (silence on unresolvable targets) and P5 (companion to the Python LSP); [E07-data-model](../foundations/E07-data-model.md) — the `ForeignKeyRef`, `Relationship`, `TableArg`, and `OpCall` ranges and the `model_index`/`table_index` lookups every jump reads; [E30-extraction-and-indexing](../foundations/E30-extraction-and-indexing.md) — the resolution of forward references, lambdas, and quoted names that make a single jump path serve every spelling.
- **Related:** [E01-architecture](../foundations/E01-architecture.md) — the no-stale-data guarantee that keeps jumps resolving against the current index; [F06-find-references](F06-find-references.md) — the reverse direction; [F07-rename](F07-rename.md) — reuses the same reference graph to rewrite, not navigate; [F13-alembic-support](F13-alembic-support.md) — owns the Alembic facts these jumps consume; [E17-testing](../foundations/E17-testing.md) / [E29-e2e-testing](../foundations/E29-e2e-testing.md) — the fixtures and harness behind the test plans.

## 17. Changelog

- **2026-06-18** — Removed [F08-symbols](F08-symbols.md) from the Related list: F08 is now narrowed to Alembic-revision workspace symbols and no longer relates to model navigation.
- **2026-06-18** — Approved.
- **2026-06-17** — Initial draft. Ported the legacy `definition.rs` jumps (FK → column, relationship → model, `back_populates` → counterpart, `__table_args__` column → definition, Alembic table/column → model, bare-name fallback) into ten requirements, made the unresolved-target `null` rule explicit per P4, and added the testing and E2E plans against the `clean-blog` cast.
