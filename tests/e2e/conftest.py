"""Shared pytest fixtures for sqlalchemy-lsp end-to-end tests.

Each test gets its own server process and a private copy of the fixture
workspace in a temp directory — the isolation guarantee from E29 §4.
"""

import asyncio
import pathlib
import shutil
import time

import pytest
import pytest_lsp
from lsprotocol import types
from pytest_lsp import ClientServerConfig, LanguageClient

# ── Paths ─────────────────────────────────────────────────────────────────────

_PROJECT_ROOT = pathlib.Path(__file__).parent.parent.parent
_FIXTURES_DIR = pathlib.Path(__file__).parent / "fixtures"
_SERVER_BIN = _PROJECT_ROOT / "target" / "debug" / "sqlalchemy-lsp"


# ── Client capabilities ───────────────────────────────────────────────────────

def _capabilities() -> types.ClientCapabilities:
    """Advertise a rich client so the server can negotiate encoding and refresh."""
    return types.ClientCapabilities(
        general=types.GeneralClientCapabilities(
            position_encodings=[
                types.PositionEncodingKind.Utf8,
                types.PositionEncodingKind.Utf16,
            ],
        ),
        workspace=types.WorkspaceClientCapabilities(
            # dynamic_registration=False: the test client has no handler for
            # client/registerCapability.  pytest-lsp promotes safe errors to
            # test failures via pytest_runtest_makereport, so we prevent the
            # server from sending the request entirely.
            did_change_watched_files=types.DidChangeWatchedFilesClientCapabilities(
                dynamic_registration=False,
                relative_pattern_support=False,
            ),
            # Omit inlayHint.refreshSupport for the same reason.
        ),
        text_document=types.TextDocumentClientCapabilities(
            synchronization=types.TextDocumentSyncClientCapabilities(
                dynamic_registration=False,
                will_save=False,
                will_save_wait_until=False,
                did_save=True,
            ),
            publish_diagnostics=types.PublishDiagnosticsClientCapabilities(),
            diagnostic=types.DiagnosticClientCapabilities(
                dynamic_registration=False,
                related_document_support=False,
            ),
        ),
    )


# ── Per-test server + workspace ───────────────────────────────────────────────

@pytest_lsp.fixture(
    config=ClientServerConfig(server_command=[str(_SERVER_BIN), "lsp"]),
)
async def client(lsp_client: LanguageClient, tmp_path: pathlib.Path):
    """Start a fresh server with a private copy of clean_blog in tmp_path.

    The fixture sends initialize/initialized and shuts down cleanly after the
    test, giving every scenario an isolated, reproducible starting state.
    """
    workspace = tmp_path / "workspace"
    shutil.copytree(_FIXTURES_DIR / "clean_blog", workspace)
    root_uri = workspace.as_uri()

    await lsp_client.initialize_session(
        types.InitializeParams(
            capabilities=_capabilities(),
            root_uri=root_uri,
            workspace_folders=[
                types.WorkspaceFolder(uri=root_uri, name="workspace"),
            ],
        )
    )

    yield

    await lsp_client.shutdown_session()


# ── Helpers re-used across test modules ──────────────────────────────────────

def workspace_uri(tmp_path: pathlib.Path, *parts: str) -> str:
    """Return a file:// URI for a path inside the test's workspace copy."""
    return (tmp_path / "workspace" / pathlib.Path(*parts)).as_uri()


def read_fixture(name: str, *parts: str) -> str:
    """Read a file from a fixture workspace (the original, not the temp copy)."""
    return (_FIXTURES_DIR / name / pathlib.Path(*parts)).read_text()


async def wait_for_uri_diagnostics(
    client: LanguageClient, uri: str, timeout: float = 5.0
) -> None:
    """Block until publishDiagnostics is received for ``uri``.

    The background workspace scan may publish diagnostics for OTHER files
    before the file under test is opened.  This helper loops until the target
    URI specifically appears in ``client.diagnostics``.
    """
    deadline = time.monotonic() + timeout
    while uri not in client.diagnostics:
        remaining = deadline - time.monotonic()
        if remaining <= 0:
            raise TimeoutError(
                f"Timed out waiting for publishDiagnostics for {uri!r}. "
                f"Known URIs: {list(client.diagnostics)}"
            )
        future = client.protocol.wait_for_notification_async(
            types.TEXT_DOCUMENT_PUBLISH_DIAGNOSTICS
        )
        try:
            await asyncio.wait_for(future, timeout=min(1.0, remaining))
        except TimeoutError:
            pass
