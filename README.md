# flowd

![Rust](https://img.shields.io/badge/Rust-stable-orange)
![License](https://img.shields.io/badge/license-MIT-blue)
![Local-first](https://img.shields.io/badge/local--first-yes-green)

Workflow automation that learns from how you work.

flowd observes local file workflows, detects repeated patterns, and suggests automations you can approve from the terminal.

- Local-first
- Terminal-first
- Deterministic automations
- Safe approvals with undo

## What is flowd?

flowd solves a simple problem: many file workflows are repetitive, but writing automation rules by hand is tedious. It watches local activity, recognizes repeated sequences such as rename and move actions, and turns them into suggestions you can inspect before anything is automated. Approved automations run locally, and the system remains useful without any cloud service or separate intelligence layer.

## Demo

### Example: automatically organizing invoices

You download invoice PDFs into `~/Downloads` and keep renaming and moving them into `~/Documents/Accounting/Invoices`.

1. flowd observes the repeated file actions in `~/Downloads`.
2. It detects that the same rename-and-move workflow keeps happening.
3. It creates a suggestion for that pattern.
4. You inspect the suggestion in the CLI.
5. You approve it and future invoices can be organized automatically.

```bash
$ flowctl suggestions

Suggestion 3:
Rename and move invoice PDFs to ~/Documents/Accounting/Invoices

$ flowctl approve 3

Approved suggestion 3 as automation 1
```

After approval, future matching files can be handled by the generated automation instead of repeating the same manual steps.

## Installation

### Install with cargo

If you are using a packaged release, install flowd with:

```bash
cargo install flowd
```

In this repository today, install the local binaries directly:

```bash
cargo install --path crates/flow-cli
cargo install --path crates/flow-daemon
```

### First-run setup

Create a config file and print the next commands to run:

```bash
flowctl setup
```

To pick the folders you want `flow-daemon` to watch from the start:

```bash
flowctl setup --watch ~/Downloads --watch ~/Desktop
```

If you want to write the config to a specific location, combine `setup` with `--config`:

```bash
flowctl --config ~/.config/flowd/config.toml setup --watch ~/Downloads
```

If the config already exists, `flowctl setup` will leave it unchanged unless you pass `--force`.

### Start the daemon

The daemon starts workflow observation and, by default, watches `~/Downloads`. It stores state locally in `./flowd.db`.

```bash
flow-daemon
```

From this repository, the current daemon binary is:

```bash
flow-daemon
```

`flowd` resolves configuration in this order:

1. `--config <path>` when provided to `flowctl` or `flow-daemon`
2. `./flowd.toml` in the current working directory
3. `$XDG_CONFIG_HOME/flowd/config.toml`
4. `~/.config/flowd/config.toml`
5. built-in defaults when no config file exists

If you want to change observed folders or runtime behavior, create a config file in one of those locations.

```toml
database_path = "./flowd.db"
observed_folders = ["~/Downloads"]
observe_clipboard = false
observe_terminal = true
observe_active_window = false
redact_clipboard_content = true
redact_command_args = true
strip_browser_query_strings = true
suggestion_min_usefulness_score = 0.0
intelligence_enabled = true
session_inactivity_secs = 300
file_event_dedup_window_ms = 500
```

You can inspect or validate the resolved config from the terminal:

```bash
flowctl config show
flowctl config validate
flowctl config path
```

### Inspect suggestions

```bash
flowctl suggestions
```

### Approve an automation

```bash
flowctl approve <id>
```

## Usage

The core loop is:

```text
observe -> detect patterns -> suggest automations -> approve
```

Useful commands:

```bash
flowctl stats
flowctl patterns
flowctl suggestions
flowctl approve <suggestion_id>
flowctl automations
flowctl dry-run <automation_id>
flowctl run <automation_id>
flowctl runs
flowctl undo <run_id>
```

All state stays local. You can inspect the SQLite database directly:

```bash
sqlite3 flowd.db "select * from patterns limit 10;"
sqlite3 flowd.db "select * from suggestions limit 10;"
sqlite3 flowd.db "select * from automations limit 10;"
```

## Architecture

flowd follows a local-first workflow architecture:

- Adapters capture local events.
- Core normalizes and persists them.
- Patterns detect repeated workflows.
- Suggestions propose automations.
- Automations execute approved workflows.

The main pipeline is:

```text
filesystem events
  -> normalized events
  -> sessions
  -> patterns
  -> suggestions
  -> approved automations
```

`flowd` is the open-core workflow engine. It owns event capture, persistence, sessions, pattern detection, baseline suggestions, automations, execution, undo, and explainability plumbing.

An optional private decision layer, `flowd-intelligence`, can improve suggestion quality through ranking, timing, suppression, personalization, clustering, wording, and display decisions. The integration direction is one-way:

```text
flowd -> flowd-intelligence
```

flowd remains fully functional without the intelligence layer.

Workspace crates:

```text
flow-core        shared domain types and configuration
flow-analysis    open-core analysis pipeline and intelligence boundary
flow-daemon      background observation
flow-cli         command-line interface
flow-db          SQLite persistence and migrations
flow-adapters    local event capture
flow-patterns    normalization, sessions, pattern detection
flow-dsl         automation specification
flow-exec        dry-run and execution
```

For more detail, see [docs/system-overview.md](/Users/nickvandort/Documents/Coding/flowd/docs/system-overview.md) and [docs/architecture.md](/Users/nickvandort/Documents/Coding/flowd/docs/architecture.md).

## Project principles

### Local-first

All data stays on your machine.

### Deterministic

The same input produces the same detected workflows.

### Inspectable

State is stored locally and can be inspected in SQLite.

### Safe

Automation focuses on explicit approval, dry runs, execution tracking, and undo.

## Language policy

All documentation, code comments, commit messages, issue discussions, and contributor-facing text must be written in English.

## Included planning files

- `PLAN.md`
- `TASKS.md`
- `PROMPTS_FOR_CODEX.md`
- `PRIVATE_CORE_BOUNDARY.md`

## Roadmap

flowd is being built incrementally with a focus on reliability, transparency, and local-first automation.

### Phase 1 - Observation

- filesystem event watcher
- raw event persistence
- event normalization
- workflow session detection

### Phase 2 - Pattern discovery

- repeated workflow detection
- pattern scoring
- suggestion generation
- CLI inspection tools

### Phase 3 - Safe automation

- automation DSL
- dry-run execution engine
- explicit user approval for automations
- safe filesystem automation primitives

### Phase 4 - Expanded observation

- terminal command workflows
- editor and IDE integrations
- application event adapters

### Phase 5 - Intelligence layer

- pattern summarization
- automation refinement
- optional local assistance

## Status

flowd is in early development. The workflow detection pipeline is implemented and evolving toward safe local automation execution.

## License

MIT
