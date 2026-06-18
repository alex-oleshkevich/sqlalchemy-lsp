# E30 — Extraction & Indexing

> **Status:** Draft
>
> **Version:** 0.1   ·   **Last updated:** 2026-06-17
>
> **Purpose:** How the server turns Python source into the facts [E07](E07-data-model.md) defines — the tree-sitter walk, the detection indicators that decide what to look at, what counts as a model, and the resolution rules for annotated columns, forward references, and user-defined base classes.
>
> **Depends on:** [E07-data-model](E07-data-model.md), [constitution](../constitution.md)   ·   **Related:** [E01-architecture](E01-architecture.md), [F01-orm-correctness-diagnostics](../features/F01-orm-correctness-diagnostics.md), [F02-best-practice-lints](../features/F02-best-practice-lints.md), [F13-alembic-support](../features/F13-alembic-support.md)

> Requirement tag: **EXTRACT**

---

## 1. Purpose & Scope

This spec is the bridge between source code and the workspace index. It says how the server decides a file is worth looking at, how it walks the tree-sitter parse to pull out models and migrations, and — the hard part — how it resolves the indirect ways SQLAlchemy lets you name a type, a base, or another model.

This spec covers:

- The tree-sitter Python parse and the resilience rule for partial/`ERROR` trees.
- The fast string-check **indicators** that gate SQLAlchemy and Alembic extraction.
- What is and isn't a model — the `__tablename__`-or-resolved-base rule.
- The class-body walk that produces columns, relationships, and table-args.
- Relationship-alias tracking (`relationship as rel`).
- **Three resolution rules:** `Annotated[...]` columns, forward references / lambdas / quoted names, and user-defined base classes (with their `MetaData`).

## 2. Non-Goals / Out of Scope

- The **shapes** extraction produces — owned by [E07](E07-data-model.md).
- The **pipeline** that calls extraction (debounce, generation counter, `spawn_blocking`, watcher events) — owned by [E01](E01-architecture.md).
- The **meaning** of any diagnostic that reads the resolved facts — owned by [F01](../features/F01-orm-correctness-diagnostics.md)/[F02](../features/F02-best-practice-lints.md)/[F13](../features/F13-alembic-support.md).
- Raw-SQL parsing inside `text()` — a deliberate Non-Goal of the whole product (see the index's Out-of-scope note).

## 3. Background & Rationale

SQLAlchemy is friendly to humans and hard on parsers. The same column can be written four ways; a model's type can travel inside an `Annotated[...]`; a relationship can name its target as a class, a string, or a lambda that defers to a name not yet defined. A naive "look for `mapped_column`" pass catches the easy cases and silently misses the rest — which then surface as false diagnostics, the one thing the constitution's P4 forbids.

So extraction is two jobs. The first is mechanical: walk the tree, find the class bodies, read the assignments. The second is **resolution** — turning the indirect forms into the same resolved fact the easy forms produce, so a feature never has to know which spelling the user chose. We do all of this statically, on the tree-sitter tree, never importing or running the user's code (P1).

The mechanical walk is ported from the legacy extractor, which handled the common forms well. The three resolution rules are the new work this spec adds, drawn from the plan's foundation row.

## 4. Concepts & Definitions

- **Indicator** — a cheap substring check (`from sqlalchemy`, `Mapped[`) that decides whether a file is worth a full parse-and-walk. (Defined in §5.2.)
- **`Annotated[...]` column** — a 2.0 idiom where column config travels with the type. (Canonical definition in [glossary](../glossary.md).)
- **Forward reference** — a model named as a string or lambda before the class exists. (Canonical definition in [glossary](../glossary.md).)
- **Declarative base** — the project's own root class all models inherit; it owns the shared `MetaData`. (Canonical definition in [glossary](../glossary.md).)
- **Resolution** — turning an indirect spelling (a quoted name, an alias, a `type_annotation_map` entry) into the same resolved fact a direct spelling would produce.

## 5. Detailed Specification

### 5.1 Parse first, walk second

Extraction always starts from a tree-sitter Python parse — never a regex over raw text.

**REQ-EXTRACT-01 — Parse with tree-sitter; never reject a partial tree.**

The server parses every Python file with `tree-sitter-python` and walks the resulting tree. The user is mid-keystroke most of the time, so the tree will often contain `ERROR` nodes. The walk treats those as ordinary nodes it simply can't read — it extracts what it can from the well-formed siblings and returns the rest. A handler that hits an unexpected node shape returns what it has, never panics. This is the constitution's P3 made concrete: degrade, don't fail.

### 5.2 Detection indicators

Before the full walk, a fast string check decides whether a file is even relevant — most files in a Python project are neither models nor migrations.

**REQ-EXTRACT-02 — A file is a SQLAlchemy candidate if it contains any model indicator.**

The server runs cheap substring checks over the raw source before parsing. A file is worth extracting models from if it contains any of:

- `from sqlalchemy` or `import sqlalchemy`
- `Mapped[`
- `mapped_column`
- `DeclarativeBase`

```rust
// src/parsing/python.rs
pub fn has_sqlalchemy_indicators(source: &str) -> bool {
    source.contains("from sqlalchemy")
        || source.contains("import sqlalchemy")
        || source.contains("Mapped[")
        || source.contains("mapped_column")
        || source.contains("DeclarativeBase")
}
```

These are deliberately broad — a false positive just means one wasted parse, while a false negative means a model goes un-indexed and cross-file diagnostics break. We err toward parsing.

**REQ-EXTRACT-03 — A file is an Alembic candidate if it imports alembic.**

A migration file is detected independently:

```rust
// src/parsing/python.rs
pub fn has_alembic_indicators(source: &str) -> bool {
    source.contains("from alembic") || source.contains("import alembic")
}
```

The two checks are independent. A single file can match both — a migration that also references model classes — so the server runs model extraction and migration extraction on it separately, and neither suppresses the other.

### 5.3 What counts as a model

Not every class in a SQLAlchemy file is a mapped model. The walk applies one rule.

**REQ-EXTRACT-04 — A class is a model if it declares `__tablename__` or extends a resolved base.**

The walk recurses through the module — including nested scopes — and inspects each `class_definition`. A class becomes a `Model` if either holds:

1. It assigns `__tablename__` in its body, **or**
2. One of its base classes resolves to a declarative base (see [REQ-EXTRACT-09](#59-resolving-user-defined-base-classes)).

A class that matches neither — a plain helper, a Pydantic schema, a dataclass — is skipped entirely and never enters the index. This keeps the index to genuine models and keeps every model-reading diagnostic from firing on the wrong class.

### 5.4 The class-body walk

Once a class is a model, the walk reads its body statement by statement.

**REQ-EXTRACT-05 — The walk classifies each annotated assignment as a column, a relationship, or neither.**

For each assignment in the class body, the walk reads the attribute name, the type annotation, and the value:

- `__tablename__ = "posts"` → sets the model's `table_name`.
- `__table_args__ = (…)` → parsed into `TableArg`s.
- An attribute typed `Mapped[...]` whose value is a `relationship(...)` call → a `Relationship`.
- An attribute typed `Mapped[...]` or assigned `mapped_column(...)`, value *not* a relationship call → a `Column`.
- The first statement, if a bare string → the model's docstring.
- Anything else → ignored.

The relationship check happens before the column check, because a relationship attribute is also `Mapped[...]`-typed. The discriminator is the value: a `relationship(...)` call (under any tracked alias, §5.5) makes it a relationship; otherwise it's a column. This ordering is what stops an attribute literally named `customer_relationship: Mapped[str] = mapped_column(...)` from being mistaken for a relationship — its value is `mapped_column`, not a `relationship(...)` call.

**REQ-EXTRACT-06 — A duplicate column attribute is recorded, not dropped.**

If the body assigns the same attribute name twice, the later one wins in `columns`, and the earlier one's name range is pushed onto `duplicate_columns` — the raw material for the duplicate-column diagnostic (`SQLA-E103`).

### 5.5 Relationship-alias tracking

A project may import `relationship` under an alias, and extraction must still recognize the call.

**REQ-EXTRACT-07 — Track import aliases of `relationship` and treat them as relationship calls.**

Before walking classes, the extractor scans the module's `from sqlalchemy…` imports for an aliased import of `relationship` — `from sqlalchemy.orm import relationship as rel`. It collects `relationship` plus every alias into a set. The class-body walk then treats a call to any name in that set (or any `module.<name>` ending in a tracked name) as a relationship call.

Take a file that writes `from sqlalchemy.orm import relationship as sa_relationship` and then `project: Mapped["Project"] = sa_relationship("Project", back_populates="properties")`. Without alias tracking the attribute would be misread as a column; with it, the call resolves to a `Relationship` targeting `Project`. The same suffix rule handles `orm.relationship(...)` and `sa.orm.relationship(...)`.

### 5.6 Resolution rule (a): `Annotated[...]` columns

SQLAlchemy 2.0 lets column configuration travel inside the type via `Annotated`, often funneled through a `registry`. Extraction resolves both.

**REQ-EXTRACT-08a — Unwrap `Annotated[...]` to its base type and merge any embedded `mapped_column`.**

A column can carry its config in the annotation rather than the value. Take `Mapped[Annotated[int, mapped_column(primary_key=True)]]` — the real type is `int`, and the `mapped_column(primary_key=True)` inside the `Annotated` configures it exactly as a value-side `mapped_column` would. Extraction unwraps `Annotated[T, …]` to `T` for the `MappedType`, and reads any `mapped_column(...)` found among the `Annotated` metadata into the same `ColumnArgs`/`ForeignKeyRef` it would read from the value side.

**REQ-EXTRACT-08b — Resolve `registry` / `type_annotation_map` aliases to their underlying column config.**

A project can register a named type alias once and reuse it:

```python
# models/types.py — a registered annotation alias
intpk = Annotated[int, mapped_column(primary_key=True)]
str50 = Annotated[str, mapped_column(String(50))]

class Base(DeclarativeBase):
    registry = registry(type_annotation_map={...})
```

When a column is typed `Mapped[intpk]`, extraction resolves `intpk` to its `Annotated[...]` definition (collected from module-level assignments and any `registry.type_annotation_map`) and applies the embedded config. The resolved column is identical to one that spelled the `mapped_column` out inline — so `id: Mapped[intpk]` is indexed as a primary-key `int`, and no diagnostic falsely flags it as missing a primary key. When an alias can't be resolved, the type falls back to `Unknown` and the column stays silent (P4).

### 5.7 Resolution rule (b): forward references, lambdas, quoted names

A model can be named before it exists — as a string, a quoted annotation, or a lambda. All of these must resolve to the same indexed model.

**REQ-EXTRACT-08c — Resolve every deferred model name to the indexed model.**

SQLAlchemy offers several ways to name a target that isn't defined yet. Extraction normalizes all of them to a single resolved `target_model` the index can look up:

- `Mapped["User"]` — a quoted forward reference in the annotation.
- `relationship("User")` — a string target argument.
- `relationship(lambda: User)` — a lambda deferring to a name.
- `Mapped[list["Post"]]` — a quoted name inside a collection.

For each, extraction strips the quoting or lambda wrapper to recover the bare name (`"User"` → `User`), stores it as `target_model`, and keeps the literal spelling in `explicit_target` so navigation can anchor on what the user actually typed. At lookup time the name resolves against `model_index` ([E07](E07-data-model.md)). The same recovery feeds `MappedType::ForwardRef` and `MappedType::List`, so hover renders `list[Post]` whether the source said `list["Post"]` or `List[Post]`.

When the recovered name matches no indexed model, `target_model` keeps the unresolved name and navigation stays silent — but the relationship-target diagnostic (`SQLA-E401`) may still read it. Extraction resolves; it never invents.

### 5.8 Resolving column nullability and cardinality

Two facts are inferred from the annotation when the call doesn't state them.

**REQ-EXTRACT-08d — Infer nullability from `Optional`, cardinality from the collection wrapper.**

When `mapped_column(...)` sets `nullable=` explicitly, that value is taken verbatim. When it doesn't, a column is nullable exactly when its resolved type is `Optional[...]` — and `Mapped[str | None]` is treated identically to `Mapped[Optional[str]]`. A relationship's `is_list` is `true` exactly when its annotation is a collection (`Mapped[list["Post"]]`, `List[...]`) and `false` for a scalar or `Optional[scalar]`. These inferences feed the nullable-not-Optional (`SQLA-W201`) and uselist-mismatch (`SQLA-W404`) diagnostics.

### 5.9 Resolving user-defined base classes

A project almost always defines its own declarative base; base-dependent rules must read *that* base, not the literal `DeclarativeBase`.

**REQ-EXTRACT-09 — Resolve the project's own declarative base and mixins, and read the resolved base's `MetaData`.**

SQLAlchemy users rarely subclass `DeclarativeBase` directly. They write `class Base(DeclarativeBase): ...` once and inherit `Base` everywhere — often with mixins (`class Post(Base, TimestampMixin)`). Extraction must follow that chain.

The walk treats a class as a declarative base if it extends a known SQLAlchemy abstract base (`DeclarativeBase`, `DeclarativeBaseNoMeta`, `MappedAsDataclass`) **or** another resolved base. So when `User(Base)` is checked against [REQ-EXTRACT-04](#53-what-counts-as-a-model), `Base` resolves transitively to `DeclarativeBase` and `User` is recognized as a model — even though it never names `DeclarativeBase` itself.

> **Warning:** This is the subtle one. A literal allow-list of `["DeclarativeBase", "Base"]` (as the legacy code used) misfires two ways: it treats *any* class named `Base` as a base even when it isn't one, and it misses a project base named `Model` or `Entity`. Resolution by inheritance chain is what gets both right.

Base resolution also carries the base's **`MetaData`**, including its `naming_convention=`. The no-naming-convention lint (`SQLA-H107`) reads the *resolved* base's `MetaData` — so a model inheriting a `Base` that sets a `naming_convention` is clean, while one whose resolved base sets none is flagged. The rule reads the resolved base, never the literal `DeclarativeBase`, which never carries a project convention.

### 5.10 Alembic extraction

Migration files are walked separately, producing the Alembic facts.

**REQ-EXTRACT-10 — Extract revision metadata and the `op.*` calls inside upgrade/downgrade.**

For an Alembic candidate, the walk reads the module-level `revision` and `down_revision` assignments (a string, a tuple, or `None`) into a `MigrationFile`, and walks the bodies of the `upgrade()` and `downgrade()` functions for `op.*` calls. Each call's operation name, table reference, and column reference become an `OpCall` ([E07](E07-data-model.md)). The revision feeds `revision_index` so chain diagnostics ([F13](../features/F13-alembic-support.md)) can resolve parents to files.

### 5.11 Indexing

Extraction's output is handed to the index, which keys it for cross-file lookup.

**REQ-EXTRACT-11 — Hand resolved facts to the index; let it replace atomically.**

Extraction returns the resolved `FileModels` (and `MigrationFile`) for a single file. It does not touch the reverse indexes itself — it hands the facts to `WorkspaceState::update_file` / `update_migration`, which purge the file's old contributions and insert the new ones in one operation ([E07 REQ-DATA-11](E07-data-model.md)). Extraction is per-file and pure; indexing is the workspace-wide step. This split is what lets pass 1 (extract one file) and pass 2 (rebuild the index) stay independent, as [E01](E01-architecture.md) requires.

## 7. Visualizations

The flow runs left to right: a file is gated by indicators, parsed, walked into resolved facts, then handed to the index that keys them for every feature.

```mermaid
%%{init: {'theme': 'base', 'themeVariables': {'fontSize': '14px'}}}%%
flowchart LR
    classDef gate fill:#FFF3CD,stroke:#FFC107,color:#333
    classDef step fill:#CCE5FF,stroke:#4A90D9,color:#004085
    classDef resolve fill:#E2D9F3,stroke:#6F42C1,color:#3D2570
    classDef store fill:#D4EDDA,stroke:#28A745,color:#155724

    src["Python source"]:::step
    ind{"has_sqlalchemy /\nhas_alembic\nindicators?"}:::gate
    parse["tree-sitter parse\n(ERROR-tolerant)"]:::step
    walk["walk classes &\nmigration bodies"]:::step
    resolve["resolve: Annotated,\nforward-refs/lambdas,\nuser base + MetaData"]:::resolve
    facts["FileModels /\nMigrationFile"]:::step
    index["WorkspaceState\n(atomic replace)"]:::store

    src --> ind
    ind -->|"no"| skip["skip file"]:::gate
    ind -->|"yes"| parse
    parse --> walk
    walk --> resolve
    resolve --> facts
    facts -->|"update_file /\nupdate_migration"| index

    linkStyle 1 stroke:#FFC107,stroke-width:2px
    linkStyle 2 stroke:#4A90D9,stroke-width:2px
    linkStyle 5 stroke:#6F42C1,stroke-width:2px
    linkStyle 6 stroke:#28A745,stroke-width:2px
```

## 9. Examples & Use Cases

Walk the `clean-blog` `Post` through extraction. The file passes `has_sqlalchemy_indicators` on `Mapped[`. The parse yields a tree; the walk finds `class Post(Base, …)`, resolves `Base` transitively to `DeclarativeBase` ([REQ-EXTRACT-09](#59-resolving-user-defined-base-classes)) and recognizes `Post` as a model. It reads `__tablename__ = "posts"`, then `author_id: Mapped[int] = mapped_column(ForeignKey("users.id"))` as a `Column` with a `ForeignKeyRef`, and `author: Mapped["User"] = relationship(back_populates="posts")` as a `Relationship` — the quoted `"User"` resolved to `target_model = "User"` ([REQ-EXTRACT-08c](#57-resolution-rule-b-forward-references-lambdas-quoted-names)). The resolved `FileModels` goes to `update_file`, which inserts `"Post"` into `model_index` and `"posts"` into `table_index`. Now hover on `author_id` resolves its FK in one lookup, and `SQLA-H107` reads the `naming_convention` off the resolved `Base`'s `MetaData`.

## 10. Edge Cases & Failure Modes

- A half-typed class with `ERROR` nodes → the well-formed columns extract; the broken one is skipped; no crash (P3).
- `Mapped[intpk]` where `intpk` is never defined → type falls back to `Unknown`; the column is indexed with no flags and stays silent (P4).
- `relationship("Ghost")` where no `Ghost` model exists → `target_model = "Ghost"`, unresolved; navigation silent, but `SQLA-E401` may report it.
- A class named `Base` that is *not* a declarative base (doesn't extend one) → correctly *not* treated as a base, because resolution follows the inheritance chain, not the name.
- A file that is both a model module and a migration → both extractors run; neither suppresses the other ([REQ-EXTRACT-03](#52-detection-indicators)).
- A dynamic `__tablename__` (computed, not a literal) → `table_name` stays `None`; the model is still indexed if it has a resolved base; no guess at the name (P4).

## 16. Cross-References

- **Depends on:** [E07-data-model](E07-data-model.md) — the fact types extraction populates; [constitution](../constitution.md) — P1 (static only), P3 (never panic on partial code), P4 (silence on unresolvable input).
- **Related:** [E01-architecture](E01-architecture.md) — the pipeline that drives extraction and the atomic-replacement contract; [F01-orm-correctness-diagnostics](../features/F01-orm-correctness-diagnostics.md) / [F02-best-practice-lints](../features/F02-best-practice-lints.md) — read the resolved facts (e.g. `SQLA-H107` reads the resolved base's `MetaData`); [F13-alembic-support](../features/F13-alembic-support.md) — consumes the Alembic extraction.

## 17. Changelog

- **2026-06-17** — Initial draft. Ported the tree-sitter walk, detection indicators, the model-recognition rule, the class-body classification, and relationship-alias tracking from the legacy extractor. Added the three new resolution rules: `Annotated[...]`/`type_annotation_map` columns (REQ-EXTRACT-08a/b), forward references / lambdas / quoted names (REQ-EXTRACT-08c), and user-defined base + `MetaData` resolution feeding `SQLA-H107` (REQ-EXTRACT-09). Added the extract→index Mermaid flow.
