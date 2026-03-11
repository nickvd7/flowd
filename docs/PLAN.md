# PLAN.md — flowd

## Project
**flowd** is a local-first workflow observer and automation suggester.

It runs as a background daemon, captures selected local events, normalizes them into workflow steps, detects repeated patterns, and proposes safe automations through a terminal-first CLI.

## Product goal
Build a system that can say:

> “I noticed you repeat this task often. Do you want me to automate it?”

## Non-goals for v1
- Full autonomous computer control
- Deep instrumentation of every app
- Cloud sync or remote inference
- Fine-tuning models
- Unsafe browser/admin automations
- Destructive shell automations

---

## Core principles
1. **Local-first**  
   All observation, storage, scoring, and proposal generation run locally by default.

2. **Terminal-first**  
   The first usable product is a daemon plus CLI, not a full GUI.

3. **Deterministic core, AI-assisted edge**  
   Detection and execution should be mostly deterministic. Small local LLMs only help with labeling, summarization, and proposal wording.

4. **Safe by default**  
   Start with reversible, previewable automations only.

5. **Open core, private intelligence**  
   The infrastructure can be open source, while the ranking and proposal intelligence remain private.

---

## v1 scope

### Observed sources
- Active app / window title
- File system events in selected folders
- Clipboard changes
- Terminal commands
- Browser tab title / URL via lightweight integration

### Suggested automation types
- File rename rules
- File move rules
- Downloads sorting
- Clipboard transforms
- Terminal macros

### Interfaces
- `flowd` daemon
- `flow-cli` CLI
- SQLite storage
- Config file

---

## Architecture

### Open core
Public modules:
- `flowd` daemon
- Event adapters
- Event normalizer
- SQLite persistence
- Session builder
- Baseline pattern detector
- Automation DSL
- Executor
- CLI
- Plugin SDK

### Private intelligence layer
Private modules:
- Pattern ranking
- Confidence scoring
- Semantic clustering
- Proposal generator
- Personalization logic
- Suggestion suppression
- Safety classifier
- Anti-annoyance heuristics

---

## Suggested repository layout

```text
flowd/
├─ crates/
│  ├─ flow-core/
│  ├─ flow-daemon/
│  ├─ flow-cli/
│  ├─ flow-db/
│  ├─ flow-adapters/
│  ├─ flow-patterns/
│  ├─ flow-dsl/
│  ├─ flow-exec/
│  ├─ flow-sdk/
│  └─ flow-fixtures/
├─ docs/
│  ├─ architecture.md
│  ├─ event-model.md
│  ├─ automation-dsl.md
│  └─ privacy.md
├─ fixtures/
│  ├─ invoice_flow/
│  ├─ noisy_browsing/
│  ├─ terminal_macro/
│  └─ dangerous_shell/
├─ .github/
└─ PLAN.md
```

Private repository (separate from this repo):
```text
flowd-intelligence/
├─ crates/
│  └─ flow-intelligence/
```

---

## Runtime architecture

### 1. Event capture
Adapters ingest raw local events from:
- file watchers
- terminal shell hooks
- clipboard listeners
- active window tracking
- browser integration

### 2. Normalization
Raw events are converted into a small action taxonomy:
- `open_app`
- `switch_app`
- `copy_text`
- `paste_text`
- `run_command`
- `create_file`
- `rename_file`
- `move_file`
- `visit_url`
- `download_file`

### 3. Sessionization
Events are grouped into sessions based on:
- time windows
- context switches
- source continuity

### 4. Pattern detection
Two layers:
- deterministic repeated-sequence detection
- optional semantic clustering of similar workflows

### 5. Proposal generation
Pattern candidates become:
- label
- summary
- estimated time savings
- confidence score
- suggested automation

### 6. Automation compilation and execution
Accepted suggestions compile into an internal DSL and run through:
- dry-run
- preview
- undo log
- audit trail

---

## Data model

### `raw_events`
- `id`
- `ts`
- `source`
- `payload_json`

### `normalized_events`
- `id`
- `ts`
- `action_type`
- `app`
- `target`
- `metadata_json`

### `sessions`
- `id`
- `start_ts`
- `end_ts`
- `session_type`

### `session_events`
- `session_id`
- `event_id`
- `ord`

### `patterns`
- `id`
- `signature`
- `count`
- `avg_duration_ms`
- `canonical_summary`
- `confidence`

### `suggestions`
- `id`
- `pattern_id`
- `status`
- `proposal_json`
- `created_at`

### `automations`
- `id`
- `spec_yaml`
- `state`
- `accepted_at`

### `automation_runs`
- `id`
- `automation_id`
- `started_at`
- `finished_at`
- `result`
- `undo_payload_json`

---

## Learning model
No model training is required for v1.

### What changes over time
The system improves through:
- repeated pattern observation
- accept/reject/snooze feedback
- category preferences
- threshold tuning
- suppression of noisy suggestion types

### What the LLM is allowed to do
A small local LLM may help with:
- naming patterns
- summarizing workflows
- grouping similar patterns
- writing user-facing proposals

### What the LLM should not do
- control the computer directly
- infer unsafe actions autonomously
- bypass confidence thresholds
- execute arbitrary shell commands

---

## Safety model

### Allowed v1 automations
- file rename
- file move
- folder routing
- safe clipboard transforms
- terminal macro suggestions only after explicit approval

### Not allowed in v1
- bulk delete automations
- `sudo` or privileged shell automations
- admin dashboard browser actions
- password handling
- authentication workflows
- destructive command generation

### Mandatory safeguards
- dry-run support
- undo log for supported actions
- explicit user approval
- reversible actions only
- redaction of sensitive event payloads

---

## Privacy model
- Local-only by default
- No telemetry by default
- Opt-in observed zones
- Configurable redaction for paths, command arguments, and clipboard content
- Browser query-string stripping optional
- Secret detection hooks later

---

## CLI design

### Main commands
```bash
flow-cli status
flow-cli tail
flow-cli events
flow-cli sessions
flow-cli patterns
flow-cli suggest
flow-cli apply <id>
flow-cli reject <id>
flow-cli snooze <id>
flow-cli automations
flow-cli runs
flow-cli undo <run-id>
flow-cli config check
```

### Example suggestion output
```bash
$ flow-cli suggest

[12] Process invoices from Downloads
freq: 6x in 7 days
avg time: 1m 42s
estimated savings: 10m/week
confidence: 0.91

observed pattern:
- file_created ~/Downloads/*.pdf
- open_file
- rename_file
- move_file ~/Documents/Invoices

proposal:
- when a PDF matching invoice/factuur appears in Downloads
- rename with date prefix
- move to ~/Documents/Invoices

actions:
[a] apply
[d] dry-run
[r] reject
[s] snooze
```

---

## MVP milestone
The first demo-worthy milestone is:

> Detect repeated file workflows in Downloads and show one useful automation suggestion in the terminal, including dry-run preview.

This is small, testable, and aligned with the product promise.

---

## Development phases

### Phase 0 — Skeleton
Deliver:
- workspace
- config
- database migrations
- daemon + CLI skeleton

### Phase 1 — Event capture
Deliver:
- file watcher
- clipboard watcher
- terminal hook
- active-window adapter

### Phase 2 — Normalization and sessions
Deliver:
- normalized event schema
- action taxonomy
- session builder

### Phase 3 — Pattern detection
Deliver:
- deterministic repeated-sequence detection
- frequency and duration estimation
- noise filtering

### Phase 4 — Suggestions
Deliver:
- suggestion records
- terminal rendering
- ranking hooks
- confidence model stub

### Phase 5 — Automation execution
Deliver:
- automation DSL
- dry-run
- executor
- undo log

### Phase 6 — Local LLM integration
Deliver:
- structured labeling
- workflow summaries
- proposal wording
- deterministic fallback

### Phase 7 — Feedback learning
Deliver:
- accept/reject/snooze memory
- suppression
- per-user prioritization

---

## Testing strategy

### Unit tests
For:
- config parsing
- normalizer
- sessionization
- signature generation
- DSL parsing
- scoring

### Integration tests
For:
- event ingestion to DB
- normalized pipeline
- pattern detection
- apply + dry-run + undo flow

### Golden tests
Fixture-based expected outputs for:
- normalized events
- sessions
- patterns
- suggestions
- terminal rendering

### Replay tests
Re-run recorded or synthetic event logs through the pipeline.

### Safety tests
Ensure no suggestions are produced for:
- dangerous shell actions
- admin-like flows
- secret-heavy contexts
- destructive file patterns

### Performance tests
Verify:
- stable ingestion under event bursts
- acceptable scan latency
- bounded memory use

---

## Initial fixture scenarios

### 1. Invoice workflow
Repeated:
- download pdf
- open file
- rename file
- move to invoices

Expected:
- one suggestible file automation

### 2. Noisy browsing
Unrelated browser events only

Expected:
- no suggestion

### 3. Terminal macro
Repeated:
- `git status`
- `cargo test`
- `cargo fmt`
- `git add -A`

Expected:
- macro candidate, no automatic execution

### 4. Dangerous shell
Repeated privileged or destructive commands

Expected:
- never suggest executable automation

### 5. Similar but variable invoices
Different filenames, same workflow

Expected:
- grouped as one pattern

---

## Open source strategy

### Public
Open source:
- daemon
- adapters
- DB schema
- CLI
- DSL
- executor
- SDK
- docs
- fixtures

### Private
Keep private:
- ranking model
- semantic grouping
- proposal generation policy
- confidence calibration
- safety classification
- user-preference learning
- suggestion timing logic

This preserves your moat in product quality rather than infrastructure.

---

## Recommended v1 focus
Build only this first:
- downloads folder watcher
- deterministic pattern detection
- terminal suggestions via CLI
- file automations (rename and move)
- dry-run preview
- undo support

That is the smallest real product slice.

---

## Exit criteria for v1
v1 is successful when:
- the daemon runs reliably in the background
- repeated download/file workflows are detected
- at least one safe automation can be applied
- dry-run and undo work
- false positives remain low enough not to annoy users
