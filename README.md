# flowd

![Rust](https://img.shields.io/badge/Rust-stable-orange)
![License](https://img.shields.io/badge/license-MIT-blue)
![Local-first](https://img.shields.io/badge/local--first-yes-green)

**flowd is a local-first automation engine that learns your workflows by observing real activity on your computer.**

Instead of writing automation rules manually, flowd records repeated actions, turns them into structured workflow patterns, and proposes safe automations you can review and approve.

flowd is designed for people who want automation **without configuration, cloud dependencies, or hidden behavior**.

Key ideas:

- **Observe real work first** instead of designing rules
- **Detect repeated workflows automatically**
- **Propose safe automations you can inspect and approve**

In short: **observe first, automate later.**

--- 

## Real workflow examples

Examples of workflows flowd can learn automatically:

```
rename invoice → move to archive
save screenshot → move to screenshots folder
move downloads → organize by file type
rename document → move to project folder
```

flowd observes these actions as you perform them and proposes automations after the pattern repeats several times.

No rules. No configuration. Just observation.

---

## Example

flowd observes your repeated workflow:

```
rename invoice → move to archive
```

After detecting this pattern several times it suggests:

```
archive invoices automatically
```

Inspect detected workflows with:

```bash
cargo run -p flow-cli patterns
cargo run -p flow-cli suggestions
cargo run -p flow-cli approve <suggestion_id>
```

---

## Why flowd

Most automation tools require users to manually design rules like:

```
IF file renamed
THEN move file
```

But people often don't know which tasks they repeat most.

flowd takes the opposite approach:

1. observe real workflows
2. detect repeated patterns
3. suggest safe automations
4. let the user approve them

Automation becomes **discoverable instead of configurable**.

---

## How it works

flowd continuously records filesystem activity and builds a local model of your workflows.

Pipeline:

```
filesystem watcher
      ↓
raw_events
      ↓
normalized_events
      ↓
sessions
      ↓
patterns
      ↓
suggestions
      ↓
automations
```

Everything is stored locally in **SQLite**.

---

## Architecture overview

flowd is built as a **local-first workflow discovery pipeline**.

It observes activity on your machine, converts events into structured data, detects repeated workflows, and proposes safe automations.

System flow:

```
Local events
    ↓
Adapters
    ↓
Raw events
    ↓
Normalization
    ↓
SQLite storage
    ↓
Session builder
    ↓
Pattern detection
    ↓
Suggestions
    ↓
CLI inspection + automation approval
```

The system is intentionally modular:

- **flow-adapters** — capture local system events
- **flow-analysis** — open-core analysis pipeline and the single optional intelligence client boundary
- **flow-core** — shared domain types
- **flow-db** — SQLite persistence and migrations
- **flow-patterns** — normalization, sessions, pattern detection
- **flow-cli** — command-line interface
- **flow-daemon** — background event processing
- **flow-dsl** — automation specification
- **flow-exec** — dry-run and execution engine

For the full architecture description see:

```
docs/architecture.md
```

---

## Current capabilities

- real filesystem event watcher
- SQLite event storage
- event normalization
- workflow session detection
- repeated pattern discovery
- pattern scoring
- suggestion generation
- CLI inspection tools
- safe filesystem automation engine (rename, move)

---

## Project principles

### Local-first

All data stays on your machine.

### Deterministic

The same input always produces the same detected workflows.

### Inspectable

All internal state can be inspected via SQLite.

### Safe

Early automation support focuses on safe filesystem operations only.

---

## Workspace crates

```
flow-core        shared domain types and configuration
flow-analysis    open-core analysis pipeline and the single optional intelligence client boundary
flow-daemon      background event watcher
flow-cli         command line interface
flow-db          SQLite persistence and migrations
flow-adapters    system event capture
flow-patterns    normalization, sessions, pattern detection
flow-dsl         automation specification
flow-exec        dry-run and execution
```

---

## 30‑second demo

Start the daemon that observes filesystem activity:

```bash
cargo run -p flow-daemon
```

Do your normal work for a moment (rename files, move downloads, organize folders).

Then inspect what flowd detected:

```bash
cargo run -p flow-cli patterns
cargo run -p flow-cli suggestions
cargo run -p flow-cli approve <suggestion_id>
cargo run -p flow-cli dry-run <automation_id>
cargo run -p flow-cli run <automation_id>
```

flowd will show workflows it discovered from your activity and propose potential automations.

This is the core loop of flowd:

```
observe → detect patterns → propose automations
```

---

## Quick start

Build the project:

```bash
cargo build
```

Run tests:

```bash
cargo test
```

Start the daemon:

```bash
cargo run -p flow-daemon
```

Inspect workflows with the CLI:

```bash
cargo run -p flow-cli patterns
cargo run -p flow-cli suggestions
cargo run -p flow-cli approve <suggestion_id>
cargo run -p flow-cli automations
cargo run -p flow-cli dry-run <automation_id>
cargo run -p flow-cli run <automation_id>
cargo run -p flow-cli sessions
```

---

## Inspecting the database

All state is stored locally in `flowd.db`.

Examples:

```bash
sqlite3 flowd.db "select * from raw_events limit 10;"
sqlite3 flowd.db "select * from normalized_events limit 10;"
sqlite3 flowd.db "select * from patterns limit 10;"
sqlite3 flowd.db "select * from suggestions limit 10;"
```

---

## Language policy

All documentation, code comments, commit messages, issue discussions, and contributor-facing text must be written in English.

This ensures:

- accessibility for global contributors
- better compatibility with AI coding agents
- consistent documentation quality

---

## Included planning files

- `PLAN.md`
- `TASKS.md`
- `PROMPTS_FOR_CODEX.md`
- `PRIVATE_CORE_BOUNDARY.md`

---

## Roadmap

flowd is being built incrementally with a focus on reliability, transparency, and local-first automation.

### Phase 1 — Observation (current)

- filesystem event watcher
- raw event persistence
- event normalization
- workflow session detection

### Phase 2 — Pattern discovery

- repeated workflow detection
- pattern scoring
- suggestion generation
- CLI inspection tools

### Phase 3 — Safe automation

- automation DSL
- dry-run execution engine
- explicit user approval for automations
- safe filesystem automation primitives

### Phase 4 — Expanded observation

- terminal command workflows
- editor and IDE integrations
- application event adapters

### Phase 5 — Intelligence layer

- pattern summarization
- automation refinement
- optional local LLM assistance

---

## Status

flowd is currently in **early development**.

The workflow detection pipeline is implemented and evolving toward safe local automation execution.

---

## License

MIT
