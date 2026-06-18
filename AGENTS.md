# Agent Instructions

This file is the **single source of truth** for working on `sqlalchemy-lsp`. `CLAUDE.md` only points here.

`sqlalchemy-lsp` is a Rust language server that gives editors SQLAlchemy- and Alembic-aware intelligence, plus a matching headless CLI. It is spec-driven and runs as a *companion* to a general Python LSP.

## Specifications

This project is spec-driven — the docs in `specs/` are the source of truth for what to build and why.

- **Start at `specs/index.md`** — the map of every spec. Read it first.
- **Load specs on demand** — from the index, open only the spec(s) relevant to your task; don't load the whole suite into context.
- **Spec-first for new features** — before building a new feature, create its spec (copy `specs/features/F00-template.md`), get it reviewed, then implement.
- **Keep specs in sync** — when you change a feature's behavior, update its spec in the same change. Specs must not drift from code.

## Build & Test

Implementation has not started yet — there is no `Cargo.toml` until the M0 milestone (beads epic `sqlalchemy-lsp-cl2`). Once scaffolded, the `Justfile` wraps the standard commands:

```bash
just build      # cargo build
just test       # cargo test (unit + integration)
just lint       # cargo clippy --all-targets -- -D warnings
just fmt        # cargo fmt
just check      # fmt --check + clippy + test
# end-to-end (needs the built binary):
pytest tests/e2e
```

## Architecture Overview

One Rust binary speaks LSP over stdio (`sqlalchemy-lsp lsp --stdio`) and backs a headless `check` CLI from the same engine. It parses Python with tree-sitter, extracts SQLAlchemy/Alembic facts per file (Pass 1), and links them into a `DashMap` workspace index (Pass 2, debounced). Every feature is a pure function reading that index. Full detail: [`specs/foundations/E01-architecture.md`](specs/foundations/E01-architecture.md).

## Conventions & Patterns

- Coding conventions, the error/resilience contract, layering rules, and the never-log-to-stdout rule live in [`specs/foundations/E16-conventions.md`](specs/foundations/E16-conventions.md).
- Governing principles (P1–P6), the diagnostic-code scheme (`SQLA-<SEV><CLASS><NN>`), and the example cast live in [`specs/constitution.md`](specs/constitution.md).
- Never commit, push, or stage without an explicit instruction from the user.

## Non-Interactive Shell Commands

**ALWAYS use non-interactive flags** with file operations to avoid hanging on confirmation prompts.

Shell commands like `cp`, `mv`, and `rm` may be aliased to include `-i` (interactive) mode on some systems, causing the agent to hang indefinitely waiting for y/n input.

**Use these forms instead:**
```bash
# Force overwrite without prompting
cp -f source dest           # NOT: cp source dest
mv -f source dest           # NOT: mv source dest
rm -f file                  # NOT: rm file

# For recursive operations
rm -rf directory            # NOT: rm -r directory
cp -rf source dest          # NOT: cp -r source dest
```

**Other commands that may prompt:**
- `scp` - use `-o BatchMode=yes` for non-interactive
- `ssh` - use `-o BatchMode=yes` to fail instead of prompting
- `apt-get` - use `-y` flag
- `brew` - use `HOMEBREW_NO_AUTO_UPDATE=1` env var

<!-- BEGIN BEADS INTEGRATION v:1 profile:minimal hash:ca08a54f -->
## Beads Issue Tracker

This project uses **bd (beads)** for issue tracking. Run `bd prime` to see full workflow context and commands.

### Quick Reference

```bash
bd ready              # Find available work
bd show <id>          # View issue details
bd update <id> --claim  # Claim work
bd close <id>         # Complete work
```

### Rules

- Use `bd` for ALL task tracking — do NOT use TodoWrite, TaskCreate, or markdown TODO lists
- Run `bd prime` for detailed command reference and session close protocol
- Use `bd remember` for persistent knowledge — do NOT use MEMORY.md files

## Session Completion

**When ending a work session**, you MUST complete ALL steps below. Work is NOT complete until `git push` succeeds.

**MANDATORY WORKFLOW:**

1. **File issues for remaining work** - Create issues for anything that needs follow-up
2. **Run quality gates** (if code changed) - Tests, linters, builds
3. **Update issue status** - Close finished work, update in-progress items
4. **PUSH TO REMOTE** - This is MANDATORY:
   ```bash
   git pull --rebase
   bd dolt push
   git push
   git status  # MUST show "up to date with origin"
   ```
5. **Clean up** - Clear stashes, prune remote branches
6. **Verify** - All changes committed AND pushed
7. **Hand off** - Provide context for next session

**CRITICAL RULES:**
- Work is NOT complete until `git push` succeeds
- NEVER stop before pushing - that leaves work stranded locally
- NEVER say "ready to push when you are" - YOU must push
- If push fails, resolve and retry until it succeeds
<!-- END BEADS INTEGRATION -->
