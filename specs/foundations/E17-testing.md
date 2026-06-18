# E17 — Testing

> **Status:** Draft
>
> **Version:** 0.1   ·   **Last updated:** 2026-06-17
>
> **Purpose:** How sqlalchemy-lsp is tested — the coverage policy, the test categories, the tools, and the shared fixtures every feature reuses. Each feature's own plan lives in its spec's §11 and links here.
>
> **Depends on:** [constitution](../constitution.md), [E02-folder-structure](E02-folder-structure.md)   ·   **Related:** [E29-e2e-testing](E29-e2e-testing.md), [E03-tech-stack](E03-tech-stack.md)

> Requirement tag: **TST**

---

## 1. Purpose & Scope

This spec defines how the whole server is tested and what "tested" means here. It is the authority every feature's **Testing** section (§11) defers to.

This spec covers:

- The coverage policy every feature must meet.
- The categories of test — unit and integration — and when to use each.
- The tools the suite standardizes on, including how rendered output is snapshotted.
- The shared **fixtures registry** — the `clean-blog` workspace and its per-code variants, defined once and linked everywhere.
- Requirement-traceability conventions, and the CLI/server parity rule.

Out of scope: end-to-end protocol journeys, which have their own foundation — see [E29-e2e-testing](E29-e2e-testing.md).

## 2. Coverage Policy

The non-negotiable bar every feature is written against.

**REQ-TST-01 — Every feature is 100% covered.**

Each feature ships a test plan (its spec's §11) covering **all of its behavior**: every `REQ-<TAG>-NN` maps to at least one test, and every editor-surface state (§6) and edge case (§10) has a test. A feature with uncovered behavior is not done.

This bar is per feature, not per line. A diagnostic feature covers each `SQLA-` code it owns; a hover feature covers each card it renders. The proof is the §11.4 table, never a raw percentage.

**REQ-TST-02 — Coverage is traceable, not just numeric.**

Coverage is demonstrated by the requirement-coverage table in each feature's §11.4, not only a line-coverage percentage. A green percentage with an untested requirement still fails the bar. Each feature's §11.4 is the index from a `REQ-<TAG>-NN` to the test that proves it.

So a reviewer reads the §11.4 table, not the coverage report, to decide whether a feature is done. The percentage is a safety net; the table is the contract.

## 3. Test Categories

Two categories live here; protocol journeys live in [E29](E29-e2e-testing.md).

| Category | Use it for | Speed / scope |
|---|---|---|
| **Unit** | Pure logic with no I/O — tree-sitter extraction of a model/column/relationship, the FK-string parse, nullable inference from `Mapped[Optional[...]]`, cascade-token validation, a single diagnostic rule firing on a snippet, position/range math. Rust `#[cfg(test)]` modules beside the code. | Fast, isolated. |
| **Integration** | Several layers wired together without the LSP boundary — extract a multi-file workspace → build the model/table/revision index → a feature's pure-function read resolves a cross-file FK or `back_populates` counterpart. Asserts the two-pass pipeline produces the right workspace index. | Slower, in-process. |

End-to-end tests — a real client driving the built binary over stdio — are **not** here. They are [E29](E29-e2e-testing.md)'s, and they carry capability negotiation, diagnostic publishing, and the editor-facing surfaces.

A good rule of thumb: if a behavior reads only the syntax tree, it's a unit test. If it needs the cross-file index — an FK in one file resolving to a model in another — it's an integration test. If it needs a live client and the protocol, it belongs in E29.

## 4. Tools & Frameworks

The standard toolchain; versions are pinned in [E03-tech-stack](E03-tech-stack.md).

- **Test runner:** `cargo test` for unit and integration; `pytest` + `pytest-lsp` for the E29 layer.
- **Assertions:** standard Rust assertions; snapshot tests (`insta`) for **rendered output where exactness matters** — the hover cards ([F04](../features/F04-hover.md)), the schema diagrams ([F12](../features/F12-schema-visualization.md)), and the `check` CLI console output ([F14](../features/F14-cli-linter.md)). A snapshot pins the exact text a user sees, so a stray space or reordered field is caught.
- **Fakes over mocks:** prefer real fixtures — real `.py` model files, real Alembic migration scripts, real tree-sitter parse trees — to mocks. We analyze source statically (constitution P1), so there is no database, no runtime `MetaData`, and nothing to stub. The fixtures *are* the test doubles.
- **Coverage reporter:** `cargo llvm-cov` in CI ([F16](../features/F16-release-ci.md)); the gate is the per-feature §11.4 tables, not a bare percentage.

Because the server never imports or runs user code, the suite is fully hermetic: a test reads files from a temp directory and asserts on the facts the extractor produced. No network, no spawned Python, no database connection.

## 5. Fixtures Registry

The canonical home for reusable test data. Each fixture has a stable heading so a feature deep-links it from its §11.3 — e.g. `[the back-populates-mismatch workspace](../foundations/E17-testing.md#back-populates-mismatch)`.

The pattern, borrowed from the wider LSP family: one fully consistent reference workspace, plus **one minimal broken variant per diagnostic code**. Each broken variant is the `clean-blog` cast mutated just enough to trigger exactly one finding, so a feature test asserts a single code and range with no noise.

### clean-blog

The constitution's example cast as a fully consistent, lint-clean SQLAlchemy 2.0 workspace. It holds `models/user.py`, `models/post.py`, `models/comment.py`, and `models/tag.py` — `User` (one-to-many to `Post`, one-to-one to `Profile`), `Post` (many `Comment`s, many `Tag`s via the `post_tags` association table), `Comment` (self-referential threaded replies), and `Tag` — plus a valid Alembic chain under `migrations/versions/` whose `down_revision` links form one clean line ending in a single head. The baseline every feature reads from; it produces **zero findings**.

#### 1xx — structure & constraints

### missing-tablename

`clean-blog` with `User`'s `__tablename__` removed — triggers `SQLA-W101`. Reused by [F01](../features/F01-orm-correctness-diagnostics.md), [F11](../features/F11-code-actions.md), [F14](../features/F14-cli-linter.md).

### duplicate-tablename

`clean-blog` with a second model declaring `__tablename__ = "users"` — triggers `SQLA-E102`. Reused by [F01](../features/F01-orm-correctness-diagnostics.md), [F14](../features/F14-cli-linter.md).

### duplicate-column

`clean-blog` with `Post` declaring `title` twice — triggers `SQLA-E103`. Reused by [F01](../features/F01-orm-correctness-diagnostics.md).

### missing-primary-key

`clean-blog` with `Tag`'s `id` primary key dropped — triggers `SQLA-W104`. Reused by [F01](../features/F01-orm-correctness-diagnostics.md).

### table-arg-column-not-found

`clean-blog` whose `Post.__table_args__` indexes a column name that doesn't exist on the model — triggers `SQLA-E105`. Reused by [F01](../features/F01-orm-correctness-diagnostics.md), [F05](../features/F05-go-to-definition.md).

### unnamed-constraint

`clean-blog` with a bare `UniqueConstraint` in `__table_args__` and no naming convention — triggers `SQLA-H106`. Reused by [F02](../features/F02-best-practice-lints.md), [F11](../features/F11-code-actions.md).

### no-naming-convention

`clean-blog` whose resolved declarative base sets no `naming_convention` on its `MetaData` — triggers `SQLA-H107`. Reused by [F02](../features/F02-best-practice-lints.md), [F11](../features/F11-code-actions.md).

#### 2xx — columns & types

### nullable-not-optional

`clean-blog` with a `Mapped[str]` column declared `nullable=True` — triggers `SQLA-W201`. Reused by [F02](../features/F02-best-practice-lints.md), [F11](../features/F11-code-actions.md).

### mutable-default

`clean-blog` with a column whose `default=[]` is a shared mutable literal — triggers `SQLA-W203`. Reused by [F02](../features/F02-best-practice-lints.md), [F11](../features/F11-code-actions.md).

### naive-datetime

`clean-blog` with a `DateTime` column missing `timezone=True` — triggers `SQLA-H205`. Reused by [F02](../features/F02-best-practice-lints.md), [F11](../features/F11-code-actions.md).

### unbounded-string

`clean-blog` with a bare `String()` column and a `target_dialect` that requires a length — triggers the dialect-gated `SQLA-H206`. Reused by [F02](../features/F02-best-practice-lints.md), [E15](E15-app-config.md).

#### 3xx — foreign keys

### bad-fk

`clean-blog` with `Post.author_id` pointing at `ForeignKey("user.id")` — a table that doesn't exist (the table is `users`). Triggers `SQLA-E301`. Reused by [F01](../features/F01-orm-correctness-diagnostics.md), [F05](../features/F05-go-to-definition.md), [F14](../features/F14-cli-linter.md).

### fk-column-not-found

`clean-blog` with an FK targeting `users.uid`, a column `users` doesn't have — triggers `SQLA-E302`. Reused by [F01](../features/F01-orm-correctness-diagnostics.md).

### fk-type-mismatch

`clean-blog` with `Post.author_id` declared `Mapped[str]` against the integer `users.id` — triggers `SQLA-W303`. Reused by [F01](../features/F01-orm-correctness-diagnostics.md), [F11](../features/F11-code-actions.md).

### ambiguous-foreign-keys

`clean-blog` with two FKs from `Post` to `users` and a `relationship` that omits `foreign_keys=` — triggers `SQLA-W304`. Reused by [F02](../features/F02-best-practice-lints.md), [F11](../features/F11-code-actions.md).

#### 4xx — relationships

### rel-target-not-found

`clean-blog` with `relationship("Auther")` — a model name no index knows — triggers `SQLA-E401`. Reused by [F01](../features/F01-orm-correctness-diagnostics.md).

### back-populates-mismatch

`clean-blog` where `Post.author` sets `back_populates="post"` but `User` exposes `posts` — triggers `SQLA-W402`. Reused by [F01](../features/F01-orm-correctness-diagnostics.md), [F06](../features/F06-find-references.md), [F11](../features/F11-code-actions.md).

### back-populates-not-found

`clean-blog` where `Post.author` names a `back_populates` attribute that doesn't exist on `User` — triggers `SQLA-W403`. Reused by [F01](../features/F01-orm-correctness-diagnostics.md).

### uselist-mismatch

`clean-blog` where `User.posts` is annotated `Mapped["Post"]` (scalar) but the counterpart implies a collection — triggers `SQLA-W404`. Reused by [F01](../features/F01-orm-correctness-diagnostics.md).

### missing-fk-for-relationship

`clean-blog` with a `relationship` whose two models share no foreign key — triggers `SQLA-H406`. Reused by [F01](../features/F01-orm-correctness-diagnostics.md), [F11](../features/F11-code-actions.md).

### unique-missing-one-to-one

`clean-blog` whose `User.profile` one-to-one lacks a `unique=True` on the backing FK column — triggers `SQLA-H407`. Reused by [F02](../features/F02-best-practice-lints.md), [F11](../features/F11-code-actions.md).

### unknown-cascade

`clean-blog` with `cascade="all, delete-orphen"` — a misspelled token — triggers `SQLA-W408`. Reused by [F01](../features/F01-orm-correctness-diagnostics.md).

### orphan-without-delete

`clean-blog` with `cascade="delete-orphan"` but no `delete` token — triggers `SQLA-W409`. Reused by [F01](../features/F01-orm-correctness-diagnostics.md), [F11](../features/F11-code-actions.md).

### circular-relationship

`clean-blog` mutated so two models hold mutually delete-cascading relationships forming a cycle — triggers `SQLA-H410`. Reused by [F01](../features/F01-orm-correctness-diagnostics.md).

#### 5xx — modernization & conventions

### backref-deprecated

`clean-blog` with `User.posts = relationship("Post", backref="author")` instead of the `back_populates` pair — triggers `SQLA-W501`. Reused by [F02](../features/F02-best-practice-lints.md), [F11](../features/F11-code-actions.md).

### legacy-declarative-base

`clean-blog` whose base is `Base = declarative_base()` rather than `class Base(DeclarativeBase)` — triggers `SQLA-W502`. Reused by [F02](../features/F02-best-practice-lints.md), [F11](../features/F11-code-actions.md).

### missing-mapped-annotation

`clean-blog` with a column written `name = mapped_column(String(120))` and no `Mapped[...]` annotation — triggers `SQLA-W504`. Reused by [F02](../features/F02-best-practice-lints.md), [F11](../features/F11-code-actions.md).

### import-alias

`clean-blog` with `import sqlalchemy as sql` instead of the conventional `sa` — triggers `SQLA-I505`. Reused by [F02](../features/F02-best-practice-lints.md), [F14](../features/F14-cli-linter.md).

#### 7xx — Alembic

### broken-migration-chain

`clean-blog` whose Alembic history has a migration whose `down_revision` names a revision no file defines — triggers `SQLA-W701`. Reused by [F13](../features/F13-alembic-support.md), [F14](../features/F14-cli-linter.md).

### multiple-heads

`clean-blog` whose Alembic history branches into two unmerged heads — triggers `SQLA-W702`. Reused by [F13](../features/F13-alembic-support.md).

### unknown-migration-table

`clean-blog` with an `op.add_column("userz", ...)` naming a table no model defines — triggers `SQLA-H703`. Reused by [F13](../features/F13-alembic-support.md), [F05](../features/F05-go-to-definition.md).

#### Cross-cutting fixtures

### non-ascii

A copy of `clean-blog` whose model classes, column attributes, `comment=` strings, and docstrings use multi-byte identifiers (accented and CJK characters). It pins the position-encoding edge cases ([E01](E01-architecture.md)): a hover or rename range must land on the right character whether the client negotiated UTF-8 or UTF-16. Reused by [F04](../features/F04-hover.md), [F05](../features/F05-go-to-definition.md), [F07](../features/F07-rename.md).

### large-workspace

A generated workspace — hundreds of models and thousands of columns across many files, plus a long Alembic chain — for the [E01 §8](E01-architecture.md) performance budgets: initial scan and index, relink latency after an edit, and hover/completion p95. A regression that blows a budget fails CI.

## 6. Conventions

**REQ-TST-03 — Requirement traceability.**

Every load-bearing `REQ-<TAG>-NN` is named in the test that verifies it, so a reader traces a rule to its proof and back. Each feature's §11.4 table is the index of this mapping.

- **Naming:** a test is named for the requirement and behavior it covers — `req_diag_03_flags_fk_type_mismatch`.
- **Structure:** arrange / act / assert; one behavior per test.
- **Fakes vs mocks:** real fixtures by default; there is nothing to mock because the server runs no user code (P1).
- **Where feature tests link:** every feature's §11 links here for categories, tools, and fixtures rather than restating them.

**REQ-TST-04 — One broken variant per code.**

Every diagnostic code in the catalog (the `SQLA-` codes owned by [F01](../features/F01-orm-correctness-diagnostics.md), [F02](../features/F02-best-practice-lints.md), and [F13](../features/F13-alembic-support.md)) has a named broken fixture in §5 that triggers it and nothing else. A new code is not done until its fixture exists and is linked from the feature's §11.3. This keeps each diagnostic test focused on a single, asserted finding.

**REQ-TST-05 — `check` and the server publish identical findings.**

A parity test runs `sqlalchemy-lsp check` over a fixture and compares its findings — code, file, range — against what the server publishes for the same workspace ([F14](../features/F14-cli-linter.md)). The two share one diagnostics engine (constitution: *one engine, two front-ends*), so this test keeps them from drifting.

The rule extends to fixes. `sqlalchemy-lsp check --fix` must produce **byte-identical** edits to the editor's quick fixes ([F11](../features/F11-code-actions.md)) for the same finding. A parity test applies both to a broken fixture and asserts the resulting source matches.

## 7. Running Tests & CI

`cargo test` runs unit + integration locally; `pytest tests/e2e` runs the E29 layer (it needs the built binary). The `qa.yml` workflow ([F16](../features/F16-release-ci.md)) runs both on every push and PR over the MSRV and stable toolchains, plus the coverage report and the `insta` snapshot check. A failing test, a stale snapshot, or an uncovered requirement (§2) blocks merge.

## 8. Cross-References

- **Depends on:** [constitution](../constitution.md) — the coverage principles (§4.4) this enforces; [E02-folder-structure](E02-folder-structure.md) — the `tests/` tree.
- **Related:** [E29-e2e-testing](E29-e2e-testing.md) — the protocol-journey foundation; [E03-tech-stack](E03-tech-stack.md) — pinned tool versions; [F01](../features/F01-orm-correctness-diagnostics.md), [F02](../features/F02-best-practice-lints.md), [F11](../features/F11-code-actions.md), [F14](../features/F14-cli-linter.md) — the per-code fixtures, the quick-fix parity, and the CLI parity test.

## 9. Changelog

- **2026-06-17** — Initial draft: the unit/integration split, the `cargo test` + `insta` + `cargo llvm-cov` toolchain, the named `clean-blog` fixtures registry with one broken variant per `SQLA-` code, the `non-ascii` and `large-workspace` fixtures, requirement traceability, and the CLI/server parity rule (REQ-TST-05).
