# E29 — End-to-End Testing

> **Status:** Draft
>
> **Version:** 0.1   ·   **Last updated:** 2026-06-17
>
> **Purpose:** How sqlalchemy-lsp is tested end to end — the coverage policy for full protocol journeys, the `pytest-lsp` harness, and the patterns every feature's E2E plan reuses. Each feature's own journeys live in its spec's §12 and link here.
>
> **Depends on:** [constitution](../constitution.md), [E17-testing](E17-testing.md)   ·   **Related:** [E01-architecture](E01-architecture.md)

> Requirement tag: **E2E**

---

## 1. Purpose & Scope

This spec defines how we test complete journeys through the running server, driving it the way a real editor does: a JSON-RPC client over stdio against the built binary. It is the authority every feature's **End-to-End Test Plan** (§12) defers to.

This spec covers:

- The E2E coverage policy every user-facing feature meets.
- The `pytest-lsp` harness and how it drives the real binary.
- Environment, seeding, and teardown for repeatable runs.
- The protocol-conformance journeys every feature inherits.
- Naming and structure for `E2E-NN` scenarios.

Out of scope: unit and integration testing and the shared fixtures registry — those live in [E17-testing](E17-testing.md).

## 2. Coverage Policy

**REQ-E2E-01 — Cover 100% of feature scope, end to end.**

Each user-facing feature's E2E plan (its §12) covers **all of its user-visible scope**: the happy path **and** every reasonably possible error path — an unknown FK table, a `back_populates` mismatch, a half-typed mid-keystroke file, an unresolvable relationship target, a broken migration chain, an empty workspace. A journey a real user can hit must have a scenario.

**REQ-E2E-02 — Happy and error paths are both first-class.**

An E2E plan that only walks the happy path is incomplete. Error paths are enumerated from the feature's edge cases (§10); each gets its own `E2E-NN` scenario with an asserted outcome. For a diagnostic feature this means: `clean-blog` → zero diagnostics is one scenario, and every broken fixture → its exact `SQLA-` code and range is another.

When [acceptance criteria](../constitution.md#46-non-functional--operational-scope) are enabled, these same scenarios are written Given/When/Then in the feature's §12.3 — the E2E plan and the acceptance criteria are the same list.

## 3. Tools & Harness

The standard E2E toolchain; versions are pinned in [E03-tech-stack](E03-tech-stack.md).

- **Driver:** `pytest-lsp` with `lsprotocol` — a real LSP client speaking JSON-RPC over stdio to the built `sqlalchemy-lsp lsp` binary. This is the exact path an editor takes, so it catches integration bugs unit tests can't: wrong capability flags, off-by-encoding ranges, out-of-order notifications, a publish that never arrives.
- **Runner & reporting:** `pytest`, with the server's stderr `tracing` log captured on failure.
- **Where E2E runs:** locally via `pytest tests/e2e`, and in `qa.yml` ([F16](../features/F16-release-ci.md)) against the release build.

> **Note:** This `pytest-lsp` + `lsprotocol` harness **replaces the legacy server's hand-rolled `LspClient`**. The hand-rolled client framed JSON-RPC by hand and was easy to desync; `pytest-lsp` gives us a typed, spec-conformant client and request/notification correlation for free, so scenarios read as editor interactions rather than byte plumbing.

## 4. Environment, Seeding & Teardown

A scenario gets a known starting state and cleans up after itself, so no two scenarios can interfere.

- **Test data:** each scenario opens a fixture workspace from the [E17 fixtures registry](E17-testing.md#5-fixtures-registry) — `clean-blog` for the happy path, and the matching broken variant (e.g. [bad-fk](E17-testing.md#bad-fk), [back-populates-mismatch](E17-testing.md#back-populates-mismatch), [broken-migration-chain](E17-testing.md#broken-migration-chain)) for each error path.
- **Isolation:** each scenario gets a fresh copy of its fixture in a temp directory and its own server process, so no scenario sees another's edits. Per-scenario isolation is mandatory — a leaked edit or a shared server is a flake waiting to happen.
- **Teardown:** the server is shut down cleanly (`shutdown`/`exit`) and the temp workspace removed between scenarios. Because the server runs no user code and opens no network connections (constitution P1), there is nothing else to reset.

## 5. Patterns

The conventions that keep the E2E suite stable and readable.

- **Wait on state, never sleep.** Scenarios await the publish, the response, or the relink they expect — never a fixed delay. The "a newly opened file always receives a (possibly empty) publish" guarantee ([E01](E01-architecture.md)) is the canonical "Pass 2 ran" signal a scenario synchronizes on. A fixed `sleep` is both slow and flaky; awaiting the publish is neither.
- **Assert ranges against both encodings.** A diagnostic, hover, or rename scenario asserts the exact range and content — not just the code — and does so under both negotiated encodings using [non-ascii](E17-testing.md#non-ascii). A range that is right in UTF-8 and off by a multi-byte character in UTF-16 is a real, common bug; this is how we catch it.
- **Drive the real surfaces.** Scenarios call the real `textDocument/*` and `workspace/executeCommand` methods, not internal helpers. If a feature can only be exercised through a private function, it isn't really tested end to end.
- **Flake policy:** a flaky scenario is quarantined and fixed, never retried-until-green.

**REQ-E2E-03 — Protocol conformance is a shared journey set.**

Every feature inherits the same protocol-conduct journeys, written once here so no feature re-tests them. They are:

- **Open → publish.** A newly opened file always receives a publish, even when it's clean (an empty diagnostic list). This is the signal scenarios wait on.
- **Relink → empty publish.** Editing a file so a finding no longer applies sends a new publish with that finding gone — an explicit empty publish clears it, never a silent drop.
- **Ordered `didChange`.** Two rapid edits apply in order; the diagnostics reflect the final text, not a stale intermediate.
- **`$/cancelRequest`.** An in-flight request (a slow completion or workspace symbol) honors a cancel and returns promptly without crashing.
- **External write → re-index.** A file changed on disk outside the editor (via the watcher) re-extracts and updates diagnostics — including the cross-file case: editing a model in file A updates a diagnostic in file B without reopening B.
- **Non-blocking `initialize`.** `initialize` returns immediately with the advertised capabilities while the workspace scan proceeds in the background; the client is never blocked waiting for the index.

Features reference these rather than re-testing them. A feature's §12 lists only its own scenarios; the shared set is assumed.

## 6. Conventions

- **Naming:** scenarios are `E2E-NN` within a feature, titled by the journey — `E2E-04: editing the FK clears SQLA-E301`.
- **Structure:** given (a seeded fixture) → when (client requests) → then (asserted response/publish). When acceptance criteria are enabled (constitution §4.6), the same scenarios are written Given/When/Then in the feature's §12.3.
- **Where feature E2E plans link:** every feature's §12 links here for the harness and patterns rather than restating them, and links the [E17 fixtures registry](E17-testing.md#5-fixtures-registry) for the workspaces it seeds.

## 7. Running E2E & CI

`pytest tests/e2e` runs the journeys locally against the built binary. `qa.yml` ([F16](../features/F16-release-ci.md)) runs them on every push and PR; a failing journey blocks merge, and the server `tracing` log plus the failing exchange are attached as artifacts so a failure is debuggable from CI alone.

## 8. Cross-References

- **Depends on:** [constitution](../constitution.md) — the coverage principles (§4.4) this enforces; [E17-testing](E17-testing.md) — the categories and the fixtures registry this reuses.
- **Related:** [E01-architecture](E01-architecture.md) — the protocol conduct (push/pull diagnostics, empty-publish-on-clear, non-blocking `initialize`, watcher re-index) the conformance journeys assert.

## 9. Changelog

- **2026-06-17** — Initial draft: the E2E coverage policy (happy + every error path), the `pytest-lsp` + `lsprotocol` stdio harness against the built binary (replacing the legacy hand-rolled `LspClient`), per-scenario fixture isolation and teardown, the wait-on-state and dual-encoding patterns, and the shared protocol-conformance journey set (REQ-E2E-03).
