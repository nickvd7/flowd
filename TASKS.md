# TASKS.md — Codex task plan for flowd

Each task below is designed to be small enough for Codex to handle cleanly.
For every task:
- keep scope tight
- require tests
- avoid adding unrelated abstractions
- prefer deterministic behavior
- avoid cloud dependencies

---

## Task 01 — Workspace scaffold

### Goal
Create a Rust workspace for `flowd` with core crates and a minimal compileable skeleton.

### Deliverables
- workspace `Cargo.toml`
- crates:
  - `flow-core`
  - `flow-daemon`
  - `flow-cli`
  - `flow-db`
  - `flow-adapters`
  - `flow-patterns`
  - `flow-dsl`
  - `flow-exec`
- placeholder README files

### Acceptance criteria
- `cargo build` passes
- `cargo test` passes
- `flow-cli` binary runs with `--help`

### Codex prompt
```text
Create a Rust workspace for a project called flowd.

Requirements:
- crates: flow-core, flow-daemon, flow-cli, flow-db, flow-adapters, flow-patterns, flow-dsl, flow-exec
- flow-cli should expose a binary with --help output
- use stable Rust
- add minimal lib.rs or main.rs files as needed
- no external runtime behavior yet

Acceptance:
- cargo build succeeds
- cargo test succeeds
```

---

## Task 02 — Config loader

### Goal
Implement configuration loading from TOML with sensible defaults.

### Deliverables
- `Config` struct
- default config file support
- observed zones configuration
- redaction settings
- database path config

### Acceptance criteria
- parse valid TOML
- invalid config returns structured error
- tests cover defaults and overrides

### Codex prompt
```text
Implement a TOML config loader for flowd.

Requirements:
- Config struct with:
  - database_path
  - observed_folders
  - observe_clipboard
  - observe_terminal
  - observe_active_window
  - redaction options
- provide defaults
- implement load_from_path
- write unit tests for:
  - default config
  - override config
  - invalid config

Do not add runtime watchers yet.
```

---

## Task 03 — SQLite migrations and repositories

### Goal
Create DB initialization and repositories for raw and normalized events.

### Deliverables
- migration runner
- tables:
  - raw_events
  - normalized_events
  - sessions
  - session_events
  - patterns
  - suggestions
  - automations
  - automation_runs
- repository interfaces

### Acceptance criteria
- migration test on temporary DB passes
- insert/select tests pass

### Codex prompt
```text
Add SQLite persistence for flowd.

Requirements:
- create migrations for the core tables
- add Rust repository methods for raw_events and normalized_events first
- use a temporary SQLite database in tests
- keep schema simple and explicit

Acceptance:
- migrations run successfully
- tests insert and read back sample rows
```

---

## Task 04 — Raw event model

### Goal
Define raw event types and serialization boundaries.

### Deliverables
- `RawEvent` struct
- `EventSource` enum
- redacted payload support
- JSON serialization helpers

### Acceptance criteria
- serialize/deserialize tests pass
- payload redaction tests pass

### Codex prompt
```text
Implement the raw event model for flowd.

Requirements:
- RawEvent struct
- EventSource enum
- payload stored as JSON
- helper to redact sensitive fields
- tests for serialization and redaction

Do not implement adapters yet.
```

---

## Task 05 — File watcher adapter

### Goal
Capture file system events from observed folders.

### Deliverables
- watcher for create/rename/move events
- emits `RawEvent`
- testable adapter abstraction

### Acceptance criteria
- integration test with temp directory captures file create and rename
- emitted events persist to DB via repository mock or test DB

### Codex prompt
```text
Implement a file watcher adapter for flowd.

Requirements:
- observe one or more configured folders
- emit raw events for file create, rename, and move
- make the watcher testable with temp directories
- add integration tests

Do not implement normalization here.
```

---

## Task 06 — Clipboard adapter

### Goal
Capture clipboard changes in a minimal, optional way.

### Deliverables
- clipboard adapter interface
- content length metadata
- optional content capture with redaction mode

### Acceptance criteria
- unit tests for emitted event shape
- content redaction behavior covered

### Codex prompt
```text
Implement a clipboard adapter abstraction for flowd.

Requirements:
- emit raw events when clipboard content changes
- support metadata-only mode
- support optional content capture with redaction
- design for local-only usage

Tests:
- event creation
- redaction behavior
- metadata-only mode
```

---

## Task 07 — Terminal hook ingestion

### Goal
Support ingestion of terminal command events.

### Deliverables
- shell hook input format
- parser for command events
- event persistence

### Acceptance criteria
- parser handles command, cwd, timestamp
- tests for redacted arguments
- integration test writes terminal raw events

### Codex prompt
```text
Add terminal command ingestion to flowd.

Requirements:
- define an input format suitable for shell hooks
- parse command, cwd, exit code if available, timestamp
- support argument redaction
- persist as raw events

Tests:
- parser tests
- repository integration test
```

---

## Task 08 — Active window adapter

### Goal
Track active app and window title changes.

### Deliverables
- adapter trait
- event model for focus changes
- debounce logic

### Acceptance criteria
- unit tests for debounce behavior
- raw events created for app switch and title change

### Codex prompt
```text
Implement active window tracking abstractions for flowd.

Requirements:
- represent app focus changes and window title changes
- include debounce logic to avoid excessive duplicate events
- write tests for duplicate suppression

Do not add platform-specific code beyond a stubbed provider interface unless needed.
```

---

## Task 09 — Normalized event model

### Goal
Convert raw events into a stable action taxonomy.

### Deliverables
- `NormalizedEvent`
- `ActionType` enum
- mappers for file, clipboard, terminal, and active-window events

### Acceptance criteria
- unit tests for mappings
- integration test writes normalized events to SQLite

### Codex prompt
```text
Implement the normalized event model for flowd.

Requirements:
- add ActionType enum with:
  open_app, switch_app, copy_text, paste_text, run_command,
  create_file, rename_file, move_file, visit_url, download_file
- implement mapping from raw file, clipboard, terminal, and active-window events
- persist normalized events
- include tests

Do not add LLM logic.
```

---

## Task 10 — Session builder

### Goal
Group normalized events into workflow sessions.

### Deliverables
- time-window sessionization
- context-switch boundaries
- session persistence

### Acceptance criteria
- fixture tests verify session boundaries
- sessions persist with event ordering

### Codex prompt
```text
Build a sessionization module for flowd.

Requirements:
- consume normalized events in timestamp order
- group into sessions using inactivity windows and context switches
- persist sessions and session_events
- keep logic deterministic

Tests:
- fixture-driven session boundary tests
- ordering tests
```

---

## Task 11 — Deterministic pattern detector

### Goal
Detect repeated workflows without any LLM.

### Deliverables
- pattern signatures
- repeated-sequence counting
- duration estimates
- suggestible boolean

### Acceptance criteria
- repeated invoice workflow becomes one pattern
- noisy browsing produces no suggestible patterns
- tests use fixtures

### Codex prompt
```text
Build a deterministic pattern detector for repeated workflows.

Input:
- normalized events grouped into sessions

Output:
- pattern candidates with:
  - signature
  - count
  - avg_duration_ms
  - suggestible bool

Rules:
- detect repeated sequences across sessions
- allow small filename variations
- keep logic deterministic
- no LLM usage

Tests:
- include a repeated invoice fixture
- include a noisy unrelated fixture
- return no suggestible pattern for noise
```

---

## Task 12 — Pattern fixtures and replay harness

### Goal
Add a fixture system and replay runner for reproducible tests.

### Deliverables
- fixture format
- replay utility
- golden output support

### Acceptance criteria
- can replay at least 3 fixture sets
- golden tests compare expected patterns and sessions

### Codex prompt
```text
Implement a replay and fixture harness for flowd.

Requirements:
- fixture format for raw or normalized events
- replay utility that feeds events through the pipeline
- golden-file test support for sessions and patterns
- keep fixtures easy to read and edit

Tests:
- replay 3 fixture sets successfully
```

---

## Task 13 — Suggestion model

### Goal
Convert pattern candidates into suggestion records.

### Deliverables
- `Suggestion` struct
- statuses: pending, applied, rejected, snoozed
- estimated savings
- confidence placeholder

### Acceptance criteria
- suggestions persist to DB
- tests verify status transitions

### Codex prompt
```text
Implement the suggestion model for flowd.

Requirements:
- Suggestion struct and persistence
- statuses: pending, applied, rejected, snoozed
- include estimated savings and confidence fields
- support status transitions

Tests:
- DB persistence tests
- status transition tests
```

---

## Task 14 — `flowctl patterns`

### Goal
Render top pattern candidates in the terminal.

### Deliverables
- `flowctl patterns`
- sorting by frequency and duration
- compact terminal output

### Acceptance criteria
- snapshot tests for rendering
- ranking stable on fixture data

### Codex prompt
```text
Implement the flowctl patterns command.

Requirements:
- read pattern candidates from SQLite
- render compact terminal output
- sort by count descending then avg duration descending
- add snapshot tests for output

Do not implement interactive UI.
```

---

## Task 15 — `flowctl suggest`

### Goal
Render user-facing suggestions in the terminal.

### Deliverables
- suggestion listing command
- proposal preview
- confidence and savings output

### Acceptance criteria
- snapshot tests pass
- suggestions rank by confidence then time savings

### Codex prompt
```text
Implement flowctl suggest.

Requirements:
- read suggestions from SQLite
- render:
  - label
  - frequency
  - avg duration
  - estimated savings
  - confidence
  - proposal preview
- sort by confidence descending, then estimated savings descending
- add snapshot tests

Do not execute automations here.
```

---

## Task 16 — Automation DSL

### Goal
Design a YAML-based internal DSL for safe automations.

### Deliverables
- DSL schema
- parser
- validator

### Acceptance criteria
- valid specs parse
- invalid specs fail with helpful errors
- tests cover file rename and move actions

### Codex prompt
```text
Design and implement a YAML-based automation DSL for flowd.

Requirements:
- support triggers, conditions, actions, and safety fields
- start with file rename and file move actions
- parser and validator required
- invalid specs should return structured errors

Tests:
- valid file automation spec
- invalid spec cases
```

---

## Task 17 — Proposal-to-DSL compiler

### Goal
Turn repeated file workflow suggestions into executable DSL specs.

### Deliverables
- compiler from pattern/suggestion to DSL
- file workflow support only

### Acceptance criteria
- repeated invoice workflow compiles to valid DSL
- tests verify exact generated structure

### Codex prompt
```text
Implement a compiler from file workflow suggestions to the flowd automation DSL.

Requirements:
- input: a repeated file workflow suggestion
- output: valid DSL spec
- start with workflows involving create/open/rename/move file patterns only
- keep compiler deterministic

Tests:
- invoice workflow compiles to expected DSL
```

---

## Task 18 — Dry-run executor

### Goal
Preview what an automation would do without changing files.

### Deliverables
- dry-run execution engine
- preview output
- structured result object

### Acceptance criteria
- dry-run lists predicted rename/move actions
- no file mutations occur
- tests verify unchanged file system state

### Codex prompt
```text
Implement a dry-run executor for flowd automations.

Requirements:
- support file rename and move actions
- return a structured preview of intended changes
- do not mutate the file system in dry-run mode

Tests:
- dry-run output for invoice rule
- verify files remain unchanged
```

---

## Task 19 — Real executor + undo log

### Goal
Execute safe file automations and record undo data.

### Deliverables
- executor for rename/move
- undo payload generation
- undo command support

### Acceptance criteria
- execution mutates files as expected
- undo restores original state
- integration tests use temp directories

### Codex prompt
```text
Implement the real file automation executor and undo log for flowd.

Requirements:
- support rename and move actions
- persist undo information for each run
- implement undo for supported actions
- use temp directories in integration tests

Safety:
- do not support deletes
```

---

## Task 20 — `flowctl apply`, `reject`, `snooze`, `undo`

### Goal
Complete the approval loop from suggestion to execution.

### Deliverables
- apply command
- reject command
- snooze command
- undo command

### Acceptance criteria
- suggestion state transitions persist
- apply can trigger dry-run or real execution
- undo works for completed supported runs

### Codex prompt
```text
Implement flowctl apply, reject, snooze, and undo.

Requirements:
- update suggestion states in SQLite
- apply should support --dry-run
- successful apply should create or run an automation
- undo should revert a supported automation run

Tests:
- end-to-end CLI integration tests
```

---

## Task 21 — Safety filter for dangerous shell patterns

### Goal
Prevent unsafe suggestions from being proposed.

### Deliverables
- safety classifier rules
- detection of dangerous shell commands
- suppression of unsafe suggestion creation

### Acceptance criteria
- fixtures containing sudo, rm -rf, or destructive patterns never become executable suggestions
- tests cover positive and negative cases

### Codex prompt
```text
Implement deterministic safety filtering for shell-related workflow suggestions.

Requirements:
- block suggestions involving sudo, rm -rf, recursive deletes, and similar destructive patterns
- keep rules explicit and testable
- unsafe patterns may be recorded, but must not become executable suggestions

Tests:
- dangerous fixtures are suppressed
- safe terminal macro fixtures still allowed
```

---

## Task 22 — Local model bridge

### Goal
Add a local LLM bridge for structured labeling only.

### Deliverables
- provider interface
- local runner integration abstraction
- JSON schema validation
- fallback behavior when model unavailable

### Acceptance criteria
- mocked provider tests pass
- invalid model output is rejected
- deterministic fallback summary exists

### Codex prompt
```text
Implement a local model bridge for flowd.

Requirements:
- provider interface for a local LLM runtime
- accept prompt input and return structured JSON only
- validate output schema
- add deterministic fallback if model is unavailable or invalid

Use cases:
- pattern label
- pattern summary
- proposal wording

Do not allow the model to execute actions.
```

---

## Task 23 — Semantic clustering

### Goal
Group similar repeated workflows into one conceptual pattern.

### Deliverables
- canonical workflow summary
- similarity scoring
- cluster merge logic

### Acceptance criteria
- variable invoice filenames cluster together
- unrelated workflows stay separate
- fixture tests pass

### Codex prompt
```text
Implement semantic clustering for repeated workflows.

Requirements:
- group similar file workflows into a single conceptual pattern
- handle variable filenames and minor path differences
- keep unrelated workflows separate
- support deterministic testing

Tests:
- variable invoice fixtures cluster together
- unrelated fixtures do not merge
```

---

## Task 24 — Feedback memory

### Goal
Use accept/reject/snooze history to adjust future suggestion behavior.

### Deliverables
- feedback persistence
- suppression windows
- category preference memory

### Acceptance criteria
- rejected suggestions become less likely to reappear immediately
- accepted suggestion categories rank higher
- tests verify threshold adjustments

### Codex prompt
```text
Implement feedback memory for flowd suggestions.

Requirements:
- persist accept, reject, and snooze history
- rejected suggestions should be suppressed for a configurable window
- accepted categories may receive ranking boosts
- keep logic deterministic and testable

Tests:
- rejection suppression
- acceptance prioritization
```

---

## Task 25 — Anti-annoyance policy

### Goal
Prevent the tool from overwhelming the user.

### Deliverables
- max suggestions per day
- duplicate suggestion suppression
- cool-down windows

### Acceptance criteria
- too many similar suggestions are capped
- duplicate proposals are suppressed
- tests cover daily caps and cooldowns

### Codex prompt
```text
Implement anti-annoyance policy logic for flowd.

Requirements:
- configurable max suggestions per day
- duplicate suggestion suppression
- cooldown windows after snooze or reject
- deterministic behavior

Tests:
- daily cap enforced
- duplicate suppression works
- cooldown respected
```

---

## Task 26 — Browser event bridge

### Goal
Add lightweight browser context ingestion.

### Deliverables
- input format for URL/title events
- raw event mapping
- optional query-string stripping

### Acceptance criteria
- visit_url events normalize correctly
- tests cover stripped and unstripped URL modes

### Codex prompt
```text
Add a browser event bridge to flowd.

Requirements:
- support URL and tab title event ingestion
- optional stripping of query strings
- emit raw events and normalized visit_url events
- keep integration lightweight and local-first

Tests:
- URL ingestion
- stripping behavior
- normalization
```

---

## Task 27 — Terminal macro proposal support

### Goal
Suggest terminal command sequences as macros.

### Deliverables
- repeated command sequence detection
- macro suggestion format
- safety guard integration

### Acceptance criteria
- safe repeated Git/Cargo workflow becomes a macro suggestion
- dangerous command flows are still blocked

### Codex prompt
```text
Extend flowd to propose terminal macros for safe repeated command sequences.

Requirements:
- detect repeated safe command sequences
- generate a terminal macro suggestion record
- integrate with safety filtering
- do not auto-execute

Tests:
- safe git/cargo sequence becomes a suggestion
- dangerous sequences remain suppressed
```

---

## Task 28 — Docs pack

### Goal
Write developer-facing docs for architecture, privacy, and extension points.

### Deliverables
- `docs/architecture.md`
- `docs/event-model.md`
- `docs/automation-dsl.md`
- `docs/privacy.md`

### Acceptance criteria
- docs match implemented interfaces
- docs include examples
- docs do not mention unsupported features

### Codex prompt
```text
Write developer-facing documentation for flowd.

Files:
- docs/architecture.md
- docs/event-model.md
- docs/automation-dsl.md
- docs/privacy.md

Requirements:
- reflect the actual current implementation
- include examples
- keep docs concrete, not aspirational
```

---

## Task 29 — Example configs and fixtures

### Goal
Provide usable starter files for contributors.

### Deliverables
- sample config
- sample fixtures
- fixture README

### Acceptance criteria
- fixture replay works on included examples
- sample config validates

### Codex prompt
```text
Add example configs and fixture sets to flowd.

Requirements:
- provide a sample TOML config
- add fixture sets for:
  - invoice workflow
  - noisy browsing
  - terminal macro
  - dangerous shell
- add README explaining how to replay fixtures

Acceptance:
- fixture replay works with included examples
- sample config validates
```

---

## Task 30 — Release-ready MVP check

### Goal
Add end-to-end tests for the MVP slice.

### Deliverables
- full pipeline integration test
- CLI snapshot tests
- executor + undo test
- release checklist doc

### Acceptance criteria
- repeated invoice workflow reaches terminal suggestion
- apply dry-run works
- real execute + undo works
- all tests pass in CI

### Codex prompt
```text
Create an MVP end-to-end test suite for flowd.

Scenario:
- repeated invoice workflow events are ingested
- normalized
- sessionized
- turned into a pattern
- converted to a suggestion
- applied in dry-run
- executed for real
- undone successfully

Requirements:
- automated tests only
- include CLI snapshot coverage where useful
- produce a short release checklist doc
```

---

## Suggested implementation order
1. Task 01
2. Task 02
3. Task 03
4. Task 04
5. Task 05
6. Task 07
7. Task 09
8. Task 10
9. Task 11
10. Task 12
11. Task 13
12. Task 14
13. Task 15
14. Task 16
15. Task 17
16. Task 18
17. Task 19
18. Task 20
19. Task 21
20. Task 22
21. Task 23
22. Task 24
23. Task 25
24. Task 26
25. Task 27
26. Task 28
27. Task 29
28. Task 30

---

## Notes for Codex usage
- Keep each task in a separate PR or branch.
- Always include fixture-driven tests.
- Prefer adding one module and one command at a time.
- Do not let Codex redesign the architecture mid-stream.
- Use the deterministic pipeline as the source of truth; treat LLM output as optional metadata.

# TASKS.md — Public contributor roadmap for flowd

This file tracks the **next meaningful work** for the open-source `flowd` repository.

It is intentionally focused on:
- small, reviewable tasks
- deterministic behavior
- local-first design
- safe automation boundaries
- contributor-friendly scope

Internal planning, private intelligence work, and experimental prompts should live outside this public repo.

---

## Current implementation status

The following foundations already exist in the repository:

- Rust workspace and crate structure
- TOML configuration loading
- SQLite schema and repositories
- filesystem watcher
- raw event persistence
- normalized event persistence
- session building
- repeated pattern detection
- suggestion generation
- automation approval flow
- dry-run execution
- safe execution for rename and move
- undo support
- CLI inspection commands

This roadmap therefore focuses on the **next open-core improvements**, not the initial scaffolding work.

---

## Task 01 — Automatic pattern refresh hardening

### Goal
Make the analysis pipeline more robust when new normalized events arrive.

### Deliverables
- deterministic refresh path for sessions, patterns, and suggestions
- clear boundaries between observation, analysis, and execution
- tests for repeated refresh behavior

### Acceptance criteria
- repeated refreshes do not create duplicate stale rows
- `cargo build` passes
- `cargo test` passes

---

## Task 02 — Suggestion cleanup and scoring

### Goal
Improve suggestion quality by ranking them and cleaning up stale suggestions.

### Deliverables
- deterministic suggestion score
- freshness tracking
- stale suggestion handling
- cleaner CLI output

### Acceptance criteria
- stale suggestions do not accumulate
- suggestions are ranked by explicit deterministic signals
- approved automations remain untouched
- `cargo build` passes
- `cargo test` passes

---

## Task 03 — Automation status management

### Goal
Allow automations to be enabled, disabled, and inspected by status.

### Deliverables
- automation status fields and persistence
- CLI support for:
  - `flow-cli enable <automation_id>`
  - `flow-cli disable <automation_id>`
- status shown in `flow-cli automations`

### Acceptance criteria
- disabled automations cannot be executed
- active automations still support dry-run and run
- `cargo build` passes
- `cargo test` passes

---

## Task 04 — Undo support hardening

### Goal
Make undo support safer and more transparent.

### Deliverables
- improved execution metadata for reversible runs
- `flow-cli runs`
- `flow-cli undo <run_id>`
- tests for reverse-order execution and safe aborts

### Acceptance criteria
- rename and move runs can be undone safely
- unsafe or incomplete runs fail clearly
- no bulk or implicit undo behavior exists
- `cargo build` passes
- `cargo test` passes

---

## Task 05 — CLI output polish

### Goal
Make CLI inspection commands more useful for real users.

### Deliverables
- clearer table-style output for:
  - `flow-cli patterns`
  - `flow-cli suggestions`
  - `flow-cli sessions`
  - `flow-cli automations`
  - `flow-cli runs`
- support for displaying status, confidence, freshness, and execution summaries where helpful

### Acceptance criteria
- output is human-readable and compact
- snapshot or formatting tests exist where practical
- `cargo build` passes
- `cargo test` passes

---

## Task 06 — Ranking and confidence improvements

### Goal
Make suggestions and patterns more useful by improving deterministic ranking.

### Deliverables
- explicit scoring model based on:
  - repetition count
  - recency
  - duration / estimated savings
  - safety bias
- sorting improvements in CLI output

### Acceptance criteria
- ranking is explicit and testable
- suggestions feel more relevant on repeated fixture data
- `cargo build` passes
- `cargo test` passes

---

## Task 07 — Terminal workflow ingestion

### Goal
Expand observation beyond filesystem workflows.

### Deliverables
- terminal event ingestion format
- terminal raw event persistence
- normalization into terminal-related `NormalizedEvent` values
- deterministic tests

### Acceptance criteria
- terminal events can be stored and normalized locally
- no automatic shell execution exists
- `cargo build` passes
- `cargo test` passes

---

## Task 08 — Clipboard workflow ingestion

### Goal
Support optional clipboard event observation with privacy-safe defaults.

### Deliverables
- clipboard adapter
- metadata-only mode
- optional redacted content capture
- normalization support where appropriate

### Acceptance criteria
- clipboard events can be captured locally
- privacy settings are respected
- `cargo build` passes
- `cargo test` passes

---

## Task 09 — Browser event bridge

### Goal
Add lightweight browser context ingestion for workflow discovery.

### Deliverables
- local input path for URL/title events
- optional query-string stripping
- normalization into `visit_url`

### Acceptance criteria
- URL events can be stored and normalized
- privacy settings are respected
- `cargo build` passes
- `cargo test` passes

---

## Task 10 — Fixtures and replay harness refresh

### Goal
Keep the test harness aligned with the live pipeline.

### Deliverables
- updated fixture sets for filesystem workflows
- replay utility for event sequences
- golden tests for sessions and patterns

### Acceptance criteria
- fixture replay remains easy to run
- sessions and patterns remain reproducible in tests
- `cargo test` passes

---

## Task 11 — Documentation polish

### Goal
Keep the public docs aligned with the current implementation.

### Deliverables
- refresh:
  - `README.md`
  - `docs/architecture.md`
  - `docs/architecture-diagram.md`
  - `docs/automation-dsl.md`
  - `docs/privacy.md`
- ensure examples match current CLI behavior

### Acceptance criteria
- docs reflect implemented commands and workflow
- no outdated `flowctl` references remain
- no unsupported features are described as complete

---

## Task 12 — Release-ready open-core MVP check

### Goal
Verify that the current open-core workflow loop is stable.

### Deliverables
- end-to-end integration test for:
  - observe
  - detect
  - suggest
  - approve
  - dry-run
  - run
  - undo
- release checklist for the public repo

### Acceptance criteria
- the open-core loop is covered by automated tests
- `cargo build` passes
- `cargo test` passes

---

## Suggested implementation order

1. Task 01 — Automatic pattern refresh hardening
2. Task 02 — Suggestion cleanup and scoring
3. Task 03 — Automation status management
4. Task 04 — Undo support hardening
5. Task 05 — CLI output polish
6. Task 06 — Ranking and confidence improvements
7. Task 10 — Fixtures and replay harness refresh
8. Task 11 — Documentation polish
9. Task 12 — Release-ready open-core MVP check
10. Task 07 — Terminal workflow ingestion
11. Task 08 — Clipboard workflow ingestion
12. Task 09 — Browser event bridge

---

## Notes for contributors

- Keep changes small and focused.
- Prefer one task per branch and PR.
- Always include tests when behavior changes.
- Treat deterministic logic as the source of truth.
- Keep open-core automation safe, inspectable, and local-first.