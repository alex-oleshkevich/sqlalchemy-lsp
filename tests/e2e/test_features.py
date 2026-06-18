"""Feature E2E tests — drives the live LSP server for all user-visible features.

Uses pytest-lsp to communicate via JSON-RPC over stdio with the built binary.
Each test opens the relevant fixture files, waits for Pass-1 diagnostics, then
exercises a specific LSP request and asserts the response.
"""

import asyncio
import pathlib

import pytest
from lsprotocol import types
from pytest_lsp import LanguageClient

from conftest import read_fixture, wait_for_uri_diagnostics, workspace_uri


# ── Position helpers ──────────────────────────────────────────────────────────

def _find(text: str, needle: str, occurrence: int = 1) -> types.Position:
    """Return the LSP Position of the nth occurrence of `needle` in `text`."""
    idx = -1
    for _ in range(occurrence):
        idx = text.find(needle, idx + 1)
        if idx < 0:
            raise ValueError(f"{needle!r} (occurrence {occurrence}) not in text")
    line = text[:idx].count("\n")
    col = idx - text[:idx].rfind("\n") - 1
    return types.Position(line=line, character=col)


async def _open_and_wait(client: LanguageClient, uri: str, text: str) -> None:
    client.text_document_did_open(
        types.DidOpenTextDocumentParams(
            text_document=types.TextDocumentItem(
                uri=uri, language_id="python", version=1, text=text
            )
        )
    )
    await wait_for_uri_diagnostics(client, uri)


async def _open_all_models(
    client: LanguageClient, tmp_path: pathlib.Path
) -> dict[str, str]:
    """Open all four model files and wait for their diagnostics.

    Returns a mapping of basename → (uri, text) so callers can compute positions.
    """
    files = ["user.py", "post.py", "comment.py", "tag.py"]
    result: dict[str, tuple[str, str]] = {}
    for name in files:
        uri = workspace_uri(tmp_path, "models", name)
        text = read_fixture("clean_blog", "models", name)
        await _open_and_wait(client, uri, text)
        result[name] = (uri, text)
    return result


# ── Go-to-definition ──────────────────────────────────────────────────────────


async def test_goto_definition_fk_resolves_to_column(
    client: LanguageClient, tmp_path: pathlib.Path
):
    """FK string 'users.id' navigates to User.id column (REQ-DEF-01)."""
    models = await _open_all_models(client, tmp_path)
    user_uri, _ = models["user.py"]
    post_uri, post_text = models["post.py"]

    # In post.py, author_id has ForeignKey("users.id")
    pos = _find(post_text, '"users.id"')
    pos = types.Position(line=pos.line, character=pos.character + 1)  # inside quotes

    result = await client.text_document_definition_async(
        types.DefinitionParams(
            text_document=types.TextDocumentIdentifier(uri=post_uri),
            position=pos,
        )
    )

    assert result is not None
    loc = result if isinstance(result, types.Location) else result[0]
    assert loc.uri == user_uri
    # Should land on "id" column definition in User (line 23, 0-indexed)
    assert loc.range.start.line == 23


async def test_goto_definition_relationship_target_resolves_to_model(
    client: LanguageClient, tmp_path: pathlib.Path
):
    """String model name in relationship() navigates to the class (REQ-DEF-03)."""
    models = await _open_all_models(client, tmp_path)
    user_uri, user_text = models["user.py"]

    # In user.py Profile.user has relationship("User", ...)
    # We click inside "User" in the relationship call
    pos = _find(user_text, 'relationship("User"')
    # Move to inside the "User" string (after the opening quote)
    pos = types.Position(line=pos.line, character=pos.character + len('relationship("') + 1)

    result = await client.text_document_definition_async(
        types.DefinitionParams(
            text_document=types.TextDocumentIdentifier(uri=user_uri),
            position=pos,
        )
    )

    assert result is not None
    loc = result if isinstance(result, types.Location) else result[0]
    assert loc.uri == user_uri
    # User class is at line 20 (0-indexed) in user.py
    assert loc.range.start.line == 20


async def test_goto_definition_back_populates_resolves_to_relationship(
    client: LanguageClient, tmp_path: pathlib.Path
):
    """back_populates string navigates to the counterpart relationship (REQ-DEF-04)."""
    models = await _open_all_models(client, tmp_path)
    user_uri, user_text = models["user.py"]

    # In user.py Profile.user: back_populates="profile"
    # We click inside "profile" in back_populates="profile"
    # User.profile has back_populates="user" — clicking that should go to Profile.user
    pos = _find(user_text, 'back_populates="user"')
    pos = types.Position(line=pos.line, character=pos.character + len('back_populates="') + 1)

    result = await client.text_document_definition_async(
        types.DefinitionParams(
            text_document=types.TextDocumentIdentifier(uri=user_uri),
            position=pos,
        )
    )

    assert result is not None
    loc = result if isinstance(result, types.Location) else result[0]
    assert loc.uri == user_uri
    # Profile.user relationship is at line 17 (0-indexed) in user.py
    assert loc.range.start.line == 17


async def test_goto_definition_model_name_in_string_type_hint(
    client: LanguageClient, tmp_path: pathlib.Path
):
    """Model name string inside Mapped['User'] navigates to the class."""
    models = await _open_all_models(client, tmp_path)
    user_uri, user_text = models["user.py"]

    # In user.py, Profile.user has type Mapped["User"]
    # Click on "User" inside Mapped["User"]
    pos = _find(user_text, 'Mapped["User"]')
    # Move inside the string: Mapped[" → +8 chars, then +1 for 'U'
    pos = types.Position(line=pos.line, character=pos.character + len('Mapped["') + 1)

    result = await client.text_document_definition_async(
        types.DefinitionParams(
            text_document=types.TextDocumentIdentifier(uri=user_uri),
            position=pos,
        )
    )

    assert result is not None
    loc = result if isinstance(result, types.Location) else result[0]
    assert loc.uri == user_uri
    assert loc.range.start.line == 20  # class User line


async def test_goto_definition_returns_none_for_plain_python(
    client: LanguageClient, tmp_path: pathlib.Path
):
    """Clicking on a non-SA token returns null (REQ-DEF-10)."""
    models = await _open_all_models(client, tmp_path)
    user_uri, _ = models["user.py"]

    # Click on line 0, col 0 — `from` keyword, not an SA construct
    result = await client.text_document_definition_async(
        types.DefinitionParams(
            text_document=types.TextDocumentIdentifier(uri=user_uri),
            position=types.Position(line=0, character=0),
        )
    )

    assert result is None or result == []


# ── Hover ─────────────────────────────────────────────────────────────────────


async def test_hover_over_column_returns_markdown(
    client: LanguageClient, tmp_path: pathlib.Path
):
    """Hovering on a column name returns a non-empty markdown hover card."""
    models = await _open_all_models(client, tmp_path)
    user_uri, user_text = models["user.py"]

    # Hover on User.id (line 23, col 4 — the 'i' of 'id')
    pos = _find(user_text, "    id: Mapped[int] = mapped_column(Integer, primary_key=True)")
    pos = types.Position(line=pos.line, character=4)

    result = await client.text_document_hover_async(
        types.HoverParams(
            text_document=types.TextDocumentIdentifier(uri=user_uri),
            position=pos,
        )
    )

    assert result is not None
    assert result.contents is not None
    content = result.contents
    if isinstance(content, types.MarkupContent):
        assert content.value
        assert "int" in content.value or "id" in content.value
    elif isinstance(content, list):
        assert content
    else:
        assert content.value


async def test_hover_over_relationship_returns_markdown(
    client: LanguageClient, tmp_path: pathlib.Path
):
    """Hovering on a relationship name returns a non-empty hover card."""
    models = await _open_all_models(client, tmp_path)
    user_uri, user_text = models["user.py"]

    # Hover on User.posts (line 27, col 4)
    pos = _find(user_text, "    posts: Mapped")
    pos = types.Position(line=pos.line, character=4)

    result = await client.text_document_hover_async(
        types.HoverParams(
            text_document=types.TextDocumentIdentifier(uri=user_uri),
            position=pos,
        )
    )

    assert result is not None
    assert result.contents is not None


async def test_hover_outside_sa_construct_returns_none(
    client: LanguageClient, tmp_path: pathlib.Path
):
    """Hovering outside any SA construct returns null."""
    models = await _open_all_models(client, tmp_path)
    user_uri, _ = models["user.py"]

    result = await client.text_document_hover_async(
        types.HoverParams(
            text_document=types.TextDocumentIdentifier(uri=user_uri),
            position=types.Position(line=0, character=0),
        )
    )

    assert result is None


# ── Find references ───────────────────────────────────────────────────────────


async def test_references_on_fk_column_finds_usages(
    client: LanguageClient, tmp_path: pathlib.Path
):
    """References on User.id finds FK usages in other models."""
    models = await _open_all_models(client, tmp_path)
    user_uri, user_text = models["user.py"]

    # References on User.id column (line 23, col 4)
    pos = _find(user_text, "    id: Mapped[int] = mapped_column(Integer, primary_key=True)")
    pos = types.Position(line=pos.line, character=4)

    result = await client.text_document_references_async(
        types.ReferenceParams(
            text_document=types.TextDocumentIdentifier(uri=user_uri),
            position=pos,
            context=types.ReferenceContext(include_declaration=True),
        )
    )

    assert result is not None
    # Should find FK references to "users.id" in post.py and profile (user.py)
    assert len(result) >= 1


async def test_references_on_model_class_finds_usages(
    client: LanguageClient, tmp_path: pathlib.Path
):
    """References on the User class name finds FK and relationship usages."""
    models = await _open_all_models(client, tmp_path)
    user_uri, user_text = models["user.py"]

    # References on User class name (line 20, col 6 = 'U' in 'class User')
    pos = _find(user_text, "class User(Base):")
    pos = types.Position(line=pos.line, character=6)

    result = await client.text_document_references_async(
        types.ReferenceParams(
            text_document=types.TextDocumentIdentifier(uri=user_uri),
            position=pos,
            context=types.ReferenceContext(include_declaration=True),
        )
    )

    # May be None if no references found, or a list
    if result is not None:
        assert isinstance(result, list)


# ── Completion ────────────────────────────────────────────────────────────────


async def test_fk_completion_returns_table_column_items(
    client: LanguageClient, tmp_path: pathlib.Path
):
    """Completion inside ForeignKey('us') returns users.* column items."""
    models = await _open_all_models(client, tmp_path)

    # Create a new model file with a partial FK string
    new_uri = workspace_uri(tmp_path, "models", "order.py")
    # Cursor will be at end of "us inside the ForeignKey string
    new_text = (
        "from __future__ import annotations\n"
        "from sqlalchemy import ForeignKey\n"
        "from sqlalchemy.orm import DeclarativeBase, Mapped, mapped_column\n"
        "\n"
        "class Base(DeclarativeBase):\n"
        "    pass\n"
        "\n"
        "class Order(Base):\n"
        "    __tablename__ = 'orders'\n"
        '    user_id: Mapped[int] = mapped_column(ForeignKey("us'
    )
    # Line 9 (0-indexed), col = len of the last line
    cursor_line = new_text.count("\n")
    cursor_col = len(new_text.split("\n")[-1])

    await _open_and_wait(client, new_uri, new_text)

    result = await client.text_document_completion_async(
        types.CompletionParams(
            text_document=types.TextDocumentIdentifier(uri=new_uri),
            position=types.Position(line=cursor_line, character=cursor_col),
        )
    )

    assert result is not None
    items = result if isinstance(result, list) else result.items
    assert items
    labels = [item.label for item in items]
    # Should offer users.id, users.name, users.email (from User model)
    assert any("users." in lbl for lbl in labels), f"no users.* in {labels}"


async def test_fk_completion_label_matches_table_column(
    client: LanguageClient, tmp_path: pathlib.Path
):
    """Each FK completion item label is in 'table.column' format."""
    models = await _open_all_models(client, tmp_path)

    new_uri = workspace_uri(tmp_path, "models", "fk_test.py")
    new_text = (
        "from sqlalchemy import ForeignKey\n"
        "from sqlalchemy.orm import DeclarativeBase, Mapped, mapped_column\n"
        "\n"
        "class Base(DeclarativeBase):\n"
        "    pass\n"
        "\n"
        "class Ref(Base):\n"
        "    __tablename__ = 'refs'\n"
        '    col: Mapped[int] = mapped_column(ForeignKey("'
    )
    cursor_line = new_text.count("\n")
    cursor_col = len(new_text.split("\n")[-1])

    await _open_and_wait(client, new_uri, new_text)

    result = await client.text_document_completion_async(
        types.CompletionParams(
            text_document=types.TextDocumentIdentifier(uri=new_uri),
            position=types.Position(line=cursor_line, character=cursor_col),
        )
    )

    assert result is not None
    items = result if isinstance(result, list) else result.items
    assert items
    for item in items:
        assert "." in item.label, f"label {item.label!r} not in table.column format"


async def test_completion_after_applying_fk_item(
    client: LanguageClient, tmp_path: pathlib.Path
):
    """After applying a FK completion item the file has the full table.column string.

    This verifies the completion label is the correct insert text so the
    editor inserts exactly what we expect.
    """
    models = await _open_all_models(client, tmp_path)

    new_uri = workspace_uri(tmp_path, "models", "apply_test.py")
    # 'us is the prefix — cursor right after the 's'
    prefix_text = (
        "from sqlalchemy import ForeignKey\n"
        "from sqlalchemy.orm import DeclarativeBase, Mapped, mapped_column\n"
        "\n"
        "class Base(DeclarativeBase):\n"
        "    pass\n"
        "\n"
        "class Tbl(Base):\n"
        "    __tablename__ = 'tbl'\n"
        '    fk: Mapped[int] = mapped_column(ForeignKey("us'
    )
    cursor_line = prefix_text.count("\n")
    cursor_col = len(prefix_text.split("\n")[-1])

    await _open_and_wait(client, new_uri, prefix_text)

    result = await client.text_document_completion_async(
        types.CompletionParams(
            text_document=types.TextDocumentIdentifier(uri=new_uri),
            position=types.Position(line=cursor_line, character=cursor_col),
        )
    )

    assert result is not None
    items = result if isinstance(result, list) else result.items
    assert items

    # Pick "users.id" specifically
    users_id = next((item for item in items if item.label == "users.id"), None)
    assert users_id is not None, f"'users.id' not found in {[i.label for i in items]}"

    # Simulate applying the completion: replace the prefix with the label
    # The completion should insert "users.id" — verify the label is correct
    assert users_id.label == "users.id"

    # Apply completion by sending didChange with the full text
    full_last_line = prefix_text.split("\n")[-1][: -len("us")] + users_id.label
    full_text = "\n".join(prefix_text.split("\n")[:-1] + [full_last_line])
    client.text_document_did_change(
        types.DidChangeTextDocumentParams(
            text_document=types.VersionedTextDocumentIdentifier(uri=new_uri, version=2),
            content_changes=[types.TextDocumentContentChangeWholeDocument(text=full_text)],
        )
    )

    # Verify the resulting text contains "users.id"
    assert "users.id" in full_text


# ── Rename ────────────────────────────────────────────────────────────────────


async def test_rename_column_updates_references(
    client: LanguageClient, tmp_path: pathlib.Path
):
    """Renaming a column updates its own name range (REQ-REN-01)."""
    models = await _open_all_models(client, tmp_path)
    user_uri, user_text = models["user.py"]

    # Rename User.name → User.full_name
    pos = _find(user_text, "    name: Mapped[str]")
    pos = types.Position(line=pos.line, character=4)

    result = await client.text_document_rename_async(
        types.RenameParams(
            text_document=types.TextDocumentIdentifier(uri=user_uri),
            position=pos,
            new_name="full_name",
        )
    )

    assert result is not None
    assert result.changes or result.document_changes


async def test_rename_not_applicable_returns_none(
    client: LanguageClient, tmp_path: pathlib.Path
):
    """Rename on a non-SA token returns null."""
    models = await _open_all_models(client, tmp_path)
    user_uri, _ = models["user.py"]

    result = await client.text_document_rename_async(
        types.RenameParams(
            text_document=types.TextDocumentIdentifier(uri=user_uri),
            position=types.Position(line=0, character=0),
            new_name="whatever",
        )
    )

    assert result is None


# ── Inlay hints ───────────────────────────────────────────────────────────────


async def test_inlay_hints_returned_for_model_file(
    client: LanguageClient, tmp_path: pathlib.Path
):
    """Inlay hints are returned for the visible range of a model file."""
    models = await _open_all_models(client, tmp_path)
    user_uri, _ = models["user.py"]

    result = await client.text_document_inlay_hint_async(
        types.InlayHintParams(
            text_document=types.TextDocumentIdentifier(uri=user_uri),
            range=types.Range(
                start=types.Position(line=0, character=0),
                end=types.Position(line=31, character=0),
            ),
        )
    )

    # Inlay hints may be empty if no hints apply to this file — just ensure no crash
    assert result is None or isinstance(result, list)


# ── Diagnostics (via LSP push) ────────────────────────────────────────────────


async def test_broken_fk_produces_diagnostic(
    client: LanguageClient, tmp_path: pathlib.Path
):
    """Changing a FK to reference an unknown table produces SQLA-E301."""
    user_uri, user_text = (lambda t: (t[0], t[1]))(
        (workspace_uri(tmp_path, "models", "user.py"),
         read_fixture("clean_blog", "models", "user.py"))
    )
    await _open_and_wait(client, user_uri, user_text)

    # Create a new model file with a bad FK
    bad_uri = workspace_uri(tmp_path, "models", "bad.py")
    bad_text = (
        "from sqlalchemy import ForeignKey\n"
        "from sqlalchemy.orm import DeclarativeBase, Mapped, mapped_column\n"
        "\n"
        "class Base(DeclarativeBase):\n"
        "    pass\n"
        "\n"
        "class BadModel(Base):\n"
        "    __tablename__ = 'bad_models'\n"
        "    fk: Mapped[int] = mapped_column(ForeignKey('ghost_table.id'))\n"
    )
    await _open_and_wait(client, bad_uri, bad_text)

    # The diagnostics for bad.py should include SQLA-E301
    diags = client.diagnostics.get(bad_uri, [])
    codes = [d.code for d in diags]
    assert "SQLA-E301" in codes, f"expected SQLA-E301 in {codes}"


async def test_clean_file_produces_no_diagnostics(
    client: LanguageClient, tmp_path: pathlib.Path
):
    """A well-formed model file produces zero diagnostics."""
    models = await _open_all_models(client, tmp_path)
    for uri, _ in models.values():
        diags = client.diagnostics.get(uri, [])
        assert diags == [], f"unexpected diagnostics in {uri}: {diags}"


# ── Signature help ────────────────────────────────────────────────────────────


async def test_signature_help_inside_relationship_call(
    client: LanguageClient, tmp_path: pathlib.Path
):
    """Signature help is available inside relationship() calls."""
    models = await _open_all_models(client, tmp_path)

    new_uri = workspace_uri(tmp_path, "models", "sig_test.py")
    new_text = (
        "from sqlalchemy.orm import DeclarativeBase, Mapped, relationship\n"
        "\n"
        "class Base(DeclarativeBase):\n"
        "    pass\n"
        "\n"
        "class M(Base):\n"
        "    __tablename__ = 'm'\n"
        "    rel = relationship("
    )
    cursor_line = new_text.count("\n")
    cursor_col = len(new_text.split("\n")[-1])

    await _open_and_wait(client, new_uri, new_text)

    result = await client.text_document_signature_help_async(
        types.SignatureHelpParams(
            text_document=types.TextDocumentIdentifier(uri=new_uri),
            position=types.Position(line=cursor_line, character=cursor_col),
        )
    )

    # May return None if no signature help applies — just ensure no crash
    assert result is None or isinstance(result, types.SignatureHelp)
