<!--
  ADR — append-only. Never edit Context, Decision, or Consequences once Accepted.
  When reversed, mark Superseded and write a new ADR.
-->

# ADR-004 — Exclude raw-SQL linting and schema-drift detection

> **Status:** Accepted
>
> **Date:** 2026-06-17
>
> **Supersedes:** —   ·   **Superseded by:** —

## Context

Two capabilities kept coming up as tempting additions, and we considered both seriously before turning them down. The first was raw-SQL linting: parsing the SQL strings inside `text(...)` and `connection.execute(...)` and flagging mistakes the way sqlfluff does. The second was schema-drift detection: comparing the ORM models against a live database, Atlas-style, to report where the two have diverged. Both are genuinely useful. Both also pull the project well outside its niche and break its core principle. We are a static analyzer (P1: never import or run user code), and these features want either a full SQL grammar or a live database connection.

## Decision

We exclude raw-SQL linting in `text()`/`execute()` and schema-drift detection from the product as deliberate Non-Goals. Migration autogeneration is excluded on the same grounds. These are recorded as rejected proposals, listed under Out of scope in [index.md](../index.md).

## Consequences

We stay focused on what we do well: static ORM and Alembic intelligence read straight from the syntax tree. We avoid a heavy SQL-parser dependency, the per-dialect grammar maintenance it implies, and the support burden of getting raw-SQL diagnostics right across every dialect. We avoid a database connection entirely, which keeps P1 intact — we never need credentials, a network socket, or a running database to do our job. The Security & Privacy posture stays simple: local files only, no network, no secrets.

The cost is a real gap for users who want these things. Someone hunting for SQL typos inside `text()` blocks, or wanting drift alerts in CI, must reach for a dedicated tool — sqlfluff for the former, Atlas or `alembic check` for the latter. We accept that gap. These are well-served by existing tools, and bolting them on would compromise the static-analysis-only guarantee that makes the rest of the server trustworthy.

## Alternatives considered

| Alternative | Why not chosen |
|---|---|
| Bundle sqlfluff-style SQL linting for `text()`/`execute()` | Requires a full SQL grammar with per-dialect upkeep, a large dependency, and ongoing support — far outside the ORM niche. |
| Atlas-style schema-drift detection against a live database | Requires a database connection, breaking the static-analysis-only principle (P1) and adding credentials, network, and runtime state to a tool that has none. |

## Changelog

- **2026-06-17** — Created.
