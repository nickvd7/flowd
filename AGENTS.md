# AGENTS.md

## Project
flowd is the open-core, local-first workflow automation engine.

It owns:
- adapters
- event ingestion
- normalization
- sessions
- pattern detection
- baseline suggestions
- automation approval
- execution
- undo
- local SQLite storage
- CLI experience
- intelligence client boundary

It does NOT own:
- private ranking logic
- private timing logic
- private suppression logic
- private personalization logic
- private semantic grouping logic
- private wording logic

Those belong to `flowd-intelligence`.

## Architecture rule
Open-core owns facts and actions.
Private-core improves prioritization, timing, suppression, personalization, and presentation.

Integration is one-way:

flowd -> flowd-intelligence

flowd must remain fully functional without the private intelligence layer.

## How to work
- Keep everything deterministic.
- Keep everything local-first.
- Do not add cloud dependencies unless explicitly requested.
- Do not add GUI/TUI unless explicitly requested.
- Keep CLI output readable and concise.
- Prefer small, reviewable changes.

## Build and test
Run before finishing:
```bash
cargo build
cargo test