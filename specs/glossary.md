# Glossary

> **Status:** Living (continuously maintained)
>
> **Last updated:** 2026-06-17
>
> **Purpose:** The canonical definition of every domain term used across the suite. Define a term once here, link to it everywhere.

Terms are grouped for scanning. When a spec introduces a term, it links here rather than redefining it. Examples draw from the [constitution](constitution.md)'s `clean-blog` cast.

---

## SQLAlchemy ORM

- **Declarative mapping** — the style where a Python class maps to a table by subclassing a declarative base. The 2.0 form subclasses `DeclarativeBase`.
- **Declarative base** — the root class (`class Base(DeclarativeBase)`) all models inherit from; it owns the shared `MetaData`. A project usually defines its own, so we resolve it rather than assume the literal name. Owned by [E30](foundations/E30-extraction-and-indexing.md).
- **Model** — a mapped class. For example, `User` maps to table `users`. A class is a model if it declares `__tablename__` or inherits a mapped base. Owned by [E07](foundations/E07-data-model.md).
- **`Mapped[...]`** — the PEP 484 annotation typing a mapped attribute, e.g. `Mapped[int]`, `Mapped[Optional[str]]`, `Mapped[list["Post"]]`. Its inner type drives nullability and cardinality inference.
- **`mapped_column(...)`** — the 2.0 column constructor; carries SQL type, `primary_key`, `nullable`, `unique`, `index`, `default`, `server_default`, `comment`, and an optional `ForeignKey`.
- **Column alias / `key`** — when `mapped_column(name="full_name")` differs from the Python attribute (`name`), the DB column and the attribute names diverge. Hover ([F04](features/F04-hover.md)) shows both.
- **Relationship** — a `relationship(...)` attribute linking two models, e.g. `User.posts`. Carries `back_populates`, `lazy`, `uselist`, `secondary`, `cascade`, `foreign_keys`, `remote_side`, `viewonly`.
- **`back_populates`** — names the reverse relationship on the target model, wiring a bidirectional pair (`User.posts` ↔ `Post.author`). The modern replacement for `backref`.
- **`backref`** — the legacy one-sided way to declare both ends from one side; deprecated in 2.0 (lint `SQLA-W501`).
- **Foreign key (FK)** — a `ForeignKey("users.id")` (string) or `ForeignKey(User.id)` (attribute) constraint linking a column to another table's column.
- **`__tablename__`** — the string assigning a model's table name (`"posts"`).
- **`__table_args__`** — a tuple of table-level constructs: `Index`, `UniqueConstraint`, `PrimaryKeyConstraint`, `CheckConstraint`, plus an optional trailing options dict.
- **Cascade** — the `cascade=` string controlling how operations propagate; tokens include `save-update`, `merge`, `delete`, `delete-orphan`, `all`.
- **`uselist`** — whether a relationship is a collection (`True`) or scalar (`False`); usually inferred from the annotation.
- **Association table / `secondary`** — a join table wiring many-to-many (e.g. `post_tags`). When it carries extra columns, the association-object pattern is preferred (lint `SQLA-H412`).
- **`Annotated[...]` column** — a 2.0 idiom where column config travels with the type, e.g. `Mapped[Annotated[int, mapped_column(primary_key=True)]]`, often via `registry.type_annotation_map`. Owned by [E30](foundations/E30-extraction-and-indexing.md).
- **Forward reference** — a model named as a string (`Mapped["User"]`, `relationship("User")`) or lambda (`relationship(lambda: User)`) before the class exists; resolved to the indexed model.
- **Hybrid property / association proxy / `validates`** — SQLAlchemy extension constructs the server understands well enough to lint. Owned by [F02](features/F02-best-practice-lints.md).

## Alembic

- **Migration** — a file under `migrations/versions/` with `revision`, `down_revision`, and `upgrade()`/`downgrade()` bodies of `op.*` calls.
- **Revision** — the unique id of a migration.
- **`down_revision`** — the parent revision (or tuple, for a merge); the chain of these forms the history.
- **Head** — a revision with no descendant. A healthy history has exactly one (lint `SQLA-W702`).
- **`op.*`** — the Alembic operations API (`op.add_column`, `op.create_table`, …). Owned by [F13](features/F13-alembic-support.md).

## LSP & server

- **Capability** — an LSP feature advertised at `initialize` (hover, completion, diagnostics, …).
- **Pass 1 / Pass 2** — per-file extraction (Pass 1) and the debounced workspace relink that rebuilds the index (Pass 2). Owned by [E01](foundations/E01-architecture.md).
- **Workspace index** — the in-memory model/table/revision lookup tables every feature reads. Owned by [E07](foundations/E07-data-model.md).
- **Fact** — a single extracted datum (a model, column, relationship, op call), replaced atomically when its file changes.
- **Generation counter** — the monotonic counter preventing a stale Pass 2 from publishing after a newer edit.
- **Diagnostic code** — a `SQLA-<SEV><CLASS><NN>` identifier for a finding (constitution §4.2).
- **`# noqa`** — an inline suppression comment; `# noqa: SQLA-W303` silences one code on a line. Owned by [E15](foundations/E15-app-config.md).
- **CLI/server parity** — the rule that `sqlalchemy-lsp check` and the editor server emit identical findings from one engine. Owned by [E17](foundations/E17-testing.md).
- **Companion LSP** — a general Python language server (Pyright, `pylsp`, Ruff) we run alongside; it owns generic Python (constitution P5).

## Changelog

- **2026-06-17** — Initial glossary covering SQLAlchemy ORM, Alembic, and LSP/server terms.
