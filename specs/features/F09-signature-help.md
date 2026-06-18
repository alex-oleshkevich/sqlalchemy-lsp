# F09 — Signature Help

> **Status:** Draft
>
> **Version:** 0.1   ·   **Last updated:** 2026-06-17
>
> **Purpose:** Signatures for the SQLAlchemy and Alembic call sites — `ForeignKey`, `relationship`, `mapped_column`, the Alembic `op.*` operations, and the model constructor `User(…)` synthesized from the model's columns — with the active parameter highlighted as you type. It fires only inside these constructs and stays silent everywhere else.
>
> **Depends on:** [constitution](../constitution.md), [E07-data-model](../foundations/E07-data-model.md), [E30-extraction-and-indexing](../foundations/E30-extraction-and-indexing.md)   ·   **Related:** [F03-completions](F03-completions.md), [E01-architecture](../foundations/E01-architecture.md), [E17-testing](../foundations/E17-testing.md), [E29-e2e-testing](../foundations/E29-e2e-testing.md), [ADR-007](../decisions/ADR-007-companion-to-python-lsp.md)

> Requirement tag: **SIG**

---

## 1. Purpose & Scope

When your cursor sits inside a SQLAlchemy or Alembic call, the server shows the call's signature — its parameters, a one-line description, and which parameter you're currently on. For the fixed-shape constructs (`ForeignKey`, `relationship`, `mapped_column`, the `op.*` operations) the signature is a curated template. For a model constructor (`User(…)`) it is *synthesized on the fly* from the model's own columns — their names, types, nullability, and defaults — so the popover mirrors the class you actually wrote.

This spec covers:

- **`ForeignKey(target)`** signature — the `"table.column"` argument.
- **`relationship(...)`** signature — target plus the wiring kwargs.
- **`mapped_column(...)`** signature — the column-configuration kwargs.
- **Alembic `op.*`** signatures — one per supported operation.
- **Model-constructor signatures** — `User(…)` synthesized from the model's columns (name, type, nullable → optional, default).
- **Active-parameter highlighting** — the parameter index computed from the cursor's position among the arguments.
- **The companion gate** — outside a recognized call, no signature.

## 2. Non-Goals / Out of Scope

- **Generic Python signature help** — the signatures of arbitrary functions, stdlib calls, the user's own helpers — owned by the user's Python LSP, per [P5](../constitution.md) and [ADR-007](../decisions/ADR-007-companion-to-python-lsp.md).
- **The completion items offered at these same sites** — owned by [F03-completions](F03-completions.md). F09 shows *the shape of the call*; F03 offers *what to type into it*.
- **How model/column facts are extracted** — owned by [E30](../foundations/E30-extraction-and-indexing.md) and [E07](../foundations/E07-data-model.md). Signature help only reads the index.

## 3. Background & Rationale

SQLAlchemy's most-used constructors carry a lot of optional keyword arguments, and remembering the exact name and order is friction. A signature popover removes it: as you type `relationship(`, you see `back_populates`, `lazy`, `cascade`, and the rest, with the current one bolded. The fixed constructs are easy — their parameter lists are stable, so we curate them once.

The interesting case is the **model constructor**. `User(…)` has no hand-written signature anywhere; SQLAlchemy generates `__init__` from the mapped columns at runtime. Since we never run user code ([P1](../constitution.md)), we *reconstruct* that signature statically from the indexed model: each column becomes a keyword parameter, typed from its `Mapped[...]` annotation, marked optional when the column is nullable or has a default. This pairs exactly with [F03](F03-completions.md), which completes those same keywords at the same cursor — one feature shows the shape, the other fills it in.

Everything here is gated by the **companion principle** ([P5](../constitution.md), [ADR-007](../decisions/ADR-007-companion-to-python-lsp.md)): we answer only inside a recognized construct, leaving every other call to the Python LSP.

## 4. Concepts & Definitions

These terms are canonical across the suite; the glossary owns the full definitions.

- **Companion LSP** — the general Python language server we run alongside; it owns signatures for generic Python. (Canonical definition in [glossary](../glossary.md).)
- **Enclosing call** — the innermost `call` node whose argument list contains the cursor, resolved to its dotted name; the base name (last segment) selects the signature.
- **Active parameter** — the zero-based index of the parameter the cursor is currently entering, computed by counting top-level commas before the cursor inside the argument list.
- **Synthesized signature** — a `SignatureInformation` built at request time from an indexed model's columns, rather than from a curated template.

## 5. Detailed Specification

Signature help is a pure function of the workspace state and a position. It finds the enclosing call from the cached parse tree, resolves the base name, computes the active parameter, and returns one `SignatureInformation` — or `None`. It never re-parses and never opens a file off disk ([E07 REQ-DATA-13](../foundations/E07-data-model.md)).

### 5.1 The enclosing-call gate

The first step is to find a recognized call around the cursor. If there isn't one, there is no signature.

**REQ-SIG-01 — Signature help fires only inside a recognized SQLAlchemy/Alembic call.**

The handler finds the innermost call whose arguments contain the cursor and resolves its dotted name. If the name is an `op.*` operation, a `ForeignKey`/`relationship`/`mapped_column` call, or resolves to an indexed model, it returns that signature. Otherwise — an arbitrary function, no enclosing call at all — it returns `None` and contributes nothing, leaving the Python LSP in charge (the companion gate, [P5](../constitution.md)).

**REQ-SIG-02 — The base name selects the signature.**

The dotted name is reduced to its last segment: `sa.ForeignKey` → `ForeignKey`, `orm.relationship` → `relationship`. `op.*` is detected by the `op.` prefix and dispatched by operation name. A name matching an indexed model routes to the synthesized constructor signature (§5.6).

### 5.2 Active-parameter computation

The popover bolds the parameter you're on; the index is computed from where the cursor sits among the arguments.

**REQ-SIG-03 — The active parameter is the count of top-level commas before the cursor.**

The handler reads the call's argument-list node, takes the text from the list's start to the cursor, and counts commas that are not nested inside parentheses, brackets, or string literals. That count is the active-parameter index. When the cursor is on a continuation line below the argument list's start, the count is taken over the current line's text before the cursor. An index past the last parameter clamps to the last one (variadic and trailing-kwargs cases stay highlighted on the final parameter).

### 5.3 `ForeignKey` signature

`ForeignKey("table.column")` takes a single string target; the signature documents its format.

**REQ-SIG-04 — `ForeignKey` shows a single `target: str` parameter.**

The label is `ForeignKey(target: str)`, the documentation explains the `"table.column"` format, and the one parameter `target: str` is documented as the target column. The active parameter is always 0.

### 5.4 `relationship` signature

`relationship(...)` shows the positional target and the wiring kwargs in order.

**REQ-SIG-05 — `relationship` shows the target and its keyword arguments.**

The label is `relationship(target, *, back_populates=, lazy=, uselist=, secondary=, cascade=, order_by=, foreign_keys=, viewonly=)`. Each parameter carries a one-line description (e.g. `lazy=` lists the loader strategies; `cascade=` lists the cascade rules). The active parameter follows §5.3's comma count, so highlighting tracks the kwarg you're entering.

### 5.5 `mapped_column` signature

`mapped_column(...)` shows the column-configuration parameters.

**REQ-SIG-06 — `mapped_column` shows its configuration parameters.**

The label is `mapped_column(type_=, *, primary_key=, nullable=, unique=, index=, default=, server_default=, name=, ForeignKey())`. Each parameter is documented in one line (`primary_key=` notes the default of `False`, `name=` is the explicit DB column name, and so on). Active parameter follows the comma count.

### 5.6 Synthesized model-constructor signature

This is the feature's centerpiece: `User(…)` gets a signature built from the model's columns, since SQLAlchemy generates `__init__` at runtime and we read only source.

**REQ-SIG-07 — A model constructor's signature is synthesized from the model's columns.**

When the enclosing call's base name resolves to an indexed model, the handler builds one parameter per column, in declaration order. Each parameter's label is `name: type`, taken from the column's Python attribute name and its `MappedType` rendered for display (`int`, `Optional[str]`, `String(120)`). The signature label is `Model(col1: t1, col2: t2, …)`. The documentation names the model and notes that the constructor is synthesized from its mapped columns.

**REQ-SIG-08 — Nullable or defaulted columns are marked optional; required columns are not.**

A column that is `nullable` or carries a `default`/`server_default` is rendered as an optional parameter — `name: Optional[str] = …` in the label and "optional" in its documentation. A non-nullable column with no default is a required parameter. This mirrors how SQLAlchemy's generated `__init__` treats them and tells the author which keywords they must supply. Nullability and default come straight from the column fact's `ColumnArgs` ([E07 REQ-DATA-03](../foundations/E07-data-model.md)).

**REQ-SIG-09 — Constructor active-parameter highlighting tracks keyword arguments.**

The constructor is normally called with keywords (`User(full_name="…", email="…")`). The active parameter is computed positionally from the comma count (§5.3); when the cursor is inside a `name=` keyword whose name matches a parameter, the handler highlights *that* parameter instead, so editing `email=` mid-call bolds the `email` parameter rather than the third positional slot. This is the highlight that pairs with the [F03](F03-completions.md) keyword completion at the same site.

### 5.7 Alembic `op.*` signatures

Each supported migration operation has a curated signature.

**REQ-SIG-10 — Each Alembic operation shows its own signature.**

For an `op.*` call, the handler dispatches by operation name and returns that operation's signature: `add_column(table_name, column, schema=None)`, `drop_column(table_name, column_name, schema=None)`, `alter_column(table_name, column_name, nullable=, type_=, server_default=, new_column_name=)`, `create_table(table_name, *columns, **kw)`, `drop_table(table_name, schema=None)`, `create_index(index_name, table_name, columns, unique=False, schema=None)`, `drop_index(index_name, table_name=None, schema=None)`, `create_unique_constraint(constraint_name, table_name, columns, schema=None)`, `drop_constraint(constraint_name, table_name, type_=None, schema=None)`, `create_foreign_key(constraint_name, source_table, referent_table, local_cols, remote_cols)`, `rename_table(old_table_name, new_table_name, schema=None)`, `execute(sqltext)`, `bulk_insert(table, rows, multiinsert=True)`. Each parameter carries a one-line description. An operation the server doesn't model returns `None`. Active parameter follows §5.3.

### 5.8 The negative contract

Like completions, signature help is silent outside the constructs it knows.

**REQ-SIG-11 — A plain-Python call shows no signature.**

If the enclosing call is an arbitrary function — or there is no enclosing call — the handler returns `None`. It never returns an empty-but-present signature that could suppress the Python LSP, and never guesses a signature for a call it doesn't recognize. The companion principle, made testable; §12 pins it with an explicit negative scenario.

### 5.9 Trigger characters

The server advertises signature-help triggers so the editor re-requests at the right moments.

**REQ-SIG-12 — The advertised trigger characters are `(` and `,`, with re-trigger on `)`.**

`(` opens the popover when you enter a call; `,` advances the active parameter as you move between arguments; `)` re-triggers so the popover dismisses or updates when a nested call closes.

## 6. UI Mockups

These sketch the signature popover as the editor renders it from our `SignatureInformation`. The popover floats above the cursor; the active parameter is shown bolded with `«…»` markers (the editor renders it bold/underlined). What we control is the label, the parameter spans, the active index, and the documentation lines.

### 6.1 Synthesized model-constructor signature — at `User(…)`

Appears when you type inside a model constructor; the parameters come from the model's columns. Pairs with the keyword completion in [F03 §6.5](F03-completions.md).

```
    u = User(full_name="Ada", │)
       ╭──────────────────────────────────────────────────────────────╮
       │ User(id: int, full_name: str, email: str,                      │
       │      created_at: Optional[datetime] = …)                       │
       │                  «email: str»                                  │  ◀ active param
       │ ─────────────────────────────────────────────────────────────│
       │ Synthesized from User's mapped columns. email is required.     │
       ╰──────────────────────────────────────────────────────────────╯
```

States: active param tracks the cursor · optional params shown with `= …` · documentation names the model.

### 6.2 `relationship(…)` signature

Appears inside a `relationship(` call; highlights the kwarg you're entering.

```
    posts: Mapped[list["Post"]] = relationship(back_populates="posts", │)
       ╭──────────────────────────────────────────────────────────────╮
       │ relationship(target, *, back_populates=, lazy=, uselist=,      │
       │      secondary=, cascade=, order_by=, foreign_keys=, viewonly=)│
       │                                         «lazy=»                │  ◀ active param
       │ ─────────────────────────────────────────────────────────────│
       │ Loading strategy: select, joined, subquery, selectin, raise…   │
       ╰──────────────────────────────────────────────────────────────╯
```

States: active param advances per comma · documentation line follows the active param.

### 6.3 `mapped_column(…)` signature

Appears inside a `mapped_column(` call.

```
    id: Mapped[int] = mapped_column(primary_key=True, │)
       ╭──────────────────────────────────────────────────────────────╮
       │ mapped_column(type_=, *, primary_key=, nullable=, unique=,     │
       │      index=, default=, server_default=, name=, ForeignKey())   │
       │                                          «nullable=»           │  ◀ active param
       │ ─────────────────────────────────────────────────────────────│
       │ Allow NULL values.                                             │
       ╰──────────────────────────────────────────────────────────────╯
```

States: active param per comma.

### 6.4 `ForeignKey(…)` signature

Appears inside a `ForeignKey(` call; the single target parameter is always active.

```
    mapped_column(ForeignKey("users.id"│))
       ╭──────────────────────────────────────────────────╮
       │ ForeignKey(target: str)                            │
       │            «target: str»                           │  ◀ active param
       │ ───────────────────────────────────────────────── │
       │ Target column in "table.column" format.            │
       ╰──────────────────────────────────────────────────╯
```

States: single static parameter, always active.

### 6.5 Alembic `op.*` signature

Appears inside an `op.*` call in a migration.

```
    op.add_column("posts", │)
       ╭──────────────────────────────────────────────────╮
       │ op.add_column(table_name, column, schema=None)     │
       │                          «column»                  │  ◀ active param
       │ ───────────────────────────────────────────────── │
       │ sa.Column() object to add.                         │
       ╰──────────────────────────────────────────────────╯
```

States: active param per comma · unsupported operation → no popover.

## 7. Visualizations

This decision tree shows how the enclosing call selects a signature, and where unmatched paths land on "no signature" — the companion gate.

```mermaid
%%{init: {'theme': 'base', 'themeVariables': {'fontSize': '14px'}}}%%
flowchart TB
    classDef ctx fill:#CCE5FF,stroke:#4A90D9,color:#004085
    classDef out fill:#D4EDDA,stroke:#28A745,color:#155724
    classDef none fill:#F8D7DA,stroke:#DC3545,color:#721C24

    start["signatureHelp at position"]:::ctx
    call{"enclosing call?"}:::ctx
    op{"op.* operation?"}:::ctx
    base{"base name?"}:::ctx
    model{"indexed model?"}:::ctx

    opsig["curated op signature"]:::out
    fixed["ForeignKey / relationship /
    mapped_column signature"]:::out
    synth["synthesized Model(...)
    from columns"]:::out
    nothing["return None
    (Python LSP owns it)"]:::none

    start --> call
    call -->|no| nothing
    call -->|yes| op
    op -->|yes| opsig
    op -->|no| base
    base -->|recognized| fixed
    base -->|unrecognized| model
    model -->|yes| synth
    model -->|no| nothing

    linkStyle 0,2,4 stroke:#4A90D9,stroke-width:2px
    linkStyle 3,5,7 stroke:#28A745,stroke-width:2px
    linkStyle 1,6 stroke:#DC3545,stroke-width:2px
```

## 8. Data Shapes

Signature help crosses the wire as the LSP `SignatureHelp` object. The shape below is one synthesized constructor signature; the `parameters` spans index into the `label`, and `activeParameter` selects the highlighted one.

```json
{
  "signatures": [
    {
      "label": "User(id: int, full_name: str, email: str, created_at: Optional[datetime] = …)",
      "documentation": "Synthesized from User's mapped columns.",
      "parameters": [
        { "label": "id: int", "documentation": "primary key" },
        { "label": "full_name: str", "documentation": "required" },
        { "label": "email: str", "documentation": "required" },
        { "label": "created_at: Optional[datetime] = …", "documentation": "optional" }
      ],
      "activeParameter": 2
    }
  ],
  "activeSignature": 0,
  "activeParameter": 2
}
```

`activeParameter` is the §5.2 comma count (or the matched keyword index, REQ-SIG-09). Each `parameters[i].label` is a substring of the signature `label`, so the editor can bold the exact span.

## 9. Examples & Use Cases

Take the `clean-blog` cast. You are constructing a `User`:

```python
# scripts/seed.py
u = User(full_name="Ada Lovelace", email="ada@example.com", )   # ← cursor after the last comma
```

`User` has columns `id` (PK), `full_name` (`String(120)`, required), `email` (required), `created_at` (`Optional[datetime]`, defaulted). The handler reads the indexed `User` model and synthesizes:

```
User(id: int, full_name: str, email: str, created_at: Optional[datetime] = …)
```

`id` is the primary key, `full_name` and `email` are required (non-nullable, no default), `created_at` is optional (it has a `default`). The cursor sits after the second comma, so `created_at` is highlighted — telling you the one remaining keyword and that it's optional. Meanwhile [F03](F03-completions.md) is offering `created_at=` (and the relationship keywords) at the same cursor. Type the shape, fill the value.

Now `relationship`: editing `Post.author = relationship(back_populates="posts", )`, the popover shows the full kwarg list with `lazy=` highlighted (the next slot), and its documentation lists the loader strategies — so you know `selectin` and `joined` exist without leaving the line.

And the negative case. In a plain helper:

```python
send_email(to=u.email, )   # ← no SQLAlchemy signature here
```

`send_email` is an arbitrary function, so the handler returns `None` and the user's Python LSP shows its signature (REQ-SIG-11).

## 10. Edge Cases & Failure Modes

- **`User(…)` where `User` resolves to no indexed model** (a same-named non-model class) → `None`; we only synthesize for indexed models ([P4](../constitution.md)).
- **A model with zero columns** → a signature with an empty parameter list and the model name in its documentation; still a valid popover.
- **A column whose type is `MappedType::Unknown`** → its parameter renders with the verbatim source type and no optional/required claim beyond what `nullable`/`default` say (we don't guess, [P4](../constitution.md)).
- **Cursor past the last argument** (trailing comma, variadic op) → active parameter clamps to the last parameter (REQ-SIG-03).
- **Multi-line call** → comma count is taken over the current line before the cursor when the cursor is below the argument-list start row.
- **Unparsed / `ERROR` tree** → if no enclosing call can be found, return `None`; never crash ([P3](../constitution.md)).
- **Unsupported `op.*` operation** (a custom op or a typo) → `None`.
- **Arbitrary function shadowing a known name** (a local `def relationship(...)`) → still shows the relationship signature; we resolve by base name and accept the rare false positive.

## 11. Testing

Signature help is tested as a pure function — given a fixture workspace and a cursor position, assert the signature label, parameter spans, active index, and documentation — plus E2E over the protocol.

### 11.1 Scope & coverage

Target: **100% of this feature's behavior is covered.** Every `REQ-SIG-NN` maps to at least one test; every popover state (§6) and edge case (§10) has a test, including the negative companion-gate case. See the policy in [E17-testing](../foundations/E17-testing.md#2-coverage-policy).

### 11.2 Test plan

Each row is a behavior under test. Shared fixtures link to the registry in [E17](../foundations/E17-testing.md#5-fixtures-registry).

| Behavior / scenario | Type | Fixtures | Verifies |
|---|---|---|---|
| Enclosing-call gate routes recognized vs unrecognized | unit | [clean-blog](../foundations/E17-testing.md#5-fixtures-registry) | REQ-SIG-01, REQ-SIG-02 |
| Active parameter = top-level comma count; nested ignored | unit | [clean-blog](../foundations/E17-testing.md#5-fixtures-registry) | REQ-SIG-03 |
| `ForeignKey` signature, single param active | unit | [clean-blog](../foundations/E17-testing.md#5-fixtures-registry) | REQ-SIG-04 |
| `relationship` signature, kwargs documented, active tracks | unit | [clean-blog](../foundations/E17-testing.md#5-fixtures-registry) | REQ-SIG-05 |
| `mapped_column` signature, params documented | unit | [clean-blog](../foundations/E17-testing.md#5-fixtures-registry) | REQ-SIG-06 |
| `User(…)` synthesized from columns, in declaration order | unit | [clean-blog](../foundations/E17-testing.md#5-fixtures-registry) | REQ-SIG-07 |
| nullable/defaulted → optional; required otherwise | unit | [clean-blog](../foundations/E17-testing.md#5-fixtures-registry) | REQ-SIG-08 |
| constructor highlight follows keyword `email=` | unit | [clean-blog](../foundations/E17-testing.md#5-fixtures-registry) | REQ-SIG-09 |
| each `op.*` operation returns its signature | unit | [clean-blog](../foundations/E17-testing.md#5-fixtures-registry) | REQ-SIG-10 |
| plain-Python call → `None` | unit | [clean-blog](../foundations/E17-testing.md#5-fixtures-registry) | REQ-SIG-11 |
| trigger characters advertised at `initialize` | integration | [clean-blog](../foundations/E17-testing.md#5-fixtures-registry) | REQ-SIG-12 |
| zero-column model → empty-param signature | unit | [clean-blog](../foundations/E17-testing.md#5-fixtures-registry) | REQ-SIG-07 |
| cursor past last arg → active clamps to last | unit | [clean-blog](../foundations/E17-testing.md#5-fixtures-registry) | REQ-SIG-03 |
| `Unknown`-typed column param renders verbatim | unit | [clean-blog](../foundations/E17-testing.md#5-fixtures-registry) | REQ-SIG-08 |
| unsupported `op.*` operation → `None` | unit | [clean-blog](../foundations/E17-testing.md#5-fixtures-registry) | REQ-SIG-10, REQ-SIG-11 |
| unparsed tree near cursor → no crash | unit | [clean-blog](../foundations/E17-testing.md#5-fixtures-registry) | REQ-SIG-01, REQ-SIG-11 |

### 11.3 Fixtures

Reusable fixtures live in [E17-testing](../foundations/E17-testing.md#5-fixtures-registry) — linked above. This feature reuses the baseline `clean-blog` workspace for every case; it adds no broken-variant fixtures, since signature help produces no findings. A small zero-column model and an `Unknown`-typed column are defined as test-local mutations of `clean-blog` for the edge cases.

### 11.4 Requirement coverage

Every load-bearing requirement maps to a test — this table is the proof.

| Requirement | Covered by |
|---|---|
| REQ-SIG-01 | Enclosing-call gate; unparsed-tree |
| REQ-SIG-02 | Enclosing-call gate routing |
| REQ-SIG-03 | Active-param comma count; past-last clamp |
| REQ-SIG-04 | `ForeignKey` signature |
| REQ-SIG-05 | `relationship` signature |
| REQ-SIG-06 | `mapped_column` signature |
| REQ-SIG-07 | constructor synthesis; zero-column model |
| REQ-SIG-08 | optional/required marking; `Unknown` type |
| REQ-SIG-09 | constructor keyword highlight |
| REQ-SIG-10 | each `op.*` signature; unsupported → None |
| REQ-SIG-11 | plain-Python → None |
| REQ-SIG-12 | trigger characters advertised |

## 12. End-to-End Test Plan

The E2E suite drives the built binary over stdio with `pytest-lsp`, requesting signature help at fixed positions in the `clean-blog` workspace and asserting the signature and active parameter — including the negative plain-Python case.

### 12.1 Coverage target

**100% of the feature's scope, end to end** — every recognized call site returns its signature with the right active parameter, and the plain-Python call returns none. See the policy in [E29-e2e-testing](../foundations/E29-e2e-testing.md#2-coverage-policy).

### 12.2 Scenarios

| # | Journey | Path | Expected outcome |
|---|---|---|---|
| E2E-01 | Signature inside `ForeignKey("users.id"` | happy | Label `ForeignKey(target: str)`, active param 0 |
| E2E-02 | Signature inside `relationship(back_populates="posts", ` | happy | relationship label, active param highlights the next kwarg |
| E2E-03 | Signature inside `mapped_column(primary_key=True, ` | happy | mapped_column label, active param advances |
| E2E-04 | Signature at `User(full_name="…", email="…", ` | happy | Synthesized `User(...)` label, `created_at` optional, highlighted |
| E2E-05 | Signature at `User(` with cursor on `email=` keyword | happy | The `email` parameter is the active one |
| E2E-06 | Signature inside `op.add_column("posts", ` | happy | `op.add_column(...)` label, `column` active |
| E2E-07 | Signature inside `op.create_index(` | happy | `op.create_index(...)` label, first param active |
| E2E-08 | **Signature inside a plain-Python call** (`send_email(to=…, `) | error/negative | Server returns no signature; Python LSP unaffected |
| E2E-09 | Signature inside an unsupported `op.frobnicate(` | error | Server returns nothing |
| E2E-10 | Signature at `User(` where `User` is a non-model class | error | Server returns nothing |
| E2E-11 | `initialize` advertises signature trigger characters | happy | `triggerCharacters` includes `(` and `,` |
| E2E-12 | Signature request in a file with a syntax error near cursor | error | No crash; nothing or the matching signature |

### 12.3 Acceptance criteria & Definition of Done

The §12.2 scenarios, written Given/When/Then, are this feature's acceptance criteria:

| # | Given | When | Then |
|---|---|---|---|
| AC-01 | The `clean-blog` workspace is indexed | I request signature help inside `relationship(` | I see the relationship signature with the current kwarg highlighted |
| AC-02 | `User` has required and optional columns | I request signature help inside `User(…)` | I see a synthesized signature marking each column required or optional |
| AC-03 | My cursor is on the `email=` keyword in `User(…)` | I request signature help | The `email` parameter is highlighted as active |
| AC-04 | My cursor is inside an ordinary Python call | I request signature help | The server contributes no signature, leaving the Python LSP in charge |

**Definition of Done:** every `REQ-SIG-NN` has a passing test (§11.4), every acceptance scenario above passes, CLI/server parity is N/A (signature help is an editor-only surface), and every enabled non-functional concern (§13) is verified.

## 13. Non-Functional Requirements

### 13.1 Security & Privacy

- **Access & authorization** — none; signature help reads only the in-memory index built from local workspace files. No trust boundary is crossed.
- **Input & validation** — the only input is a source position and the cached tree/source; both are already-parsed local data. The handler tolerates `ERROR` nodes and out-of-range positions without crashing ([P3](../constitution.md)).
- **Data sensitivity** — none; labels and documentation are derived from the user's own source (model/column names, types). Nothing is transmitted, persisted, or logged beyond the inherited `tracing` envelope (stderr/`log_file`, never stdout). Crucially, the synthesized constructor signature is built statically from the index — we never instantiate or import the model ([P1](../constitution.md)).
- **Baseline** — within the suite-wide posture (constitution §13.1 inheritance): reads only local files, executes no user code, opens no network connection, sends no telemetry.

### 13.2 Accessibility

**N/A** — the editor renders the signature popover; the server emits only text (labels, parameter spans, documentation) and an active-parameter index. The inherited content rule applies: the active parameter and optional/required status are conveyed in words and structure, never by color alone. See constitution §4.6.

## 16. Cross-References

- **Depends on:** [constitution](../constitution.md) — P1 (static synthesis, never instantiate the model), P4 (silence on unresolvable input), P5 (companion gate); [E07-data-model](../foundations/E07-data-model.md) — the `Model`/`Column`/`ColumnArgs`/`MappedType` facts the constructor signature is synthesized from; [E30-extraction-and-indexing](../foundations/E30-extraction-and-indexing.md) — the resolution that makes a model-name call resolvable to an indexed model.
- **Related:** [F03-completions](F03-completions.md) — the completions offered at these same call sites, sharing the model-constructor site and the companion gate; [E01-architecture](../foundations/E01-architecture.md) — the pure-function dispatch and cached source/tree this handler reads; [E17-testing](../foundations/E17-testing.md) — the `clean-blog` fixture and coverage policy; [E29-e2e-testing](../foundations/E29-e2e-testing.md) — the stdio harness and protocol-conformance journeys; [ADR-007](../decisions/ADR-007-companion-to-python-lsp.md) — the companion decision the negative contract enforces.

## 17. Changelog

- **2026-06-17** — Initial draft. Specified curated signatures for `ForeignKey`, `relationship`, `mapped_column`, and the Alembic `op.*` operations, the statically-synthesized model-constructor signature (columns → typed parameters, nullable/default → optional, keyword-aware highlighting), the active-parameter comma count, and the companion-gate negative contract (REQ-SIG-11); ported the signature templates from the legacy signature-help handler; added the §6 popover mockups and the §7 dispatch decision tree.
</content>
</invoke>
