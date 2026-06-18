<!--
  ADR — append-only. Never edit Context, Decision, or Consequences once Accepted.
  When reversed, mark Superseded and write a new ADR.
-->

# ADR-008 — `SQLA-I207` missing-column-comment ships off by default

> **Status:** Accepted
>
> **Date:** 2026-06-18
>
> **Supersedes:** —   ·   **Superseded by:** —   ·   **Amends:** [ADR-003](ADR-003-comprehensive-lints-defaults.md)

## Context

[ADR-003](ADR-003-comprehensive-lints-defaults.md) set the policy that every best-practice lint ([F02](../features/F02-best-practice-lints.md)) ships **on** by default, with two exceptions — `SQLA-H416` and `SQLA-H602` — held off because they are heuristics shaky enough to fire on valid code.

A third rule sits uncomfortably under that default-on policy for a *different* reason. `SQLA-I207` (missing-column-comment) flags any `mapped_column` without a `comment=`. It is not a shaky heuristic — its detection is trivial and never wrong — but requiring a database comment on *every* column is a strong, team-specific style opinion. On a typical existing schema it fires on nearly every column at once, which is exactly the "defaults to noise, so it gets turned off entirely" failure ADR-003 warned about. The rule is valuable for teams that have adopted column-comment discipline, and noise for everyone else.

## Decision

`SQLA-I207` (missing-column-comment) ships **off by default**, joining `SQLA-H416` and `SQLA-H602` in the off-by-default set. The `recommended` preset ([E15 REQ-CFG-07](../foundations/E15-app-config.md)) therefore enables every F02 rule *except these three*; the `all` preset still includes them. Unlike H416/H602, I207 is off for **noise/opinion** reasons, not heuristic instability — its detectability stays `high`.

## Consequences

The off-by-default set grows from two to three rules; every "default-on except two" statement becomes "except three," and the three are no longer all "hard heuristics" — H416/H602 are off because they false-positive, I207 because it is an opt-in style policy. Teams that want column-comment enforcement add `SQLA-I207` (or the `all` preset) to `diagnostics.select`. The first-run noise floor drops on the common case where a project has no column comments. ADR-003's core policy is unchanged; this only moves one rule across the on/off line and records why it differs from the other two.

## Alternatives considered

| Alternative | Why not chosen |
|---|---|
| Keep `SQLA-I207` on by default (per ADR-003) | It fires on nearly every column of a typical existing schema — the canonical "too noisy, gets disabled" case. Better off-by-default and opt-in. |
| Drop `SQLA-I207` entirely | It is genuinely useful for teams with comment discipline, and detection is cheap and exact. Shipping it off-by-default keeps the value without the noise. |
| Add a `nursery`/separate group instead of off-by-default | Over-engineering for one rule; the existing off-by-default mechanism and the `recommended`/`all` presets already express it. |

## Changelog

- **2026-06-18** — Created. Amends ADR-003: `SQLA-I207` joins the off-by-default set for noise/opinion reasons (not heuristic instability).
