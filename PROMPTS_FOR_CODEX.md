# PROMPTS_FOR_CODEX.md — ready-to-use Codex prompts for flowd

This file contains copy-paste prompts for Codex, organized by phase.
Each prompt is designed to be:
- narrowly scoped
- test-driven
- deterministic where possible
- aligned with the architecture in `PLAN.md`

Use one prompt per task or PR.

---

# Phase 0 — Repo foundation

## Prompt 01 — Initialize Rust workspace
```text
You are implementing the initial repository skeleton for a local-first Rust project called flowd.

Goal:
Create a compileable Rust workspace with the following crates:
- flow-core
- flow-daemon
- flow-cli
- flow-db
- flow-adapters
- flow-patterns
- flow-dsl
- flow-exec

Requirements:
- Use stable Rust
- flow-cli must expose a binary with --help output
- Each crate should compile with a minimal public API or placeholder main/lib
- Add a top-level README with a short description of the project
- Do not add any runtime behavior yet
- Keep the workspace clean and conventional

Acceptance criteria:
- cargo build succeeds
- cargo test succeeds
- cargo run -p flow-cli -- --help succeeds

Deliverables:
- workspace Cargo.toml
- per-crate Cargo.toml
- minimal src files
- top-level README
```

## Prompt 02 — Add config loader
```text
Implement configuration loading for flowd.

Context:
flowd is a local-first daemon + CLI that observes selected local workflows and suggests automations.

Task:
Add a TOML config loader with sensible defaults.

Requirements:
- Create a Config struct
- Include fields:
  - database_path
  - observed_folders
  - observe_clipboard
  - observe_terminal
  - observe_active_window
  - redact_clipboard_content
  - redact_command_args
  - strip_browser_query_strings
- Provide a default constructor
- Implement load_from_path(path)
- Use structured error handling
- Add unit tests for:
  - default config
  - valid override config
  - invalid config returns useful error

Constraints:
- No async required
- No watchers yet
- Keep types explicit and serializable

Acceptance criteria:
- cargo test passes
- config can be parsed from TOML
- invalid TOML and invalid field types are covered by tests
```

## Prompt 03 — Add SQLite schema and migration runner
```text
Implement SQLite initialization and migrations for flowd.

Task:
Create a migration runner and core schema for the following tables:
- raw_events
- normalized_events
- sessions
- session_events
- patterns
- suggestions
- automations
- automation_runs

Requirements:
- Use a simple migration strategy suitable for a local CLI/daemon project
- Add repository helpers for raw_events and normalized_events first
- Keep the schema explicit and easy to inspect
- Use a temporary SQLite database in tests

Tests:
- migration runner creates all tables
- insert/select round-trip for raw_events
- insert/select round-trip for normalized_events

Constraints:
- No ORM-heavy abstractions unless truly necessary
- Favor clarity over generic repository layers
```

---

# Phase 1 — Event ingestion

## Prompt 04 — Raw event model
```text
Implement the raw event model for flowd.

Goal:
Represent locally observed events before normalization.

Requirements:
- Add RawEvent struct
- Add EventSource enum with at least:
  - FileWatcher
  - Clipboard
  - Terminal
  - ActiveWindow
  - Browser
- Store payload as JSON
- Add helper methods for:
  - redacted payload view
  - serialization/deserialization
- Keep timestamps explicit
- Keep the model stable and easy to persist

Tests:
- serialize/deserialize round-trip
- redaction helper masks configured sensitive fields
- stable JSON shape snapshot or assertion test

Constraints:
- Do not add adapters yet
- Do not add normalization yet
```

## Prompt 05 — File watcher adapter
```text
Implement a file watcher adapter for flowd.

Goal:
Observe selected folders and emit raw file events.

Requirements:
- Watch one or more configured folders
- Emit events for:
  - file create
  - file rename
  - file move (as far as platform/library semantics allow)
- Translate observed changes into RawEvent values
- Make the watcher testable with temporary directories
- Keep the adapter boundary clean so the rest of the system is decoupled from the watcher library

Tests:
- integration test with temp directory captures file creation
- integration test captures rename
- emitted events can be stored via the raw event repository

Constraints:
- No normalization in this task
- No suggestion logic
- Avoid platform-specific complexity beyond what is required for tests
```

## Prompt 06 — Clipboard adapter abstraction
```text
Implement a clipboard adapter abstraction for flowd.

Goal:
Emit raw clipboard events in a privacy-conscious way.

Requirements:
- Support metadata-only mode
- Support optional content capture mode
- Include content length metadata
- Support redaction if content capture is enabled
- Expose a clean adapter interface suitable for daemon integration
- No cloud or remote behavior

Tests:
- metadata-only mode emits expected shape
- content capture mode emits expected shape
- redaction logic is covered

Constraints:
- If real clipboard observation is difficult cross-platform, design a provider abstraction and test against a fake provider
- Keep the API small
```

## Prompt 07 — Terminal command ingestion
```text
Add terminal command ingestion support to flowd.

Goal:
Ingest terminal command events from shell hooks or a simple stdin/file format.

Requirements:
- Define an input format suitable for shell integration
- Parse:
  - command
  - cwd
  - timestamp
  - exit code if present
- Support command argument redaction
- Convert parsed data into RawEvent
- Add persistence integration

Tests:
- parser handles valid input
- parser rejects malformed input cleanly
- redaction behavior covered
- repository integration test writes terminal raw events

Constraints:
- Do not build a full shell plugin system yet
- Keep the format simple and documented in code comments
```

## Prompt 08 — Active window tracking abstraction
```text
Implement active window tracking abstractions for flowd.

Goal:
Represent app focus changes and window title changes without locking into one platform implementation.

Requirements:
- Add a provider trait or interface
- Represent:
  - app switch
  - window title change
- Add debounce logic to suppress duplicate rapid events
- Emit RawEvent values
- Keep platform-specific code minimal or stubbed if necessary

Tests:
- debounce suppresses duplicate repeated events
- app switch and title change produce distinct raw event shapes

Constraints:
- This task is about the abstraction and event model boundary, not complete cross-platform support
```

## Prompt 09 — Browser bridge input
```text
Add a lightweight browser bridge input path to flowd.

Goal:
Allow browser URL/title events to enter the pipeline.

Requirements:
- Define a simple local input format for:
  - URL
  - tab title
  - timestamp
- Support optional query-string stripping
- Convert events into RawEvent
- Keep it local-first and suitable for use by a future browser extension or localhost bridge

Tests:
- URL event parsing
- query-string stripping behavior
- raw event serialization

Constraints:
- No extension implementation required
- No normalization in this task
```

---

# Phase 2 — Normalization and sessions

## Prompt 10 — Normalized event model
```text
Implement the normalized event model for flowd.

Goal:
Convert raw events into a stable action taxonomy that downstream logic can rely on.

Requirements:
- Add NormalizedEvent struct
- Add ActionType enum with:
  - open_app
  - switch_app
  - copy_text
  - paste_text
  - run_command
  - create_file
  - rename_file
  - move_file
  - visit_url
  - download_file
- Implement mappers from:
  - file raw events
  - clipboard raw events
  - terminal raw events
  - active window raw events
  - browser raw events
- Persist normalized events

Tests:
- unit tests for each mapper
- integration test writes normalized events to SQLite
- invalid or unsupported raw payloads fail gracefully

Constraints:
- No LLM logic
- Keep mapping deterministic
```

## Prompt 11 — Session builder
```text
Build the sessionization module for flowd.

Goal:
Group normalized events into workflow sessions.

Requirements:
- Consume normalized events in timestamp order
- Group into sessions using:
  - inactivity timeout
  - context switch boundaries
- Persist sessions and ordered session_events
- Keep logic deterministic and easy to replay
- Make thresholds configurable

Tests:
- fixture-driven tests for session boundaries
- event ordering preserved within sessions
- unrelated events become separate sessions

Constraints:
- No semantic clustering yet
- No suggestions yet
```

## Prompt 12 — Add fixture and replay harness
```text
Implement a replay harness and fixture format for flowd.

Goal:
Make the pipeline testable and reproducible with recorded or synthetic event sequences.

Requirements:
- Define a human-editable fixture format for raw or normalized events
- Add a replay utility that feeds events through the pipeline
- Add golden-file support for expected sessions and patterns
- Keep fixtures easy to read and modify

Tests:
- replay at least 3 fixture sets successfully
- golden-file comparisons for session output
- clear test failures when outputs drift

Constraints:
- Prefer simple JSON or YAML fixtures
- Avoid over-engineering a generic harness
```

---

# Phase 3 — Pattern detection

## Prompt 13 — Deterministic repeated pattern detector
```text
Implement a deterministic pattern detector for repeated workflows in flowd.

Goal:
Detect repeated workflows without using any LLM.

Input:
- normalized events grouped into sessions

Output:
- pattern candidates with:
  - signature
  - count
  - avg_duration_ms
  - suggestible bool

Requirements:
- detect repeated sequences across sessions
- allow small filename variations
- treat clearly noisy unrelated sessions as non-suggestible
- keep matching logic deterministic and testable

Tests:
- repeated invoice workflow fixture becomes one suggestible pattern
- noisy browsing fixture produces no suggestible patterns
- sequence count and duration are correct on fixtures

Constraints:
- No semantic clustering in this task
- No LLM usage
```

## Prompt 14 — Pattern ranking baseline
```text
Implement a baseline ranking function for pattern candidates in flowd.

Goal:
Rank detected patterns using deterministic signals before any private intelligence layer exists.

Requirements:
- Rank by a combination of:
  - frequency
  - average duration
  - recency
  - reversibility / safety bias
- Return a confidence-like score suitable for display
- Keep the formula explicit and testable
- Make weights configurable or centralized

Tests:
- fixture-based ranking test
- safer repeated file workflows rank above noisy or borderline workflows
- deterministic output ordering for equal-score tie-breaks

Constraints:
- No LLM usage
- No hidden heuristics outside tested code
```

## Prompt 15 — Safety filter for dangerous patterns
```text
Implement deterministic safety filtering for workflow suggestions in flowd.

Goal:
Prevent unsafe patterns from becoming executable automation suggestions.

Requirements:
- Explicitly suppress or block suggestions involving:
  - sudo
  - rm -rf
  - recursive deletes
  - privileged shell actions
  - clearly destructive file operations
- Unsafe patterns may still exist as observed patterns, but must not become executable suggestions
- Keep rules explicit and testable

Tests:
- dangerous shell fixture is suppressed
- safe git/cargo sequence still allowed as a macro candidate
- file rename/move workflows remain allowed

Constraints:
- No ML or LLM classification here
- Prefer allowlist + explicit deny rules
```

---

# Phase 4 — Suggestions and CLI

## Prompt 16 — Suggestion model and persistence
```text
Implement the suggestion model for flowd.

Goal:
Convert pattern candidates into persisted suggestion records.

Requirements:
- Add Suggestion struct
- Add statuses:
  - pending
  - applied
  - rejected
  - snoozed
- Include:
  - estimated savings
  - confidence
  - proposal payload or preview data
- Add repository support and state transitions

Tests:
- DB persistence tests
- status transition tests
- repeated state changes remain valid and explicit

Constraints:
- Keep the model independent from terminal rendering
```

## Prompt 17 — flowctl patterns
```text
Implement the flowctl patterns command.

Goal:
Display pattern candidates in a compact terminal-friendly format.

Requirements:
- Read patterns from SQLite
- Render:
  - label or fallback summary
  - count
  - avg duration
  - confidence or score
  - suggestible flag
- Sort by count descending, then avg duration descending unless a score exists
- Add snapshot tests for output

Constraints:
- No interactive UI
- No automation execution
- Keep the output compact and readable
```

## Prompt 18 — flowctl suggest
```text
Implement flowctl suggest.

Goal:
Display user-facing automation suggestions in the terminal.

Requirements:
- Read suggestions from SQLite
- Render:
  - label
  - frequency
  - avg duration
  - estimated savings
  - confidence
  - proposal preview
- Sort by confidence descending, then estimated savings descending
- Add snapshot tests for terminal output
- Use sensible fallback wording when labels are unavailable

Constraints:
- Do not execute automations
- No TUI
```

## Prompt 19 — flowctl status and tail
```text
Implement flowctl status and flowctl tail.

Goal:
Provide visibility into the daemon state and recently observed events.

Requirements:
- flowctl status should show:
  - database path
  - enabled observed sources
  - event counts or last activity summary
- flowctl tail should show recent raw or normalized events
- Add tests for formatting and basic filtering behavior

Constraints:
- Keep output useful for debugging
- No interactive streaming UI required
```

---

# Phase 5 — Automation DSL and execution

## Prompt 20 — Design automation DSL
```text
Design and implement a YAML-based internal DSL for flowd automations.

Goal:
Represent safe automations in a structured, reviewable, deterministic format.

Requirements:
- Support:
  - trigger
  - conditions
  - actions
  - safety settings
- Start with file rename and file move actions
- Add parser and validator
- Return structured errors for invalid specs

Tests:
- valid file automation spec parses successfully
- invalid spec cases fail with useful errors
- validator enforces required fields

Constraints:
- No delete actions
- No shell execution in this task
```

## Prompt 21 — Compile file suggestions to DSL
```text
Implement a compiler from repeated file workflow suggestions to the flowd automation DSL.

Goal:
Turn a repeated file workflow into a valid, executable DSL spec.

Requirements:
- Input is a file-oriented workflow suggestion
- Output is a valid DSL spec
- Support create/open/rename/move style repeated workflows
- Keep the compiler deterministic
- Emit clear defaults for safety settings

Tests:
- invoice workflow suggestion compiles to expected DSL structure
- unsupported suggestions fail cleanly

Constraints:
- File workflows only
- No LLM usage
```

## Prompt 22 — Dry-run executor
```text
Implement a dry-run executor for flowd automations.

Goal:
Preview intended file changes without mutating the filesystem.

Requirements:
- Support file rename and move actions
- Return structured preview output
- Integrate with the DSL parser/validator
- No filesystem mutations in dry-run mode

Tests:
- dry-run preview for invoice rule
- verify file system remains unchanged after dry-run
- preview structure is deterministic

Constraints:
- No delete support
```

## Prompt 23 — Real executor and undo log
```text
Implement the real executor and undo log for supported file automations in flowd.

Goal:
Safely execute rename/move automations and record enough information to undo them.

Requirements:
- Execute rename and move actions
- Persist automation_runs
- Persist undo payload per run
- Implement undo for supported actions
- Use temporary directories in integration tests

Tests:
- execution changes files as expected
- undo restores original state
- automation_runs contain useful audit information

Constraints:
- Do not support deletes
- Keep all actions explicit and reversible
```

## Prompt 24 — CLI apply/reject/snooze/undo
```text
Implement the approval loop commands for flowctl.

Goal:
Allow users to act on suggestions from the terminal.

Commands:
- flowctl apply <id>
- flowctl reject <id>
- flowctl snooze <id>
- flowctl undo <run-id>

Requirements:
- update suggestion status in SQLite
- apply should support --dry-run
- successful apply should create or run an automation
- undo should revert a supported run
- add CLI integration tests

Constraints:
- No interactive TUI required
- Keep errors explicit and user-friendly
```

---

# Phase 6 — Local LLM integration

## Prompt 25 — Local model bridge
```text
Implement a local model bridge for flowd.

Goal:
Use a local LLM runtime only for structured labeling and summarization.

Requirements:
- Add a provider interface for a local LLM runtime
- Accept prompt input and require structured JSON output
- Validate returned JSON against a schema
- Add deterministic fallback behavior if the model is unavailable or output is invalid
- Intended use cases:
  - pattern label
  - pattern summary
  - proposal wording

Tests:
- mocked provider success case
- invalid JSON rejected
- schema mismatch rejected
- fallback path covered

Constraints:
- The model must never execute actions
- The model must not bypass safety filters
- Keep the interface small and testable
```

## Prompt 26 — Semantic clustering
```text
Implement semantic clustering for repeated workflows in flowd.

Goal:
Group similar repeated workflows into one conceptual pattern.

Requirements:
- handle variable filenames and minor path differences
- keep unrelated workflows separate
- support deterministic testability
- work with canonical summaries or structured workflow representations
- if using a model, keep it behind the provider interface and preserve deterministic fallback behavior

Tests:
- variable invoice fixtures cluster together
- unrelated workflows do not merge
- cluster output remains stable on repeated runs

Constraints:
- Do not let clustering change execution safety
```

## Prompt 27 — Proposal generation helper
```text
Implement proposal generation helpers for flowd suggestions.

Goal:
Turn pattern candidates into user-facing labels, summaries, and proposal previews.

Requirements:
- Prefer deterministic generation when possible
- Optionally use local model bridge for wording improvements
- Always keep a deterministic fallback
- Output should include:
  - short label
  - one-paragraph summary
  - one automation preview block

Tests:
- deterministic fallback output for known fixture patterns
- invalid model output falls back safely
- proposal text does not include unsupported actions

Constraints:
- No execution logic here
- Safety constraints remain controlled outside the model
```

---

# Phase 7 — Learning and anti-annoyance

## Prompt 28 — Feedback memory
```text
Implement feedback memory for flowd suggestions.

Goal:
Adjust future suggestion behavior based on accept/reject/snooze history.

Requirements:
- persist feedback history
- rejected suggestions should be suppressed for a configurable window
- accepted categories may receive a ranking boost
- snoozed suggestions should reappear only after their cooldown
- keep logic deterministic and explicit

Tests:
- rejection suppression
- acceptance prioritization
- snooze cooldown respected

Constraints:
- No model training
- No hidden heuristics outside tested code
```

## Prompt 29 — Anti-annoyance policy
```text
Implement anti-annoyance policy logic for flowd.

Goal:
Prevent the tool from becoming noisy or intrusive.

Requirements:
- configurable max suggestions per day
- duplicate suggestion suppression
- cool-down windows after reject or snooze
- deterministic behavior
- integrate policy checks before suggestions are shown

Tests:
- daily cap enforced
- duplicate suppression works
- cooldown respected
- safe high-confidence suggestions still show when allowed

Constraints:
- Keep the policy implementation simple and testable
```

---

# Phase 8 — Extended capabilities

## Prompt 30 — Terminal macro suggestions
```text
Extend flowd to propose terminal macros for safe repeated command sequences.

Goal:
Suggest macros for repeated safe terminal workflows.

Requirements:
- detect repeated safe command sequences
- create a suggestion record for a macro candidate
- integrate with safety filtering
- do not auto-execute macros
- include a preview of the macro sequence

Tests:
- safe git/cargo workflow becomes a macro suggestion
- dangerous command sequences remain suppressed
- duplicate macro suggestions are controlled by anti-annoyance policy

Constraints:
- No automatic shell execution in this task
```

## Prompt 31 — Browser visit normalization
```text
Extend normalization to better handle browser context.

Goal:
Produce useful visit_url and browser-related normalized events for future workflow detection.

Requirements:
- normalize URL/title browser events
- support optional domain-only or path-aware normalization modes
- keep privacy settings respected
- add tests for noisy navigation vs repeated meaningful visits

Constraints:
- No browser extension implementation
- No form-filling or browser automation yet
```

## Prompt 32 — Observed zones and privacy guards
```text
Implement observed zones and privacy guardrails for flowd.

Goal:
Give users explicit control over what is observed and what is redacted.

Requirements:
- support include/exclude rules for folders and sources
- support content redaction for clipboard and terminal args
- support query-string stripping for browser events
- ensure event capture respects the config at ingest time

Tests:
- excluded folder events are ignored
- clipboard metadata-only mode works
- terminal args redacted when enabled
- browser query strings stripped when configured

Constraints:
- Keep behavior explicit and config-driven
```

---

# End-to-end prompts

## Prompt 33 — MVP end-to-end pipeline
```text
Create an MVP end-to-end test suite for flowd.

Scenario:
- repeated invoice workflow events are ingested
- normalized
- sessionized
- turned into a repeated pattern
- converted to a suggestion
- applied in dry-run
- executed for real
- undone successfully

Requirements:
- automated tests only
- use fixture-driven inputs where practical
- include CLI snapshot coverage where useful
- produce a short release checklist document

Constraints:
- no cloud dependencies
- no unsupported action types
- keep the test readable and maintainable
```

## Prompt 34 — Documentation pack
```text
Write developer-facing documentation for flowd.

Files:
- docs/architecture.md
- docs/event-model.md
- docs/automation-dsl.md
- docs/privacy.md

Requirements:
- reflect the current implementation accurately
- include concrete examples
- explain where deterministic logic ends and optional local-model assistance begins
- do not mention unsupported features as if they already exist

Constraints:
- concise but useful
- no marketing fluff
```

## Prompt 35 — Release checklist and contributor bootstrap
```text
Create release and contributor bootstrap documentation for flowd.

Deliverables:
- RELEASE_CHECKLIST.md
- CONTRIBUTING.md
- examples/config.sample.toml
- fixtures/README.md

Requirements:
- release checklist should cover:
  - tests
  - fixture replay
  - safety review
  - dry-run validation
  - undo validation
- contributing guide should explain workspace layout and how to run tests
- sample config should match current implementation
- fixtures README should explain how to replay included examples

Constraints:
- keep docs concrete and aligned with code
```

---

# Prompt templates for private intelligence crate

Use these only in your private repo or private crate.

## Template A — Ranking policy
```text
Implement a proprietary ranking policy module for flowd suggestions.

Goal:
Rank suggestion candidates using signals such as:
- frequency
- recency
- duration
- reversibility
- category preference
- prior feedback
- annoyance suppression

Requirements:
- keep the implementation modular
- expose a tested ranking interface
- do not leak private heuristics into public docs
- add deterministic tests using fixture data

Constraints:
- safety filters still override ranking
- no external services
```

## Template B — Proposal wording engine
```text
Implement a proprietary proposal wording engine for flowd.

Goal:
Generate high-quality labels and summaries for suggestions while preserving safety constraints.

Requirements:
- accept structured pattern data
- return:
  - label
  - short summary
  - one automation preview block
- support deterministic fallback output
- integrate with local model bridge if available

Constraints:
- never invent unsupported actions
- no execution
- keep wording concise and actionable
```

## Template C — Suggestion timing and suppression
```text
Implement a proprietary suggestion timing engine for flowd.

Goal:
Decide when a suggestion should be shown to the user.

Signals to consider:
- confidence
- estimated savings
- recency
- prior reject/snooze
- category saturation
- daily cap
- duplicate similarity

Requirements:
- explicit API
- deterministic tests
- easy to simulate on fixture history

Constraints:
- no UI logic
- no external services
```

---

# Recommended working method with Codex

1. Use one prompt per branch or PR.
2. Require tests in every prompt.
3. Keep prompts narrow and avoid architectural redesign mid-task.
4. Re-run fixture replay tests frequently.
5. Treat local-model output as optional metadata, never as the source of truth for safety or execution.
