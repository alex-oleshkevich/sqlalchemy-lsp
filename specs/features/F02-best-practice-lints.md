# F02 — Best-Practice Lints

> **Status:** Draft
>
> **Version:** 0.4   ·   **Last updated:** 2026-06-18
>
> **Purpose:** The configurable best-practice lint set — the rules that flag SQLAlchemy code that *works* but invites a bug, a deprecation, or a maintenance headache. Every rule ships on by default except three: two of the hardest heuristics, and one opt-in style rule.
>
> **Depends on:** [constitution](../constitution.md), [E07-data-model](../foundations/E07-data-model.md), [E30-extraction-and-indexing](../foundations/E30-extraction-and-indexing.md), [E16-conventions](../foundations/E16-conventions.md), [E15-app-config](../foundations/E15-app-config.md)   ·   **Related:** [F01-orm-correctness-diagnostics](F01-orm-correctness-diagnostics.md), [F11-code-actions](F11-code-actions.md), [E17-testing](../foundations/E17-testing.md), [E29-e2e-testing](../foundations/E29-e2e-testing.md)

> Requirement tag: **LINT**

---

## 1. Purpose & Scope

This spec defines the **best-practice lints** — the findings that don't say your code is *wrong*, but that it could be better. Where [F01](F01-orm-correctness-diagnostics.md) catches code that will misbehave at runtime (a foreign key to a table that doesn't exist), F02 catches code that runs fine today but carries a hidden cost: a naive datetime that bites you in production, a deprecated `backref` that won't survive the next major version, a column with no `Mapped[...]` annotation that the type checker can't see.

Think of the difference like a compiler error versus a compiler warning. F01 is the error; F02 is the warning your future self will thank you for heeding.

This spec covers:

- The 27 best-practice lint rules, grouped by diagnostic class (structure, columns, foreign keys, relationships, modernization, ORM extensions).
- For each rule: its `SQLA-` code, default severity, default on/off state, what triggers it, the message it emits, an example, and detectability notes.
- The default-on-except-three policy and how each rule is configured or suppressed.
- The test plan and end-to-end journeys that prove every rule fires exactly where it should.

## 2. Non-Goals / Out of Scope

- **The correctness diagnostics** — missing/duplicate `__tablename__`, unknown-FK-table, type/column mismatches, `back_populates` mismatches, cascade validity, and the rest of the `SQLA-1xx`–`4xx` *correctness core* — are owned by [F01](F01-orm-correctness-diagnostics.md). F02 reproduces only the best-practice slice of those classes and cross-references F01 for the rest; it never duplicates a rule.
- **The Alembic lints** (`SQLA-7xx`) belong to [F13](F13-alembic-support.md).
- **How a rule is toggled, re-leveled, or suppressed** — the `diagnostics.select`/`ignore`/`severity` keys, `target_dialect`, and `# noqa` syntax — is owned by [E15](../foundations/E15-app-config.md). This spec states each rule's *default* and *config key* and defers the resolution mechanics there.
- **The quick-fixes** that repair these findings are owned by [F11](F11-code-actions.md). This spec notes which rules have a fix and what it does; F11 specifies the edit.
- **How the facts are extracted and resolved** — `Annotated[...]` columns, forward references, the user-defined base and its `MetaData` — is owned by [E30](../foundations/E30-extraction-and-indexing.md). F02 reads the resolved facts; it never re-parses.
- **Generic Python lints** — unused imports, line length, naming style — belong to the user's Python LSP (constitution P5). We fire only inside SQLAlchemy constructs.

## 3. Background & Rationale

The legacy SQLAlchemy LSP shipped a solid *correctness* core but stopped there. Meanwhile the Python ecosystem accumulated a body of SQLAlchemy wisdom — scattered across the 2.0 migration guide, flake8-sqlalchemy, ruff/flake8-bugbear, pylint-sqlalchemy, and the mypy plugin — that no single tool collects in one place. F02 is that collection, ported into our static-analysis engine and namespaced under `SQLA-`.

Each rule earns its place by catching a real, recurring mistake. A `default=[]` shared across every row. A `relationship` with two candidate foreign keys and no `foreign_keys=` to disambiguate. A `Column(String)` with no length on a dialect that requires one. None of these is a syntax error; all of them are bugs waiting for the right input.

> **Why default-on, with three exceptions.** [ADR-003](../decisions/ADR-003-comprehensive-lints-defaults.md) decided that best-practice lints are most useful when they're *on* — a lint you have to discover and enable is a lint nobody runs. So every rule defaults on, and you turn off the ones your team has decided against. There are three exceptions, off for two different reasons. Two are the hardest heuristics: `SQLA-H416` and `SQLA-H602` false-positive too readily to inflict on every project, so they ship off and you opt in. The third, `SQLA-I207`, is detected exactly — but requiring a `comment=` on every column is an opt-in style opinion that would fire on nearly every column of a typical schema, so it ships off as noise, not because the detection is shaky ([ADR-008](../decisions/ADR-008-default-off-missing-column-comment.md)). The policy and its rationale live in [E15 REQ-CFG-07](../foundations/E15-app-config.md#55-the-diagnostic-code-scheme), [ADR-003](../decisions/ADR-003-comprehensive-lints-defaults.md), and [ADR-008](../decisions/ADR-008-default-off-missing-column-comment.md).

## 4. Concepts & Definitions

These terms recur below; the glossary owns the canonical definitions.

- **Lint** — a best-practice finding (this spec's `SQLA-` codes), as opposed to a *correctness diagnostic* ([F01](F01-orm-correctness-diagnostics.md)). Both are diagnostics; the split is about severity of consequence.
- **Detectability** — how confidently the rule can fire from static facts alone. A rule that reads a single flag is *high*; one that must compare facts across files or infer intent is *heuristic* and marked as such, so a reader knows where false positives are likeliest.
- **Resolved base** — the project's own declarative base, resolved through the inheritance chain, carrying its `MetaData` and `naming_convention`. (Owned by [E30 REQ-EXTRACT-09](../foundations/E30-extraction-and-indexing.md#59-resolving-user-defined-base-classes); canonical definition in [glossary](../glossary.md).)
- **`target_dialect`** — the configured SQL dialect that gates dialect-sensitive rules like `SQLA-H206`. (Owned by [E15](../foundations/E15-app-config.md#54-the-key-reference).)
- **Diagnostic code** — the stable `SQLA-<SEV><CLASS><NN>` identifier. The severity letter records the *default*; the code never changes when a user re-levels it. (Constitution §4.2; [E15 REQ-CFG-06](../foundations/E15-app-config.md#55-the-diagnostic-code-scheme).)

## 5. Detailed Specification

This is the rule catalog. Each rule is a load-bearing requirement (`REQ-LINT-NN`) carrying its code, default severity, default on/off state, trigger, message, an example drawn from the `clean-blog` cast, and detectability notes. Where a quick-fix exists, the example shows the suggested fix and links [F11](F11-code-actions.md).

Two conventions run throughout. Every rule is **suppressible** with `# noqa: SQLA-<code>` and **configurable** in [E15](../foundations/E15-app-config.md) — stated once here, not repeated per rule. And every rule fires only on the *resolved* facts from [E07](../foundations/E07-data-model.md)/[E30](../foundations/E30-extraction-and-indexing.md): when a fact is `Unknown` or a reference is unresolvable, the rule stays silent rather than guessing (constitution P4).

### 5.1 The default-on-except-three policy

Before the rules, the policy that frames them. Every rule below defaults **on** — it fires unless you turn it off — except three: `SQLA-H416` (viewonly-write), `SQLA-H602` (association-proxy-misconfigured), and `SQLA-I207` (missing-column-comment), which default **off** and must be named in `diagnostics.select` to enable.

**REQ-LINT-01 — Every F02 rule defaults on except `SQLA-H416`, `SQLA-H602`, and `SQLA-I207`.**

The server enables 24 of the 27 F02 rules out of the box, so a project with no config still gets the bulk of the best-practice review. Three ship off, for two distinct reasons. `SQLA-H416` and `SQLA-H602` are the hardest heuristics — they read intent the syntax tree only hints at, so they false-positive often enough to annoy ([ADR-003](../decisions/ADR-003-comprehensive-lints-defaults.md)). `SQLA-I207` is detected exactly, but firing on nearly every column of a typical schema makes it pure noise unless a team opts in to documenting every column ([ADR-008](../decisions/ADR-008-default-off-missing-column-comment.md)). All three ship off and you opt in.

To turn a default-on rule off, name it in `diagnostics.ignore`. To turn an off-by-default rule on, name it in `diagnostics.select`. To re-level any rule, set `diagnostics.severity` — the code stays stable. The resolution order and these keys are owned by [E15 REQ-CFG-07](../foundations/E15-app-config.md#55-the-diagnostic-code-scheme) and [REQ-CFG-08](../foundations/E15-app-config.md#55-the-diagnostic-code-scheme).

### 5.2 Structure & constraints (`SQLA-1xx`)

These three rules sit alongside the F01 structure diagnostics (`SQLA-W101` missing-tablename, `SQLA-E102` duplicate-tablename, `SQLA-E103` duplicate-column, `SQLA-E105` table-arg-column-not-found, all owned by [F01](F01-orm-correctness-diagnostics.md)). F02 owns the three that are about good practice rather than correctness.

**REQ-LINT-02 — `SQLA-W104` missing-primary-key.**

- **Default:** warning · **on**.
- **Triggers when** a model declares a table (`__tablename__` or a resolved base) but none of its columns sets `primary_key=True`, and no `__table_args__` `PrimaryKeyConstraint` names one. SQLAlchemy refuses to map a class with no primary key, so this almost always means a forgotten `primary_key=True`.
- **Message:** `model \`Tag\` has no primary key; add primary_key=True to a column or a PrimaryKeyConstraint`.
- **Example.** `clean-blog`'s `Tag` with its PK dropped triggers the rule; the fix adds the flag back:

  ```python
  # models/tag.py  — trigger
  class Tag(Base):
      __tablename__ = "tags"
      id: Mapped[int] = mapped_column()          # no primary_key=True anywhere

  # models/tag.py  — suggested fix
      id: Mapped[int] = mapped_column(primary_key=True)
  ```

- **Detectability:** high. It reads the `primary_key` flag across the model's columns and its `__table_args__`. No quick-fix in F11 (the *which* column is the user's call), so the message guides rather than auto-edits.

**REQ-LINT-03 — `SQLA-H106` unnamed-constraint.**

- **Default:** hint · **on**.
- **Triggers when** a `__table_args__` entry is a bare constraint construct (`UniqueConstraint(...)`, `CheckConstraint(...)`, `Index(...)`) with no `name=`, and the resolved base sets no `naming_convention` that would name it automatically. An unnamed constraint gets a database-assigned name that Alembic can't reliably target in a later migration.
- **Message:** `constraint has no name; add name= or set a naming_convention so migrations can target it`.
- **Example.** `clean-blog` with a bare `UniqueConstraint` in `Post.__table_args__` and no convention:

  ```python
  # models/post.py  — trigger
  __table_args__ = (UniqueConstraint("title"),)          # no name=

  # models/post.py  — suggested fix (F11 names it)
  __table_args__ = (UniqueConstraint("title", name="uq_posts_title"),)
  ```

- **Detectability:** medium. It reads the `TableArg` kind and whether a `name=` is present, *and* the resolved base's `MetaData` for a `naming_convention` ([E30 REQ-EXTRACT-09](../foundations/E30-extraction-and-indexing.md#59-resolving-user-defined-base-classes)). When a convention names the constraint shape, the rule stays silent — so it interacts with `SQLA-H107` below. F11 offers a quick-fix that inserts a `name=`.

**REQ-LINT-04 — `SQLA-H107` no-naming-convention.**

- **Default:** hint · **on**.
- **Triggers when** the project's *resolved* declarative base sets no `naming_convention` on its `MetaData`. Without a convention, every implicit index and constraint gets a backend-assigned name, which makes migrations brittle and cross-dialect behavior inconsistent.
- **Message:** `declarative base sets no naming_convention; constraints will get unpredictable database-assigned names`.
- **Example.** The rule reads the base resolved by [E30](../foundations/E30-extraction-and-indexing.md), not the literal `DeclarativeBase`:

  ```python
  # models/base.py  — trigger
  class Base(DeclarativeBase):
      pass                                       # MetaData has no naming_convention

  # models/base.py  — suggested fix (F11 scaffolds the convention)
  class Base(DeclarativeBase):
      metadata = MetaData(naming_convention={
          "ix": "ix_%(column_0_label)s",
          "uq": "uq_%(table_name)s_%(column_0_name)s",
          "fk": "fk_%(table_name)s_%(column_0_name)s_%(referred_table_name)s",
          "pk": "pk_%(table_name)s",
      })
  ```

- **Detectability:** medium. It reads the convention **only from code** — the resolved base's `MetaData` ([E30 REQ-EXTRACT-09](../foundations/E30-extraction-and-indexing.md#59-resolving-user-defined-base-classes)), never from config. The rule fires **once per resolved base**, anchored on the base class, not once per model. Known limitation: a project that configures its `naming_convention` in Alembic's `env.py` rather than on the declarative base is *not* seen, so this rule will false-positive there — disable it in config or `# noqa` it (§10, OQ-LINT-2 in §15). F11 scaffolds a convention block.

### 5.3 Columns & types (`SQLA-2xx`)

These rules sit alongside F01's `SQLA-W201` nullable-not-optional (owned by [F01](F01-orm-correctness-diagnostics.md)). They read the `Column`, `ColumnArgs`, and `MappedType` facts from [E07](../foundations/E07-data-model.md).

**REQ-LINT-05 — `SQLA-H202` optional-without-nullable.**

- **Default:** hint · **on**.
- **Triggers when** a column's type is `Mapped[Optional[...]]` (or `Mapped[... | None]`) but `mapped_column(nullable=False)` is set explicitly. The annotation says "this can be `None`" while the column says "this can't" — they contradict, and one of them is a mistake.
- **Message:** `column \`bio\` is typed Optional but declared nullable=False; the annotation and the column disagree`.
- **Example.**

  ```python
  # models/user.py  — trigger
  bio: Mapped[Optional[str]] = mapped_column(nullable=False)

  # models/user.py  — suggested fix (drop the contradicting flag, or drop Optional)
  bio: Mapped[Optional[str]] = mapped_column()
  ```

- **Detectability:** high. It compares the `Optional` wrapper in `MappedType` against the explicit `nullable` in `ColumnArgs`; it fires only when `nullable` is *explicitly* `False`, never on inferred nullability. This is the mirror image of F01's `SQLA-W201` (a `Mapped[str]` declared `nullable=True`). F11 offers a quick-fix that drops the contradicting flag.

**REQ-LINT-06 — `SQLA-W203` mutable-default.**

- **Default:** warning · **on**.
- **Triggers when** a column's `default=` is a mutable literal — `[]`, `{}`, `set()`, or `dict()`/`list()` calls — passed directly rather than wrapped in a callable. A single mutable instance is then shared across every row that takes the default, so a mutation on one row silently mutates the default for all.
- **Message:** `mutable default \`[]\` is shared across rows; wrap it in a callable, e.g. default=list`.
- **Example.**

  ```python
  # models/post.py  — trigger
  tags_cache: Mapped[list] = mapped_column(JSON, default=[])

  # models/post.py  — suggested fix (F11 wraps it)
  tags_cache: Mapped[list] = mapped_column(JSON, default=list)
  ```

- **Detectability:** high. It reads `ColumnArgs.default` as source text ([E07 REQ-DATA-03](../foundations/E07-data-model.md#52-the-column-fact)) and matches the mutable-literal shapes. Because the default is stored as text, the match is syntactic and precise. F11 offers a quick-fix wrapping the literal in its callable (`[]` → `list`, `{}` → `dict`).

**REQ-LINT-07 — `SQLA-W204` default-and-server-default.**

- **Default:** warning · **on**.
- **Triggers when** a single `mapped_column(...)` sets both `default=` (a Python-side default applied by the ORM on insert) and `server_default=` (a database-side default). The two can disagree, and which one wins depends on whether the ORM or raw SQL did the insert — a subtle source of drift.
- **Message:** `column sets both default and server_default; they can diverge — keep one`.
- **Example.**

  ```python
  # models/post.py  — trigger
  created_at: Mapped[datetime] = mapped_column(
      default=datetime.utcnow, server_default=func.now())
  ```

- **Detectability:** high. It reads two flags on the same `mapped_column` call. No false-positive surface — both args are either present or not. No F11 quick-fix (which default to keep is a judgment call); the message states the trade-off.

**REQ-LINT-08 — `SQLA-H205` naive-datetime.**

- **Default:** hint · **on**.
- **Triggers when** a column's type is `DateTime` (the SQL type) without `timezone=True`, or the Python type is a bare `datetime`. A naive datetime stores no timezone, so the same value means different instants depending on the server's locale — a bug that only shows up in production, across regions.
- **Message:** `DateTime column \`created_at\` is timezone-naive; pass timezone=True`.
- **Example.**

  ```python
  # models/post.py  — trigger
  created_at: Mapped[datetime] = mapped_column(DateTime())

  # models/post.py  — suggested fix (F11 adds the flag)
  created_at: Mapped[datetime] = mapped_column(DateTime(timezone=True))
  ```

- **Detectability:** medium. It reads the `MappedType` (`SqlType { name: "DateTime", args }` or the `DateTime` scalar) and checks for `timezone=True` among the args. It can miss a custom datetime type alias it can't classify — in which case it stays silent (P4) rather than guessing. F11 offers a quick-fix adding `timezone=True`.

**REQ-LINT-09 — `SQLA-H206` unbounded-string (dialect-gated).**

- **Default:** hint · **on** — but only fires when `target_dialect` is set to a dialect that needs a length.
- **Triggers when** a column uses a bare `String` / `VARCHAR` with no length argument *and* `target_dialect` is a dialect where unbounded strings are an error or a footgun (notably `mysql`). On Postgres an unbounded `String` is fine, so the rule is gated on the configured dialect to avoid firing where it's harmless.
- **Message:** `String column \`title\` has no length; \`mysql\` requires one — use String(n)`.
- **Example.**

  ```python
  # models/post.py  — trigger (with target_dialect = "mysql")
  title: Mapped[str] = mapped_column(String())

  # models/post.py  — suggested fix
  title: Mapped[str] = mapped_column(String(255))
  ```

- **Detectability:** medium, and **dialect-gated**. It reads the `SqlType { name: "String", args }` and fires only when `args` is empty *and* `target_dialect` ([E15](../foundations/E15-app-config.md#54-the-key-reference)) names a length-requiring dialect. When `target_dialect` is unset, the rule stays silent rather than assume a backend (constitution P4; [E15 §9](../foundations/E15-app-config.md#9-edge-cases--failure-modes)). No F11 quick-fix (the length is the user's call).

**REQ-LINT-10 — `SQLA-I207` missing-column-comment *(default OFF)*.**

- **Default:** info · **off** — opt in via `diagnostics.select`.
- **Triggers when** a column has no `comment=` argument. A column comment lands in the database schema and in generated docs, so a team that documents its schema wants every column to carry one. This is the gentlest rule — info severity — because many projects legitimately skip comments.
- **Off by default** because it fires on nearly every column of an existing schema. That's noise from an opt-in style opinion, not a heuristic problem — the detection is trivial and exact. Opt in by naming `SQLA-I207` in `diagnostics.select`, or enable the `all` preset, when your team commits to documenting every column ([ADR-008](../decisions/ADR-008-default-off-missing-column-comment.md)).
- **Message:** `column \`email\` has no comment; add comment= to document it in the schema`.
- **Example.**

  ```python
  # models/user.py  — trigger
  email: Mapped[str] = mapped_column(String(255), unique=True)

  # models/user.py  — suggested fix
  email: Mapped[str] = mapped_column(
      String(255), unique=True, comment="Login email, unique per account")
  ```

- **Detectability:** high. It reads whether `comment=` is present on the `mapped_column` — a trivial, exact check. The rule is off by default for noise, not detectability ([ADR-008](../decisions/ADR-008-default-off-missing-column-comment.md)); when a team opts in, it fires reliably on every uncommented column. No F11 quick-fix (the comment text is the user's to write).

### 5.4 Foreign keys (`SQLA-3xx`)

These two rules sit alongside F01's foreign-key correctness core (`SQLA-E301` unknown-fk-table, `SQLA-E302` fk-column-not-found, `SQLA-W303` fk-type-mismatch, owned by [F01](F01-orm-correctness-diagnostics.md)). They read `ForeignKeyRef` and `Relationship` facts together.

**REQ-LINT-11 — `SQLA-W304` ambiguous-foreign-keys.**

- **Default:** warning · **on**.
- **Triggers when** a `relationship` links two models that have *two or more* foreign keys between them, and the relationship omits `foreign_keys=`. SQLAlchemy can't tell which FK the relationship rides on, and raises at mapper-configuration time — but only when the app boots, so the bug hides until then.
- **Message:** `relationship \`Post.author\` is ambiguous: \`posts\` has 2 FKs to \`users\`; add foreign_keys=`.
- **Example.** `clean-blog` with `Post` carrying both `author_id` and `editor_id` to `users`, and a relationship that doesn't disambiguate:

  ```python
  # models/post.py  — trigger
  author_id: Mapped[int] = mapped_column(ForeignKey("users.id"))
  editor_id: Mapped[int] = mapped_column(ForeignKey("users.id"))
  author: Mapped["User"] = relationship(back_populates="posts")   # which FK?

  # models/post.py  — suggested fix (F11 inserts foreign_keys=)
  author: Mapped["User"] = relationship(
      back_populates="posts", foreign_keys=[author_id])
  ```

- **Detectability:** **heuristic.** It counts the foreign keys between the relationship's two models (this model's columns plus, for cross-file pairs, the target's) and fires only when the count is ≥ 2 and `foreign_keys=` is absent. The cross-file count depends on the index ([E07](../foundations/E07-data-model.md)); when the target model is unresolved, the rule can't count and stays silent. F11 offers a quick-fix scaffolding `foreign_keys=` with the candidate columns.

**REQ-LINT-12 — `SQLA-W305` composite-fk-no-foreign-keys.**

- **Default:** warning · **on**.
- **Triggers when** a relationship spans a *composite* foreign key (two or more columns referencing a composite key on the target) and omits `foreign_keys=`. Composite keys can't be inferred as reliably as single-column ones, so SQLAlchemy needs the explicit list.
- **Message:** `relationship \`Order.line\` rides a composite FK; specify foreign_keys= explicitly`.
- **Example.**

  ```python
  # models/order.py  — trigger
  __table_args__ = (ForeignKeyConstraint(["a_id", "b_id"], ["parent.a", "parent.b"]),)
  parent: Mapped["Parent"] = relationship()        # composite, no foreign_keys=

  # models/order.py  — suggested fix
  parent: Mapped["Parent"] = relationship(foreign_keys=[a_id, b_id])
  ```

- **Detectability:** **heuristic.** It detects a `ForeignKeyConstraint` in `__table_args__` (a composite FK) or two single-column FKs to the same target table, then checks the relationship for `foreign_keys=`. Composite-FK detection from `__table_args__` is the harder half; when the constraint can't be parsed, the rule stays silent. F11 offers a quick-fix listing the composite columns.

### 5.5 Relationships (`SQLA-4xx`)

These six rules sit alongside F01's relationship correctness core (`SQLA-E401`, `W402`, `W403`, `W404`, `W405`, `H406`, `H407`, `W408`, `W409`, `H410`, all owned by [F01](F01-orm-correctness-diagnostics.md)). They read the `Relationship` fact ([E07 REQ-DATA-05](../foundations/E07-data-model.md#53-the-relationship-fact)).

**REQ-LINT-13 — `SQLA-W411` missing-remote-side.**

- **Default:** warning · **on**.
- **Triggers when** a *self-referential* relationship (target model equals the owning model, e.g. a threaded comment) omits `remote_side=`. Without it, SQLAlchemy can't tell the "parent" end from the "child" end of the self-join, and the relationship maps wrong.
- **Message:** `self-referential relationship \`Comment.parent\` needs remote_side= to orient the self-join`.
- **Example.** `clean-blog`'s threaded `Comment`:

  ```python
  # models/comment.py  — trigger
  parent_id: Mapped[Optional[int]] = mapped_column(ForeignKey("comments.id"))
  parent: Mapped["Comment"] = relationship()        # self-ref, no remote_side=

  # models/comment.py  — suggested fix
  parent: Mapped["Comment"] = relationship(remote_side=[id])
  ```

- **Detectability:** medium. It fires when `target_model` equals the owning model and `remote_side=` is absent. Self-reference is easy to detect from the resolved `target_model`; the rule is reliable. No F11 quick-fix (the remote-side column is the user's call), though the message names the pattern.

**REQ-LINT-14 — `SQLA-H412` secondary-with-data-columns.**

- **Default:** hint · **on**.
- **Triggers when** a many-to-many `relationship(secondary=...)` points at an association table that carries *its own data columns* beyond the two foreign keys. Once the join table holds data, the association-object pattern (a mapped class) is the maintainable choice; a bare `secondary=` can't expose those columns.
- **Message:** `association table \`post_tags\` has data columns; consider the association-object pattern`.
- **Example.** `clean-blog`'s `post_tags` grown a `created_at`:

  ```python
  # models/post.py  — trigger
  tags: Mapped[list["Tag"]] = relationship(secondary=post_tags)
  # post_tags = Table("post_tags", Base.metadata,
  #     Column("post_id", ForeignKey("posts.id")),
  #     Column("tag_id", ForeignKey("tags.id")),
  #     Column("created_at", DateTime))          # ← extra data column
  ```

- **Detectability:** **heuristic.** It resolves the `secondary` table name to its definition and counts non-FK columns. Resolving a `Table(...)` defined elsewhere is the hard part; when the table can't be resolved, the rule stays silent (P4). No F11 quick-fix (refactoring to an association object is too large to auto-apply).

**REQ-LINT-15 — `SQLA-W413` non-collection-mapped.**

- **Default:** warning · **on**.
- **Triggers when** a relationship's annotation is a bare scalar (`Mapped["Post"]`) but its wiring implies a collection — its `back_populates` counterpart is a collection, or `uselist=True` is set. The annotation and the cardinality disagree, so the typed attribute lies about its shape.
- **Message:** `relationship \`User.posts\` is typed as a scalar but is a collection; use Mapped[list[Post]]`.
- **Example.**

  ```python
  # models/user.py  — trigger
  posts: Mapped["Post"] = relationship(back_populates="author")   # should be a list

  # models/user.py  — suggested fix
  posts: Mapped[list["Post"]] = relationship(back_populates="author")
  ```

- **Detectability:** **heuristic.** It compares the annotation's `is_list` against the inferred cardinality (the counterpart's shape, or `uselist`). This overlaps F01's `SQLA-W404` uselist-mismatch — F01 owns the case where `uselist=` *explicitly* contradicts the annotation; F02's W413 owns the softer case where the *annotation* should have been a collection given the relationship's role. When the counterpart is unresolved, the rule stays silent. F11 offers a quick-fix wrapping the type in `list[...]`.

**REQ-LINT-16 — `SQLA-H414` lazy-select-scalar.**

- **Default:** hint · **on**.
- **Triggers when** a *scalar* (many-to-one) relationship uses the default `lazy="select"` (or omits `lazy`, which defaults to select). Each access then fires a separate query — the N+1 pattern. For a scalar parent, `lazy="joined"` or `lazy="selectin"` is usually the better default.
- **Message:** `scalar relationship \`Post.author\` uses lazy="select" (N+1 risk); consider lazy="joined"`.
- **Example.**

  ```python
  # models/post.py  — trigger
  author: Mapped["User"] = relationship(back_populates="posts")   # lazy defaults to select

  # models/post.py  — suggested direction (no auto-fix)
  author: Mapped["User"] = relationship(back_populates="posts", lazy="joined")
  ```

- **Detectability:** **heuristic** — this is a *performance opinion*, not a correctness fact. It reads `is_list == false` and `lazy` being unset or `"select"`. It's the kind of rule a team may well `ignore` wholesale; that's expected. No F11 quick-fix (the right loading strategy is workload-dependent).

**REQ-LINT-17 — `SQLA-H415` lazy-joined-collection.**

- **Default:** hint · **on**.
- **Triggers when** a *collection* (one-to-many or many-to-many) relationship uses `lazy="joined"`. A joined eager load on a collection multiplies rows (a cartesian-ish blow-up) and is almost always the wrong choice; `lazy="selectin"` is the recommended collection strategy.
- **Message:** `collection relationship \`User.posts\` uses lazy="joined" (row blow-up); prefer lazy="selectin"`.
- **Example.**

  ```python
  # models/user.py  — trigger
  posts: Mapped[list["Post"]] = relationship(
      back_populates="author", lazy="joined")     # joined on a collection

  # models/user.py  — suggested direction
  posts: Mapped[list["Post"]] = relationship(
      back_populates="author", lazy="selectin")
  ```

- **Detectability:** medium. It reads `is_list == true` and `lazy == "joined"` — both are explicit facts, so it's more reliable than H414, but it's still a performance opinion a team may tune. No F11 quick-fix.

**REQ-LINT-18 — `SQLA-H416` viewonly-write *(default OFF)*.**

- **Default:** hint · **off** — opt in via `diagnostics.select`.
- **Triggers when** a relationship is declared `viewonly=True` but the surrounding code appears to *write* through it (an append, an assignment to the collection). Writes through a `viewonly` relationship are silently ignored at flush, so this is a real bug — but detecting the write requires following data flow the static facts barely capture, which is why it ships **off**.
- **Message:** `relationship \`User.all_posts\` is viewonly but is written to; the write will be ignored at flush`.
- **Example.**

  ```python
  # somewhere in app code  — trigger (only with SQLA-H416 enabled)
  user.all_posts.append(post)        # all_posts is relationship(viewonly=True)
  ```

- **Detectability:** **hard heuristic — the reason it's off by default** ([ADR-003](../decisions/ADR-003-comprehensive-lints-defaults.md)). It must find a write site (`.append(...)`, an assignment) that targets a `viewonly` relationship, which means correlating a relationship fact with an arbitrary expression elsewhere in the file. False positives are easy (a same-named attribute on a different object), so the rule is conservative and off by default. No F11 quick-fix.

### 5.6 Modernization & conventions (`SQLA-5xx`)

These rules nudge code toward the SQLAlchemy 2.0 idiom and the project's conventions. This class is wholly F02's.

Three of these rules flag code that is *on its way out* of SQLAlchemy, not merely unfashionable — a `backref`, a `declarative_base()` call, a `session.query(...)`. LSP has a tag for exactly that, and we use it so the editor can show the reader at a glance.

**REQ-LINT-19b — The three modernization rules carry the `Deprecated` diagnostic tag.**

`SQLA-W501` (legacy-backref), `SQLA-W502` (legacy-declarative-base), and `SQLA-I503` (legacy-query-api) each set the `Deprecated` tag on the published `Diagnostic`, per the tag model in [E16](../foundations/E16-conventions.md). An editor that honors the tag renders these findings **struck through** — the legacy construct is shown crossed out in the source, so the reader sees it is deprecated before they even read the message. The tag travels in `Diagnostic.tags`; it is additive to the code, severity, and message, and never replaces them. No other F02 rule sets `Deprecated` — these three are the only ones flagging a construct SQLAlchemy itself is retiring. (The `Unnecessary` tag, the other LSP tag in the [E16](../foundations/E16-conventions.md) model, is used by no F02 rule.)

**REQ-LINT-19 — `SQLA-W501` legacy-backref.**

- **Default:** warning · **on**.
- **Triggers when** a `relationship(...)` uses `backref=` (or `backref(...)`) instead of the modern `back_populates` pair. `backref` is the legacy one-sided way to declare both ends; 2.0 prefers the explicit `back_populates` on each side, which the type checker and this server can both see.
- **Message:** `\`backref\` is legacy; declare an explicit back_populates pair instead`.
- **Example.** The [backref-deprecated](../foundations/E17-testing.md#backref-deprecated) fixture:

  ```python
  # models/user.py  — trigger
  posts: Mapped[list["Post"]] = relationship("Post", backref="author")

  # models/user.py + models/post.py  — suggested fix (F11 rewrites both sides)
  # user.py:
  posts: Mapped[list["Post"]] = relationship(back_populates="author")
  # post.py:
  author: Mapped["User"] = relationship(back_populates="posts")
  ```

- **Detectability:** high. It reads whether `backref=` is present on the relationship call. F11 offers a quick-fix that rewrites the `backref` into a `back_populates` pair, adding the counterpart attribute on the target model.
- **Tag:** `Deprecated` (REQ-LINT-19b; [E16](../foundations/E16-conventions.md)) — the editor renders the finding struck through.

**REQ-LINT-20 — `SQLA-W502` legacy-declarative-base.**

- **Default:** warning · **on**.
- **Triggers when** the base is created with the legacy `Base = declarative_base()` call rather than the 2.0 `class Base(DeclarativeBase)`. The function form predates typed mappings and doesn't play as well with `Mapped[...]` inference.
- **Message:** `\`declarative_base()\` is legacy; subclass DeclarativeBase instead`.
- **Example.** The [legacy-declarative-base](../foundations/E17-testing.md#legacy-declarative-base) fixture:

  ```python
  # models/base.py  — trigger
  Base = declarative_base()

  # models/base.py  — suggested fix (F11 rewrites it)
  class Base(DeclarativeBase):
      pass
  ```

- **Detectability:** high. It detects the `declarative_base()` call at module scope. F11 offers a quick-fix that rewrites it to the class form.
- **Tag:** `Deprecated` (REQ-LINT-19b; [E16](../foundations/E16-conventions.md)) — the editor renders the finding struck through.

**REQ-LINT-21 — `SQLA-I503` legacy-query-api.**

- **Default:** info · **on**.
- **Triggers when** code uses the legacy `Query` API — `session.query(User)` or `Model.query` — instead of the 2.0 `select()` construct. The `Query` API still works but is in long-term legacy mode; `select()` is the future-proof form.
- **Message:** `\`session.query(...)\` is the legacy API; prefer select(...) with session.execute()`.
- **Example.**

  ```python
  # app/repo.py  — trigger
  users = session.query(User).filter(User.email == email).all()

  # app/repo.py  — suggested direction
  users = session.execute(select(User).where(User.email == email)).scalars().all()
  ```

- **Detectability:** medium. It matches `session.query(...)` and `<Model>.query` access patterns syntactically. Because the rewrite changes the surrounding call chain substantially, there's no safe F11 quick-fix; info severity reflects that it's a nudge, not a demand.
- **Tag:** `Deprecated` (REQ-LINT-19b; [E16](../foundations/E16-conventions.md)) — the editor renders the finding struck through.

**REQ-LINT-22 — `SQLA-W504` missing-mapped-annotation.**

- **Default:** warning · **on**.
- **Triggers when** a column is assigned `mapped_column(...)` (or the legacy `Column(...)`) with **no** `Mapped[...]` annotation. In 2.0 the annotation is what gives the attribute a static type; without it, the type checker sees `Any` and this server can't infer nullability or cardinality.
- **Message:** `column \`name\` has no Mapped[...] annotation; the type checker can't see its type`.
- **Example.** The [missing-mapped-annotation](../foundations/E17-testing.md#missing-mapped-annotation) fixture:

  ```python
  # models/user.py  — trigger
  name = mapped_column(String(120))            # no Mapped[...] annotation

  # models/user.py  — suggested fix (F11 infers and adds the annotation)
  name: Mapped[str] = mapped_column(String(120))
  ```

- **Detectability:** high. It reads whether a `mapped_column`-valued assignment carries a `Mapped[...]` annotation ([E30 REQ-EXTRACT-05](../foundations/E30-extraction-and-indexing.md#54-the-class-body-walk)). F11 offers a quick-fix that infers the type from the SQL type (`String` → `str`, `Integer` → `int`) and adds the `Mapped[...]` annotation.

**REQ-LINT-23 — `SQLA-I505` import-alias.**

- **Default:** info · **on**.
- **Triggers when** `sqlalchemy` is imported under a non-conventional alias — `import sqlalchemy as sql` instead of the community-standard `as sa`. A consistent alias makes code grep-able and matches the docs.
- **Message:** `\`import sqlalchemy as sql\`; the convention is \`as sa\``.
- **Example.** The [import-alias](../foundations/E17-testing.md#import-alias) fixture:

  ```python
  # models/tag.py  — trigger
  import sqlalchemy as sql

  # models/tag.py  — suggested fix (F11 renames the alias and its uses)
  import sqlalchemy as sa
  ```

- **Detectability:** high. It reads the module's import statements for a `sqlalchemy` import aliased to anything other than `sa`. F11 offers a quick-fix that renames the alias and every use of it in the file.

**REQ-LINT-24 — `SQLA-I506` missing-repr.**

- **Default:** info · **on**.
- **Triggers when** a model defines no `__repr__` and doesn't inherit one from a mixin or `MappedAsDataclass`. A model with no `__repr__` prints as an opaque `<User object at 0x...>` in logs and debuggers, which slows debugging.
- **Message:** `model \`User\` has no __repr__; add one for readable logs and debugging`.
- **Example.**

  ```python
  # models/user.py  — trigger
  class User(Base):
      __tablename__ = "users"
      id: Mapped[int] = mapped_column(primary_key=True)
      # ... no __repr__

  # models/user.py  — suggested direction
      def __repr__(self) -> str:
          return f"User(id={self.id!r})"
  ```

- **Detectability:** medium. It checks the model's body (and resolved bases/mixins) for a `__repr__` definition or a `MappedAsDataclass` base that generates one. The mixin/base case depends on [E30](../foundations/E30-extraction-and-indexing.md) base resolution. Info severity; no F11 quick-fix (the repr fields are the user's call).

### 5.7 ORM extensions (`SQLA-6xx`)

These three rules cover the SQLAlchemy extension constructs — hybrid properties, association proxies, and validators. They read constructs that decorate methods, which is the hardest class to analyze statically. This class is wholly F02's.

**REQ-LINT-25 — `SQLA-H601` hybrid-without-expression.**

- **Default:** hint · **on**.
- **Triggers when** a method decorated `@hybrid_property` is used in a query-like context but defines no `@<name>.expression` (or `@<name>.inplace.expression`) companion. Without the expression form, the hybrid works on instances but falls back to Python-level evaluation in queries — often not what the author intended.
- **Message:** `hybrid_property \`full_name\` has no .expression companion; queries will use Python-level fallback`.
- **Example.**

  ```python
  # models/user.py  — trigger
  @hybrid_property
  def full_name(self) -> str:
      return f"{self.first} {self.last}"
  # ... no @full_name.expression defined

  # models/user.py  — suggested direction
  @full_name.expression
  def full_name(cls):
      return cls.first + " " + cls.last
  ```

- **Detectability:** **heuristic.** It finds `@hybrid_property`-decorated methods and checks for a matching `.expression` companion in the same class. Detecting whether the hybrid is *used in a query* is beyond the static facts, so the rule fires on the *absence of the companion* alone — which can over-report for hybrids only ever used on instances. On by default but a candidate to `ignore`. No F11 quick-fix.

**REQ-LINT-26 — `SQLA-H602` association-proxy-misconfigured *(default OFF)*.**

- **Default:** hint · **off** — opt in via `diagnostics.select`.
- **Triggers when** an `association_proxy("rel", "attr")` names a target relationship or attribute that the server can't resolve — the named relationship doesn't exist on the model, or the named attribute doesn't exist on the proxied target. A misconfigured proxy fails only when first accessed at runtime.
- **Message:** `association_proxy targets \`tags.name\` but \`Tag\` has no attribute \`name\``.
- **Example.**

  ```python
  # models/post.py  — trigger (only with SQLA-H602 enabled)
  tag_names = association_proxy("tags", "naem")     # typo: Tag has `name`
  ```

- **Detectability:** **hard heuristic — the reason it's off by default** ([ADR-003](../decisions/ADR-003-comprehensive-lints-defaults.md)). It must resolve the proxy's first argument to a relationship on this model, then the second to an attribute on *that relationship's target* — a two-hop cross-file resolution that fails quietly whenever either hop is unresolved, producing false positives on perfectly valid proxies. Ships **off**; opt in when your project leans on association proxies. No F11 quick-fix.

**REQ-LINT-27 — `SQLA-H603` validates-without-include-removes.**

- **Default:** hint · **on**.
- **Triggers when** a method decorated `@validates("some_collection")` validates a *collection* attribute but omits `include_removes=True`. Without it, the validator runs on appends but not on removes, so a validator meant to guard collection membership silently skips removals.
- **Message:** `@validates on collection \`tags\` omits include_removes=True; removals won't be validated`.
- **Example.**

  ```python
  # models/post.py  — trigger
  @validates("tags")
  def check_tag(self, key, tag):
      ...                                       # validates appends only

  # models/post.py  — suggested fix
  @validates("tags", include_removes=True)
  def check_tag(self, key, tag, is_remove):
      ...
  ```

- **Detectability:** **heuristic.** It finds `@validates(...)`-decorated methods, resolves the validated attribute name, and fires only when that attribute is a *collection* relationship and `include_removes=True` is absent. The collection check depends on resolving the named attribute to a `Relationship` with `is_list == true`; when the attribute is a scalar column, the rule correctly stays silent. No F11 quick-fix (the validator signature also changes).

## 6. Visualizations

The 27 F02 rules at a glance, by class — their code, default severity, default state, and whether [F11](F11-code-actions.md) ships a quick-fix. Three rows are off by default. Two — `SQLA-H416` and `SQLA-H602` — are the hardest heuristics ([ADR-003](../decisions/ADR-003-comprehensive-lints-defaults.md)); the third, `SQLA-I207`, is off for noise/opinion rather than detectability, so its detectability column stays `high` ([ADR-008](../decisions/ADR-008-default-off-missing-column-comment.md)).

| Code | Rule | Sev | Default | Quick-fix? | Detectability |
|---|---|---|---|---|---|
| `SQLA-W104` | missing-primary-key | W | on | — | high |
| `SQLA-H106` | unnamed-constraint | H | on | ✅ | medium |
| `SQLA-H107` | no-naming-convention | H | on | ✅ | medium |
| `SQLA-H202` | optional-without-nullable | H | on | ✅ | high |
| `SQLA-W203` | mutable-default | W | on | ✅ | high |
| `SQLA-W204` | default-and-server-default | W | on | — | high |
| `SQLA-H205` | naive-datetime | H | on | ✅ | medium |
| `SQLA-H206` | unbounded-string | H | on (dialect-gated) | — | medium |
| `SQLA-I207` | missing-column-comment | I | **off** | — | high |
| `SQLA-W304` | ambiguous-foreign-keys | W | on | ✅ | heuristic |
| `SQLA-W305` | composite-fk-no-foreign-keys | W | on | ✅ | heuristic |
| `SQLA-W411` | missing-remote-side | W | on | — | medium |
| `SQLA-H412` | secondary-with-data-columns | H | on | — | heuristic |
| `SQLA-W413` | non-collection-mapped | W | on | ✅ | heuristic |
| `SQLA-H414` | lazy-select-scalar | H | on | — | heuristic |
| `SQLA-H415` | lazy-joined-collection | H | on | — | medium |
| `SQLA-H416` | viewonly-write | H | **off** | — | hard heuristic |
| `SQLA-W501` | legacy-backref | W | on | ✅ | high |
| `SQLA-W502` | legacy-declarative-base | W | on | ✅ | high |
| `SQLA-I503` | legacy-query-api | I | on | — | medium |
| `SQLA-W504` | missing-mapped-annotation | W | on | ✅ | high |
| `SQLA-I505` | import-alias | I | on | ✅ | high |
| `SQLA-I506` | missing-repr | I | on | — | medium |
| `SQLA-H601` | hybrid-without-expression | H | on | — | heuristic |
| `SQLA-H602` | association-proxy-misconfigured | H | **off** | — | hard heuristic |
| `SQLA-H603` | validates-without-include-removes | H | on | — | heuristic |

## 9. Examples & Use Cases

Walk a realistic case through the `clean-blog` cast. A developer adds a `tags_cache` column to `Post` to memoize computed tags, writes it the quick way, and never sets a length on the project's Postgres-targeted `String` columns:

```python
# models/post.py
class Post(Base):
    __tablename__ = "posts"
    id: Mapped[int] = mapped_column(primary_key=True)
    created_at: Mapped[datetime] = mapped_column(DateTime())       # SQLA-H205
    tags_cache: Mapped[list] = mapped_column(JSON, default=[])     # SQLA-W203
    author: Mapped["User"] = relationship(back_populates="posts")  # SQLA-H414
```

On save, the server publishes three F02 findings, each with its range and code: `SQLA-H205` on the naive `DateTime`, `SQLA-W203` on the shared mutable default, and `SQLA-H414` on the scalar relationship's default lazy loading. The first two carry F11 quick-fixes (`timezone=True`; wrap `[]` in `list`); the third is a performance nudge with no auto-fix.

The team decides the lazy-loading hints are noise for their workload and the naive-datetime rule should block merges. Both go in `pyproject.toml` ([E15 §6](../foundations/E15-app-config.md#6-examples--use-cases)):

```toml
# pyproject.toml
[tool.sqlalchemy-lsp.diagnostics]
ignore = ["SQLA-H414", "SQLA-H415"]
severity = { "SQLA-H205" = "error" }
```

Now `SQLA-H414`/`H415` stop firing entirely, and `SQLA-H205` reports at error severity — but its code stays `SQLA-H205`, so an existing `# noqa: SQLA-H205` on a deliberately-naive column keeps working ([E15 REQ-CFG-06](../foundations/E15-app-config.md#55-the-diagnostic-code-scheme)).

## 10. Edge Cases & Failure Modes

- **Unresolvable fact → silence.** Any rule whose trigger depends on a resolved type, base, or cross-file model stays silent when that fact is `Unknown` or unresolved — never a guess (constitution P4; [E07 §10](../foundations/E07-data-model.md#10-edge-cases--failure-modes)).
- **Dialect unset → `SQLA-H206` silent.** With no `target_dialect`, the dialect-gated rule never fires, even on a bare `String()` ([E15 §9](../foundations/E15-app-config.md#9-edge-cases--failure-modes)).
- **Off-by-default rule in `ignore` but never `select` → no-op.** Ignoring `SQLA-H416`/`SQLA-H602`/`SQLA-I207` while they're already off is harmless, not an error ([E15 §9](../foundations/E15-app-config.md#9-edge-cases--failure-modes)).
- **W413 vs F01's W404 overlap.** When `uselist=` *explicitly* contradicts the annotation, F01's `SQLA-W404` owns it; F02's `SQLA-W413` fires only on the softer annotation-should-be-a-collection case. The two never double-report the same site.
- **H106 silenced by a naming convention.** When the resolved base sets a `naming_convention` that names the constraint shape, `SQLA-H106` (unnamed-constraint) stays silent — the convention names it automatically.
- **Convention configured in Alembic `env.py` → `SQLA-H106`/`SQLA-H107` false-positive.** We read the `naming_convention` only from the resolved base's `MetaData` in code (v0.2; the config key was dropped). A project that sets its convention in `env.py` instead is not seen, so both rules may fire incorrectly. The escape hatch is to disable the rule (`diagnostics.ignore`) or `# noqa` it. Tracked as OQ-LINT-2 (§15).
- **Partial / mid-keystroke file.** A class with `ERROR` nodes lints its well-formed columns and skips the broken one; no crash (constitution P3; [E16](../foundations/E16-conventions.md)).
- **A `# noqa` that matched no F02 finding → `SQLA-W901`.** An unused suppression is itself reported ([E15 REQ-CFG-10](../foundations/E15-app-config.md#56-inline-suppression-with--noqa)).

## 11. Testing

Every F02 rule is proven by a unit test that fires it on its named broken fixture and confirms it stays silent on `clean-blog`, plus the cross-file and config behaviors above. The strategy, categories, tools, and the fixtures registry live in [E17-testing](../foundations/E17-testing.md); this section maps each `REQ-LINT-NN` to its test and links the fixtures.

### 11.1 Scope & coverage

Target: **100% of this feature's behavior is covered.** Every `REQ-LINT-NN` maps to at least one test; every edge case in §10 has a test. A rule is not done until its broken fixture exists in [E17](../foundations/E17-testing.md) and is linked here ([E17 REQ-TST-04](../foundations/E17-testing.md#6-conventions)). See the policy in [E17 §2](../foundations/E17-testing.md#2-coverage-policy).

> **Note on fixtures.** [E17](../foundations/E17-testing.md#5-fixtures-registry) defines the per-code broken variants. Several F02 codes already have named variants there ([missing-primary-key](../foundations/E17-testing.md#missing-primary-key), [unnamed-constraint](../foundations/E17-testing.md#unnamed-constraint), [no-naming-convention](../foundations/E17-testing.md#no-naming-convention), [mutable-default](../foundations/E17-testing.md#mutable-default), [naive-datetime](../foundations/E17-testing.md#naive-datetime), [unbounded-string](../foundations/E17-testing.md#unbounded-string), [ambiguous-foreign-keys](../foundations/E17-testing.md#ambiguous-foreign-keys), [unique-missing-one-to-one](../foundations/E17-testing.md#unique-missing-one-to-one), [backref-deprecated](../foundations/E17-testing.md#backref-deprecated), [legacy-declarative-base](../foundations/E17-testing.md#legacy-declarative-base), [missing-mapped-annotation](../foundations/E17-testing.md#missing-mapped-annotation), [import-alias](../foundations/E17-testing.md#import-alias)). The remaining F02 codes each need a new `clean-blog` variant added to the [E17 registry](../foundations/E17-testing.md#5-fixtures-registry) — named in the table below as the variant slug a test would link, per [REQ-TST-04](../foundations/E17-testing.md#6-conventions).

### 11.2 Test plan

Each row is one rule (or behavior) under test, asserting the exact `SQLA-` code and range on its broken fixture and zero findings on `clean-blog`.

| Behavior / scenario | Type | Fixtures | Verifies |
|---|---|---|---|
| All 27 rules on by default; `SQLA-H416`/`H602`/`I207` off | unit | [clean-blog](../foundations/E17-testing.md#clean-blog) | REQ-LINT-01 |
| Missing primary key fires `SQLA-W104` | unit | [missing-primary-key](../foundations/E17-testing.md#missing-primary-key) | REQ-LINT-02 |
| Bare constraint with no convention fires `SQLA-H106` | unit | [unnamed-constraint](../foundations/E17-testing.md#unnamed-constraint) | REQ-LINT-03 |
| Resolved base with no convention fires `SQLA-H107` once | unit | [no-naming-convention](../foundations/E17-testing.md#no-naming-convention) | REQ-LINT-04 |
| `Optional` + `nullable=False` fires `SQLA-H202` | unit | `#optional-without-nullable` | REQ-LINT-05 |
| Mutable `default=[]` fires `SQLA-W203` | unit | [mutable-default](../foundations/E17-testing.md#mutable-default) | REQ-LINT-06 |
| Both `default` and `server_default` fire `SQLA-W204` | unit | `#default-and-server-default` | REQ-LINT-07 |
| Naive `DateTime` fires `SQLA-H205` | unit | [naive-datetime](../foundations/E17-testing.md#naive-datetime) | REQ-LINT-08 |
| Bare `String()` fires `SQLA-H206` only when dialect set; silent when unset | unit | [unbounded-string](../foundations/E17-testing.md#unbounded-string) | REQ-LINT-09 |
| Column with no `comment=` fires `SQLA-I207` only when enabled | unit | `#missing-column-comment` | REQ-LINT-10 |
| Two FKs to one target + no `foreign_keys=` fires `SQLA-W304` | integration | [ambiguous-foreign-keys](../foundations/E17-testing.md#ambiguous-foreign-keys) | REQ-LINT-11 |
| Composite FK + no `foreign_keys=` fires `SQLA-W305` | integration | `#composite-fk-no-foreign-keys` | REQ-LINT-12 |
| Self-ref relationship with no `remote_side=` fires `SQLA-W411` | unit | `#missing-remote-side` | REQ-LINT-13 |
| `secondary` table with data columns fires `SQLA-H412` | integration | `#secondary-with-data-columns` | REQ-LINT-14 |
| Scalar annotation on a collection fires `SQLA-W413`; no double-report with W404 | unit | `#non-collection-mapped` | REQ-LINT-15 |
| Scalar relationship defaulting `lazy="select"` fires `SQLA-H414` | unit | `#lazy-select-scalar` | REQ-LINT-16 |
| Collection relationship `lazy="joined"` fires `SQLA-H415` | unit | `#lazy-joined-collection` | REQ-LINT-17 |
| Write through a `viewonly` relationship fires `SQLA-H416` only when enabled | unit | `#viewonly-write` | REQ-LINT-18 |
| `backref=` fires `SQLA-W501` | unit | [backref-deprecated](../foundations/E17-testing.md#backref-deprecated) | REQ-LINT-19 |
| `declarative_base()` fires `SQLA-W502` | unit | [legacy-declarative-base](../foundations/E17-testing.md#legacy-declarative-base) | REQ-LINT-20 |
| `session.query(...)` fires `SQLA-I503` | unit | `#legacy-query-api` | REQ-LINT-21 |
| `mapped_column` with no `Mapped[...]` fires `SQLA-W504` | unit | [missing-mapped-annotation](../foundations/E17-testing.md#missing-mapped-annotation) | REQ-LINT-22 |
| `import sqlalchemy as sql` fires `SQLA-I505` | unit | [import-alias](../foundations/E17-testing.md#import-alias) | REQ-LINT-23 |
| Model with no `__repr__` fires `SQLA-I506` | unit | `#missing-repr` | REQ-LINT-24 |
| `@hybrid_property` with no `.expression` fires `SQLA-H601` | unit | `#hybrid-without-expression` | REQ-LINT-25 |
| Misconfigured `association_proxy` fires `SQLA-H602` only when enabled | integration | `#association-proxy-misconfigured` | REQ-LINT-26 |
| `@validates` on a collection without `include_removes` fires `SQLA-H603` | unit | `#validates-without-include-removes` | REQ-LINT-27 |
| Unresolvable fact / unset dialect / partial file → silence | unit | [clean-blog](../foundations/E17-testing.md#clean-blog) variants | §10 edge cases |
| `# noqa: SQLA-<code>` suppresses; unused → `SQLA-W901` | integration | per-code variant | §10, [E15 REQ-CFG-09/10](../foundations/E15-app-config.md#56-inline-suppression-with--noqa) |
| CLI/server parity: `check` and server emit identical F02 findings | integration | per-code variants | [E17 REQ-TST-05](../foundations/E17-testing.md#6-conventions) |

### 11.3 Fixtures

All fixtures are the shared per-code broken variants in the [E17 fixtures registry](../foundations/E17-testing.md#5-fixtures-registry) — F02 defines none of its own. The existing variants are linked in §11.2; the new variants F02 requires (slugs `#optional-without-nullable`, `#default-and-server-default`, `#missing-column-comment`, `#composite-fk-no-foreign-keys`, `#missing-remote-side`, `#secondary-with-data-columns`, `#non-collection-mapped`, `#lazy-select-scalar`, `#lazy-joined-collection`, `#viewonly-write`, `#legacy-query-api`, `#missing-repr`, `#hybrid-without-expression`, `#association-proxy-misconfigured`, `#validates-without-include-removes`) are added to the E17 registry as part of landing this feature ([E17 REQ-TST-04](../foundations/E17-testing.md#6-conventions)).

### 11.4 Requirement coverage

Every load-bearing requirement maps to a test — this table is the proof.

| Requirement | Covered by |
|---|---|
| REQ-LINT-01 | default-on-except-three policy test |
| REQ-LINT-02 | `SQLA-W104` missing-primary-key test |
| REQ-LINT-03 | `SQLA-H106` unnamed-constraint test |
| REQ-LINT-04 | `SQLA-H107` no-naming-convention test |
| REQ-LINT-05 | `SQLA-H202` optional-without-nullable test |
| REQ-LINT-06 | `SQLA-W203` mutable-default test |
| REQ-LINT-07 | `SQLA-W204` default-and-server-default test |
| REQ-LINT-08 | `SQLA-H205` naive-datetime test |
| REQ-LINT-09 | `SQLA-H206` dialect-gated unbounded-string test |
| REQ-LINT-10 | `SQLA-I207` missing-column-comment opt-in test |
| REQ-LINT-11 | `SQLA-W304` ambiguous-foreign-keys test |
| REQ-LINT-12 | `SQLA-W305` composite-fk-no-foreign-keys test |
| REQ-LINT-13 | `SQLA-W411` missing-remote-side test |
| REQ-LINT-14 | `SQLA-H412` secondary-with-data-columns test |
| REQ-LINT-15 | `SQLA-W413` non-collection-mapped test (+ W404 non-overlap) |
| REQ-LINT-16 | `SQLA-H414` lazy-select-scalar test |
| REQ-LINT-17 | `SQLA-H415` lazy-joined-collection test |
| REQ-LINT-18 | `SQLA-H416` viewonly-write opt-in test |
| REQ-LINT-19 | `SQLA-W501` legacy-backref test |
| REQ-LINT-20 | `SQLA-W502` legacy-declarative-base test |
| REQ-LINT-21 | `SQLA-I503` legacy-query-api test |
| REQ-LINT-22 | `SQLA-W504` missing-mapped-annotation test |
| REQ-LINT-23 | `SQLA-I505` import-alias test |
| REQ-LINT-24 | `SQLA-I506` missing-repr test |
| REQ-LINT-25 | `SQLA-H601` hybrid-without-expression test |
| REQ-LINT-26 | `SQLA-H602` association-proxy-misconfigured opt-in test |
| REQ-LINT-27 | `SQLA-H603` validates-without-include-removes test |

## 12. End-to-End Test Plan

Driven by `pytest-lsp` over stdio against the built binary, these journeys prove each lint fires with its exact code and range on its fixture, that config and `# noqa` toggle it, and that the editor and the CLI agree. The harness, isolation, and the shared protocol-conformance journeys live in [E29-e2e-testing](../foundations/E29-e2e-testing.md); F02 lists only its own scenarios.

### 12.1 Coverage target

**100% of the feature's scope, end to end** — every lint fixture publishes its expected code, plus the config-override, suppression, and parity paths. See the policy in [E29 §2](../foundations/E29-e2e-testing.md#2-coverage-policy).

### 12.2 Scenarios

| # | Journey | Path | Expected outcome |
|---|---|---|---|
| E2E-01 | Open `clean-blog` | happy | Zero F02 findings published |
| E2E-02 | Open each default-on lint fixture | happy | Each publishes exactly its expected `SQLA-` code at the right range |
| E2E-03 | Open the `SQLA-H416` / `SQLA-H602` / `SQLA-I207` fixtures with defaults | happy | No finding — all three ship off |
| E2E-04 | Same three fixtures with the codes in `diagnostics.select` | happy | Each now publishes its code |
| E2E-05 | Open the `SQLA-H206` fixture with no `target_dialect` | error | No finding — dialect-gated rule stays silent |
| E2E-06 | Same fixture with `target_dialect = "mysql"` | happy | `SQLA-H206` published |
| E2E-07 | A lint code listed in `diagnostics.ignore` | happy | That code no longer published; others unaffected |
| E2E-08 | A lint code re-leveled via `diagnostics.severity` | happy | Published at the new severity; code unchanged |
| E2E-09 | `# noqa: SQLA-W203` on the triggering line | happy | `SQLA-W203` suppressed on that line only |
| E2E-10 | Bare `# noqa` on a lint line | happy | All F02 findings on that line suppressed |
| E2E-11 | `# noqa: SQLA-W203` on a line that never triggers it | error | `SQLA-W901 unused-noqa` published |
| E2E-12 | Edit a triggering line to its suggested fix | happy | The finding clears via an explicit (re)publish |
| E2E-13 | `check` over a lint fixture vs the server's publish | happy | Identical code/file/range (CLI/server parity) |
| E2E-14 | Half-typed model with `ERROR` nodes plus a clean lintable column | error | Clean column's lint fires; no crash |
| E2E-15 | Open a `SQLA-W501` / `SQLA-W502` / `SQLA-I503` fixture | happy | The published diagnostic carries the `Deprecated` tag in `Diagnostic.tags` (REQ-LINT-19b) |

### 12.3 Acceptance criteria & Definition of Done

The §12.2 scenarios, written Given/When/Then, are this feature's acceptance criteria:

| # | Given | When | Then |
|---|---|---|---|
| AC-01 | The lint-clean `clean-blog` workspace | the client opens its model files | the server publishes zero F02 findings |
| AC-02 | A broken variant for a default-on lint | the client opens it | the server publishes exactly that lint's `SQLA-` code at the offending range |
| AC-03 | The `SQLA-H416` fixture with default config | the client opens it | no finding is published (rule ships off) |
| AC-04 | `SQLA-H416` named in `diagnostics.select` | the client opens the same fixture | the rule now publishes its finding |
| AC-05 | The `unbounded-string` fixture and no `target_dialect` | the client opens it | `SQLA-H206` does not fire |
| AC-06 | A lint code in `diagnostics.ignore` | the client opens its fixture | that finding is absent and unrelated findings remain |
| AC-07 | A `# noqa: SQLA-<code>` on the triggering line | the client opens the file | that code is suppressed on that line only |
| AC-08 | A `# noqa` that matches no finding | the client opens the file | `SQLA-W901 unused-noqa` is published |
| AC-09 | A broken lint fixture | `sqlalchemy-lsp check` runs and the server publishes for the same workspace | the two emit identical code, file, and range |
| AC-10 | A `legacy-backref`, `legacy-declarative-base`, or `legacy-query-api` fixture | the client opens it | the published diagnostic carries the `Deprecated` tag so the editor strikes it through |

**Definition of Done:** every `REQ-LINT-NN` has a passing test (§11.4), every acceptance scenario above passes, the per-code fixtures exist in [E17](../foundations/E17-testing.md), CLI/server parity holds ([E17 REQ-TST-05](../foundations/E17-testing.md#6-conventions)), and the §13.1 security posture is verified.

## 13. Non-Functional Requirements

### 13.1 Security & Privacy

F02 inherits the suite-wide security envelope; it adds no new trust boundary.

- **Access & authorization** — none. F02 is a read-only static analysis of local workspace files; it crosses no trust boundary and exposes no surface to authorize.
- **Input & validation** — the untrusted input is the user's own Python source, read through tree-sitter, never imported or executed (constitution P1). A lint rule that hits an unexpected node shape returns no finding rather than crashing (P3; [E16](../foundations/E16-conventions.md)). A malformed config value resolves to a config warning, not a failure ([E15 REQ-CFG-08](../foundations/E15-app-config.md#55-the-diagnostic-code-scheme)).
- **Data sensitivity** — none. F02 handles no PII, secrets, or regulated data; findings name model/column identifiers from the user's own source. It opens no network connection and sends no telemetry; logs go to stderr or `log_file` only, never stdout ([E16](../foundations/E16-conventions.md)).
- **Baseline** — stays within the suite's inherited envelope (constitution §4.6): local files only, no code execution, no network, no secrets.

Accessibility (§13.2) is N/A — F02 is a pure-data diagnostics feature with no rendered UI; the editor renders findings, and severity is carried in the code and message text, never color alone (constitution §4.6). Performance & Scale (§13.4) is covered once in [E01 §8](../foundations/E01-architecture.md) against the `large-workspace` fixture and not restated here. Observability (§13.5) is the suite-wide `tracing` default. Permissions (§13.3) and Rollout (§14) are N/A (constitution §4.6).

## 15. Open Questions & Decisions

- **OQ-LINT-1** — `SQLA-H414`/`H415` (lazy-loading strategy) encode a performance *opinion*, not a fact. Should they ship at `info` rather than `hint`, or be grouped under a single toggle so a team can disable all loading-strategy hints at once? Deferred; the per-rule `ignore` already covers the common case.
- **OQ-LINT-2** — `SQLA-H106`/`SQLA-H107` read the `naming_convention` only from the resolved base's `MetaData` in code (the config key was dropped in v0.2). A project that configures its convention in Alembic's `env.py` is not seen and gets a false positive. Should the extractor learn to read a convention from `env.py`, or is the `ignore`/`# noqa` escape hatch enough? Deferred — most 2.0 projects set the convention on the base.
- **Resolved — default-on-except-three.** Whether best-practice lints default on was settled by [ADR-003](../decisions/ADR-003-comprehensive-lints-defaults.md): on, except the two hardest heuristics `SQLA-H416` and `SQLA-H602`. [ADR-008](../decisions/ADR-008-default-off-missing-column-comment.md) later added `SQLA-I207` to the off-by-default set — off for noise/opinion, not heuristic instability — making it three.
- **Resolved — F01/F02 split.** Correctness diagnostics (`SQLA-1xx`–`4xx` core) live in [F01](F01-orm-correctness-diagnostics.md); best-practice lints live here. The two never duplicate a code, and overlapping cases (W413 vs W404) have a clear owner (§10).

## 16. Cross-References

- **Depends on:** [constitution](../constitution.md) — P4 (silence on unresolvable input) and P5 (companion to the Python LSP) govern when a lint may fire; [E07-data-model](../foundations/E07-data-model.md) — the `Column`/`Relationship`/`MappedType` facts every rule reads; [E30-extraction-and-indexing](../foundations/E30-extraction-and-indexing.md) — resolves the base + `MetaData` (`SQLA-H107`), `Annotated` columns, and forward refs the rules depend on; [E16-conventions](../foundations/E16-conventions.md) — the error/resilience contract a rule obeys; [E15-app-config](../foundations/E15-app-config.md) — the `select`/`ignore`/`severity` keys, `target_dialect` gating, and `# noqa` suppression that toggle every rule.
- **Related:** [F01-orm-correctness-diagnostics](F01-orm-correctness-diagnostics.md) — the correctness core this spec sits beside and cross-references but never duplicates; [F11-code-actions](F11-code-actions.md) — the quick-fixes that repair the fixable lints (byte-identical to `check --fix`); [E17-testing](../foundations/E17-testing.md) — the per-code fixtures registry and the CLI/server parity rule; [E29-e2e-testing](../foundations/E29-e2e-testing.md) — the harness and shared protocol-conformance journeys; [ADR-003](../decisions/ADR-003-comprehensive-lints-defaults.md) — the default-on-except-three decision, as amended by [ADR-008](../decisions/ADR-008-default-off-missing-column-comment.md) — `SQLA-I207` off by default for noise/opinion.

## 17. Changelog

- **2026-06-18** — v0.4: `SQLA-I207` (missing-column-comment) now ships off by default ([ADR-008](../decisions/ADR-008-default-off-missing-column-comment.md)) — off for noise/opinion, not heuristic instability; reframed the default-off set from two rules to three.
- **2026-06-18** — v0.3: The three modernization rules `SQLA-W501` (legacy-backref), `SQLA-W502` (legacy-declarative-base), and `SQLA-I503` (legacy-query-api) now carry the LSP `Deprecated` diagnostic tag ([E16](../foundations/E16-conventions.md)), so editors render them struck through. Added REQ-LINT-19b and the three per-rule **Tag** notes (§5.6), an E2E scenario (E2E-15) and acceptance criterion (AC-10) asserting the published diagnostic includes the tag.
- **2026-06-18** — v0.2: `SQLA-H106`/`SQLA-H107` now read the `naming_convention` **only from the resolved base's `MetaData`** in code; the `naming_convention` config key was dropped from [E15](../foundations/E15-app-config.md). Added the `env.py`-configured-convention false-positive edge case (§10) and OQ-LINT-2 (§15).
- **2026-06-17** — Initial draft. Specified the 27 best-practice lints across structure (`SQLA-W104`/`H106`/`H107`), columns & types (`H202`/`W203`/`W204`/`H205`/`H206`/`I207`), foreign keys (`W304`/`W305`), relationships (`W411`/`H412`/`W413`/`H414`/`H415`/`H416`), modernization (`W501`/`W502`/`I503`/`W504`/`I505`/`I506`), and ORM extensions (`H601`/`H602`/`H603`), each with a `REQ-LINT-NN`, default severity/state, trigger, message, example, and detectability notes. Recorded the default-on-except-two policy ([ADR-003](../decisions/ADR-003-comprehensive-lints-defaults.md)), the F01 cross-reference boundary, the per-rule test plan with §11.4 coverage, and the end-to-end journeys including config override, `# noqa` suppression, and CLI/server parity.
