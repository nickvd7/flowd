# flowd

A local-first workflow observer and automation suggester.

This repository is a starter template for a terminal-first daemon + CLI that:
- observes selected local events
- normalizes them into workflow steps
- detects repeated patterns
- proposes safe automations

## Language Policy

All documentation, code comments, commit messages, issue discussions, and contributor-facing text must be written in English.

This ensures:
- accessibility for global contributors
- better compatibility with AI coding agents
- consistent documentation quality

## Workspace crates

- `flow-core` — shared domain types
- `flow-daemon` — background service entrypoint
- `flow-cli` — terminal interface
- `flow-db` — SQLite schema and repositories
- `flow-adapters` — event source adapters
- `flow-patterns` — sessionization and pattern detection
- `flow-dsl` — automation DSL
- `flow-exec` — dry-run and execution engine

## Included planning files

- `PLAN.md`
- `TASKS.md`
- `PROMPTS_FOR_CODEX.md`
- `PRIVATE_CORE_BOUNDARY.md`

## Quick start

```bash
cargo build
cargo test
cargo run -p flow-cli -- --help
cargo run -p flow-cli -- suggest
cargo run -p flow-daemon
```

`flow-cli suggest` reads pending suggestions from SQLite. By default it uses `./flowd.db`; tests and local runs can override this with `FLOWD_DB_PATH=/path/to/flowd.db`.
