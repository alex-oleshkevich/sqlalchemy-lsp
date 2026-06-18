<!--
  ADR — append-only. Never edit Context, Decision, or Consequences once Accepted.
  When reversed, mark Superseded and write a new ADR.
-->

# ADR-003 — Comprehensive lints, configurable, default-on

> **Status:** Accepted
>
> **Date:** 2026-06-17
>
> **Supersedes:** —   ·   **Superseded by:** —

## Context

We chose to ship far more than the legacy project's correctness checks. Beyond the ported correctness core, we researched a large best-practice rule set drawn from the SQLAlchemy 2.0 docs and other linters — flake8-sqlalchemy, ruff/bugbear, pylint-sqlalchemy, and the mypy plugin — adding roughly thirty new rules. That raised two questions. Should best-practice opinions be on by default, or opt-in? And how do we keep a stylistic nudge from being mistaken for a real bug? A linter that defaults to silence rarely gets enabled; a linter that defaults to noise gets turned off entirely. We also knew a few of the new rules rely on heuristics shaky enough to fire on valid code.

## Decision

We split findings into two feature specs: correctness diagnostics ([F01](../features/F01-orm-correctness-diagnostics.md), things that are positively wrong) and best-practice lints ([F02](../features/F02-best-practice-lints.md), opinions about good SQLAlchemy).

Every best-practice lint is configurable, with per-rule severity, and ships **on** by default — except the two hardest heuristics, `SQLA-H416` (viewonly-write) and `SQLA-H602` (association-proxy-misconfigured), which ship **off**.

## Consequences

A user gets the full benefit of the rule set the moment they install us, with no configuration ritual — best practices are the default, not a hidden feature. The two off-by-default heuristics keep the experience clean: the rules most likely to fire on valid code never surprise anyone unless they opt in. Per-rule severity means a team can downgrade an opinion to a hint or silence it entirely in [E15](../foundations/E15-app-config.md) config, and the `SQLA-` code stays stable even when its severity is overridden.

The cost is a higher first-run noise floor on legacy codebases that predate these conventions. We accept that: best practices are the point, and `# noqa` plus config `ignore` give an escape hatch. Keeping correctness and best-practice findings in separate specs adds a documentation seam, but it lets us hold correctness to a stricter bar (P4: only diagnose what's positively wrong) than the more opinionated lints.

## Alternatives considered

| Alternative | Why not chosen |
|---|---|
| All best-practice lints opt-in (default off) | A linter nobody enables helps nobody. The value is in good defaults; most users never tune config. |
| All lints on, including the two shaky heuristics | The viewonly-write and association-proxy heuristics fire on valid code often enough to erode trust in every other rule. Off-by-default is the honest default. |

## Changelog

- **2026-06-17** — Created.
