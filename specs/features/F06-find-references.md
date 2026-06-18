# F06 — Find References

> **Status:** Draft
>
> **Version:** 0.1   ·   **Last updated:** 2026-06-17
>
> **Purpose:** From a model, column, or relationship definition, list everywhere across the workspace that points at it — foreign keys, relationship targets, base-class uses, and `back_populates` counterparts.
>
> **Depends on:** [constitution](../constitution.md), [E07-data-model](../foundations/E07-data-model.md), [E30-extraction-and-indexing](../foundations/E30-extraction-and-indexing.md)   ·   **Related:** [E01-architecture](../foundations/E01-architecture.md), [E17-testing](../foundations/E17-testing.md), [E29-e2e-testing](../foundations/E29-e2e-testing.md), [F05-go-to-definition](F05-go-to-definition.md), [F07-rename](F07-rename.md), [F08-symbols](F08-symbols.md)

> Requirement tag: **REF**

---

## 1. Purpose & Scope

*Find references* is go-to-definition run backwards. Put your cursor on a model, a column, or a relationship and ask "what uses this?" — the server returns every place in the workspace that names it: the foreign keys that target it, the relationships that point at it, the classes that inherit it, and the `back_populates` strings that complete it.

This spec covers the three reference kinds the server resolves:

- **Model references** — foreign keys whose table is this model, relationships whose target is this model, and classes whose base list contains this model.
- **Column references** — foreign keys whose target column is this column (and whose table is this column's model).
- **Relationship references** — `back_populates` strings on other models that name this relationship.

## 2. Non-Goals / Out of Scope

- **Generic Python references** — finding every plain-Python use of a symbol belongs to the user's Python LSP (constitution P5; [ADR-007](../decisions/ADR-007-companion-to-python-lsp.md)). We answer only for SQLAlchemy constructs.
- **The forward direction** — jumping *to* a definition is owned by [F05-go-to-definition](F05-go-to-definition.md).
- **Rewriting the references** — renaming every use is owned by [F07-rename](F07-rename.md), which walks the same reference graph but produces edits instead of locations.
- **How references are extracted** — the FK, relationship, and base facts come from [E30](../foundations/E30-extraction-and-indexing.md) / [E07](../foundations/E07-data-model.md).

## 3. Background & Rationale

Before you rename `User` or delete a column, you want to know what depends on it. The Python LSP can't tell you — the dependencies live in strings (`ForeignKey("users.id")`), forward references (`relationship("User")`), and `back_populates` values it treats as opaque text. We read those as the structured references they are.

The search is a workspace scan over already-extracted facts, not a re-parse. The server walks every file's models in the index and collects the references that match — the foreign keys whose `table` equals the target, the relationships whose `target_model` equals it, the `back_populates` strings that name it. Each match contributes a `Location` with the range the extractor already recorded. The behavior is ported from the legacy `references.rs`.

Two things shape what counts as a match. First, a foreign key may name the target by *table name* (`"users"`) or by *model name* (`User`), so both are accepted ([E30](../foundations/E30-extraction-and-indexing.md) reconciles them). Second, per P4, we only return references we can positively tie to the definition under the cursor — we never return a string that merely *looks* similar.

## 4. Concepts & Definitions

- **Reference** — a location in source that names a definition: a FK string, a relationship target, a base-class name, or a `back_populates` value.
- **Model reference** — a foreign key, relationship target, or base-class use naming a model.
- **Column reference** — a foreign key whose target column is the column under the cursor.
- **Relationship reference** — a `back_populates` value naming the relationship under the cursor.

## 5. Detailed Specification

The handler is a pure function: it takes the workspace state, a URI, and a cursor position, and returns a list of `Location`s (possibly empty). It first decides *what* the cursor is on — a model name, a column name, or a relationship name — then runs the matching workspace scan.

### 5.1 Dispatch by what's under the cursor

The first job is classifying the cursor; the scan that follows depends on it.

**REQ-REF-01 — The reference search dispatches on the definition the cursor sits on.**

The server checks the cursor against the ranges in the file's models, in order:

1. Inside a model's class-name range → search for **model references** (§5.2).
2. Inside a column's attribute range → search for **column references** (§5.3).
3. Inside a relationship's attribute range → search for **relationship references** (§5.4).

The first match wins. A cursor that sits on none of these — plain Python, a string literal, whitespace — yields an empty result and cedes the position to the Python LSP (constitution P5).

### 5.2 Model references

A model is referenced three ways: by foreign keys that target its table, by relationships that target it, and by classes that inherit it.

**REQ-REF-02 — Foreign keys targeting this model's table are references.**

The server scans every model's columns across the workspace. A column's foreign key is a reference when its `table` equals this model's `table_name` *or* this model's class name (FKs may be written either way). The recorded `ForeignKeyRef.range` becomes a `Location`. So asking for references on `User` returns the `ForeignKey("users.id")` on `Post.author_id`.

**REQ-REF-03 — Relationships targeting this model are references.**

A relationship whose resolved `target_model` equals this model is a reference; its `target_range` becomes a `Location`. References on `User` therefore include the target of `Post.author`'s `relationship("User", …)`.

**REQ-REF-04 — Classes inheriting this model are references.**

When another model's `bases` list contains this model's name, that subclass is a reference and its class-name range is a `Location`. This catches a mixin or a shared base — if `User` were a base for other models, each subclass would be listed.

### 5.3 Column references

A column is referenced by the foreign keys that point at it.

**REQ-REF-05 — Foreign keys whose target column is this column are references.**

The server scans every column's foreign key across the workspace and matches on *both* halves: the FK's `column` must equal this column's name, *and* the FK's `table` must equal this column's model's table name or class name. Both conditions matter — a foreign key to `users.id` is a reference of `User.id`, but a foreign key to `posts.id` is not, even though both name a column called `id`. Asking for references on `User.id` returns the FK on `Post.author_id`.

When the column's owning model has neither a table name nor a usable class name to match on, the search returns empty rather than over-matching every `id` in the workspace (P4).

### 5.4 Relationship references

A relationship is referenced by the `back_populates` strings on the other side of the pair.

**REQ-REF-06 — `back_populates` values naming this relationship are references.**

The server scans relationships across the workspace for one that (a) targets this relationship's owning model and (b) carries a `back_populates` equal to this relationship's attribute name. The recorded `back_populates_range` becomes a `Location`. Asking for references on `User.posts` returns the `back_populates="posts"` on `Post.author` — the string that wires the pair from the other end.

### 5.5 Empty results, never guesses

**REQ-REF-07 — A definition with no references returns an empty list.**

When nothing in the workspace points at the definition under the cursor, the server returns an empty list — not `null`, not a near-miss. A column no foreign key targets, a model nothing inherits or relates to, a relationship with no `back_populates` partner: each is legitimately reference-free. The server reports exactly what it can resolve and nothing it can't (P4).

### 5.6 Including the declaration

LSP lets the client ask whether the definition itself should appear in the results.

**REQ-REF-08 — Honor the `includeDeclaration` context flag.**

The `textDocument/references` request carries a `context.includeDeclaration` boolean. When it is true, the definition's own range is included in the results alongside the references; when false, only the references are returned. The server respects the flag the client sends.

## 7. Visualizations

The table below maps each definition kind to the references the server collects and the match rule for each.

| Cursor is on | Reference kinds returned | Match rule |
|---|---|---|
| Model `User` | FKs targeting `users`/`User` | FK `table` == table name or class name |
| | relationships targeting `User` | relationship `target_model` == model |
| | subclasses of `User` | another model's `bases` contains the name |
| Column `User.id` | FKs targeting `users.id`/`User.id` | FK `column` == name **and** `table` == table/class |
| Relationship `User.posts` | `back_populates="posts"` on the pair's other side | targets this model **and** `back_populates` == attr name |
| anything unresolved | empty list (P4) | — |

## 9. Examples & Use Cases

Take the `clean-blog` cast and put your cursor on `class User` in `models/user.py`. You ask for references. The server scans the workspace: in `models/post.py` it finds `Post.author_id`'s `ForeignKey("users.id")` (table matches `users`) and `Post.author`'s `relationship("User", …)` (target matches `User`), and returns both locations (REQ-REF-02, REQ-REF-03). If `Profile` listed `User` among its bases, that class would appear too (REQ-REF-04).

Now move the cursor onto `posts` in `User.posts` and ask again. The server scans relationships for one that targets `User` and carries `back_populates="posts"` — it finds `Post.author` and returns the range of its `back_populates` string (REQ-REF-06). When a teammate later removes `Post` entirely, the index rebuilds ([E01](../foundations/E01-architecture.md)) and the same query on `User.posts` returns an empty list — the reference is genuinely gone, not stale (REQ-REF-07).

## 10. Edge Cases & Failure Modes

- **No references exist** → empty list, not `null` (REQ-REF-07).
- **FK written by model name vs. table name** → both accepted as model references (REQ-REF-02).
- **Two columns named `id` in different models** → a FK to `users.id` is a reference of `User.id` only, never `Post.id` — both halves must match (REQ-REF-05).
- **Column whose model has no table name and no usable class name** → empty, to avoid over-matching every `id` (REQ-REF-05).
- **`includeDeclaration` true vs. false** → the definition is or isn't in the list accordingly (REQ-REF-08).
- **Cursor on plain Python** → empty; the Python LSP owns it (REQ-REF-01).
- **Partial / `ERROR`-node file** → un-extracted references simply don't appear; no crash (P3).
- **Self-referential relationship** (`Comment.parent` → `Comment`) → the FK and `back_populates` on the same model are found correctly, in the same file.

## 11. Testing

Find-references is tested by placing a cursor on each definition kind in a known fixture and asserting the exact set of returned locations — including the empty-result cases.

### 11.1 Scope & coverage

Target: **100% of this feature's behavior is covered.** Every `REQ-REF-NN` maps to at least one test; every edge case (§10) has a test. See the policy in [E17-testing](../foundations/E17-testing.md#2-coverage-policy).

### 11.2 Test plan

Each row is a behavior under test. Shared fixtures live in [E17-testing](../foundations/E17-testing.md#5-fixtures-registry).

| Behavior / scenario | Type | Fixtures | Verifies |
|---|---|---|---|
| References on `User` include the FK on `Post.author_id` | integration | [clean-blog](../foundations/E17-testing.md#clean-blog) | REQ-REF-02 |
| References on `User` include the `relationship("User")` target | integration | [clean-blog](../foundations/E17-testing.md#clean-blog) | REQ-REF-03 |
| References on a base model include each subclass | integration | [clean-blog](../foundations/E17-testing.md#clean-blog) | REQ-REF-04 |
| References on `User.id` include the FK on `Post.author_id` | integration | [clean-blog](../foundations/E17-testing.md#clean-blog) | REQ-REF-05 |
| References on `User.id` exclude a FK to `posts.id` (both halves matched) | integration | [clean-blog](../foundations/E17-testing.md#clean-blog) | REQ-REF-05 |
| References on `User.posts` include `back_populates="posts"` on `Post.author` | integration | [clean-blog](../foundations/E17-testing.md#clean-blog) | REQ-REF-06 |
| Self-referential `Comment.parent` references resolve in-file | integration | [clean-blog](../foundations/E17-testing.md#clean-blog) | REQ-REF-05, REQ-REF-06 |
| Reference-free column → empty list | integration | [clean-blog](../foundations/E17-testing.md#clean-blog) | REQ-REF-07 |
| `includeDeclaration` true includes the definition; false omits it | integration | [clean-blog](../foundations/E17-testing.md#clean-blog) | REQ-REF-08 |
| Cursor on plain Python → empty list | unit | [clean-blog](../foundations/E17-testing.md#clean-blog) | REQ-REF-01 |
| Multi-byte identifiers: reference ranges land on correct UTF positions | integration | [non-ascii](../foundations/E17-testing.md#non-ascii) | REQ-REF-02, REQ-REF-05 |

### 11.3 Fixtures

All fixtures are the shared ones in [E17-testing](../foundations/E17-testing.md#5-fixtures-registry); this feature defines none of its own.

- **clean-blog** — the one-to-many, many-to-many, and self-referential references this feature collects.
- **non-ascii** — pins UTF-8/UTF-16 range correctness for references into multi-byte identifiers.

### 11.4 Requirement coverage

| Requirement | Covered by |
|---|---|
| REQ-REF-01 | "Cursor on plain Python → empty list" |
| REQ-REF-02 | "References on `User` include the FK", "non-ascii ranges" |
| REQ-REF-03 | "References on `User` include the relationship target" |
| REQ-REF-04 | "References on a base model include each subclass" |
| REQ-REF-05 | "References on `User.id` include the FK", "exclude FK to `posts.id`", "non-ascii ranges" |
| REQ-REF-06 | "References on `User.posts` include `back_populates`", "self-referential `Comment.parent`" |
| REQ-REF-07 | "Reference-free column → empty list" |
| REQ-REF-08 | "`includeDeclaration` true/false" |

## 12. End-to-End Test Plan

Driven by `pytest-lsp` over stdio against the built binary, each scenario opens a fixture, issues `textDocument/references` at a position with a `context`, and asserts the returned location set. Follow the harness and patterns in [E29-e2e-testing](../foundations/E29-e2e-testing.md).

### 12.1 Coverage target

**100% of the feature's scope, end to end** — every reference kind and every empty-result path. See the policy in [E29-e2e-testing](../foundations/E29-e2e-testing.md#2-coverage-policy).

### 12.2 Scenarios

| # | Journey | Path | Expected outcome |
|---|---|---|---|
| E2E-01 | References on `class User` | happy | locations include the FK and relationship in `post.py` |
| E2E-02 | References on `User.id` | happy | locations include the FK on `Post.author_id`, exclude unrelated `id`s |
| E2E-03 | References on `User.posts` | happy | locations include `back_populates="posts"` on `Post.author` |
| E2E-04 | References on a base class | happy | locations include each subclass |
| E2E-05 | References on a reference-free column | error | empty list returned |
| E2E-06 | References with `includeDeclaration=true` | happy | the definition's own range is included |
| E2E-07 | References on plain Python | error | empty list (ceded to Python LSP) |
| E2E-08 | Cross-file freshness: delete `Post`, then query `User.posts` | error | empty list, no stale reference |
| E2E-09 | References into a multi-byte identifier (non-ascii) | happy | ranges correct under both UTF-8 and UTF-16 |

### 12.3 Acceptance criteria & Definition of Done

The §12.2 scenarios, written Given/When/Then, are this feature's acceptance criteria:

| # | Given | When | Then |
|---|---|---|---|
| AC-01 | the clean-blog workspace is indexed | I request references on `class User` | I get the FK and relationship in `post.py` |
| AC-02 | the clean-blog workspace is indexed | I request references on `User.id` | I get the FK on `author_id` and nothing matching only on column name |
| AC-03 | the clean-blog workspace is indexed | I request references on `User.posts` | I get the `back_populates="posts"` location |
| AC-04 | a column nothing targets | I request its references | I get an empty list |
| AC-05 | `Post` was just deleted | I request references on `User.posts` | I get an empty list, no stale entry |

**Definition of Done:** every `REQ-REF-NN` has a passing test (§11.4), every acceptance scenario above passes, and the §13.1 security posture is verified.

## 13. Non-Functional Requirements

### 13.1 Security & Privacy

- **Access & authorization** — none; a single-user developer tool. References read only the in-memory index built from local workspace files.
- **Input & validation** — the cursor position and the `includeDeclaration` flag are the only inputs; an out-of-range position yields an empty list, never a panic (P3).
- **Data sensitivity** — none. The feature executes no user code (P1), opens no network connection, sends no telemetry, and handles no secrets. Logs go to stderr or the configured `log_file`, never stdout.
- **Baseline** — stays within the suite-wide envelope stated once in the [constitution](../constitution.md); a workspace scan over cached facts that returns locations only.

## 16. Cross-References

- **Depends on:** [constitution](../constitution.md) — P4 (only positively-resolved references) and P5 (companion to the Python LSP); [E07-data-model](../foundations/E07-data-model.md) — the FK `table`/`column`, relationship `target_model`/`back_populates`, and `bases` facts plus the ranges each reference reports; [E30-extraction-and-indexing](../foundations/E30-extraction-and-indexing.md) — the reconciliation of model-name vs. table-name foreign keys that the match rules rely on.
- **Related:** [E01-architecture](../foundations/E01-architecture.md) — the no-stale-data guarantee that keeps reference sets current; [F05-go-to-definition](F05-go-to-definition.md) — the forward direction over the same graph; [F07-rename](F07-rename.md) — rewrites the very references this feature lists; [E17-testing](../foundations/E17-testing.md) / [E29-e2e-testing](../foundations/E29-e2e-testing.md) — the fixtures and harness behind the test plans.

## 17. Changelog

- **2026-06-17** — Initial draft. Ported the legacy `references.rs` searches (model references via FKs, relationship targets, and bases; column references via FKs matched on both halves; relationship references via `back_populates`) into eight requirements, made the empty-result and `includeDeclaration` rules explicit, and added the testing and E2E plans against the `clean-blog` cast.
