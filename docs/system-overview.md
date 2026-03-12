# flowd System Overview

`flowd` is the public open-core workflow engine. It observes local activity, stores a deterministic event history, detects repeated workflows, generates baseline suggestions, and lets the user approve, execute, and undo safe automations from the CLI.

`flowd-intelligence` is a separate private repository that can optionally improve decision quality. It does not replace the open-core engine. It only evaluates already-detected suggestion candidates and returns display-oriented decisions such as ranking, timing, suppression, personalization, clustering, wording, and display orchestration.

The complete system remains local-first, deterministic in the open core, terminal-first, and functional without any cloud dependency or GUI.

## System Overview

At a high level, the full system works like this:

1. The user performs normal work on the local machine.
2. `flowd` observes supported activity through local adapters.
3. `flowd` stores raw and normalized events in SQLite.
4. `flowd` rebuilds sessions and detects repeated patterns.
5. `flowd` generates baseline suggestions from those patterns.
6. `flowd` may optionally send narrow DTOs to `flowd-intelligence` for display-quality decisions.
7. `flowd` renders suggestions in the CLI.
8. The user may approve a suggestion, execute the resulting automation, and undo supported actions.

Baseline open-core flow:

```text
user activity
  -> flowd observation
  -> raw event persistence
  -> normalization
  -> session building
  -> pattern detection
  -> baseline suggestions
  -> CLI display
  -> approval
  -> execution
  -> undo
```

Optional intelligence-enhanced flow:

```text
baseline suggestions
  -> intelligence boundary in flowd
  -> flowd-intelligence evaluation
  -> display decisions returned to flowd
  -> CLI display
```

## Ownership by repository

`flowd` owns facts and actions. It is responsible for:

- event capture
- persistence
- sessions
- patterns
- baseline suggestions
- approval
- execution
- undo

More concretely, `flowd` owns local adapters, the SQLite schema, normalization, session rebuilding, pattern detection, suggestion persistence, CLI inspection, automation definitions, dry-run planning, execution, and undo logging.

`flowd-intelligence` owns decision-quality improvements only. It is responsible for:

- ranking
- timing
- suppression
- personalization
- clustering
- proposal wording
- display orchestration

That means the private repository may change which already-generated suggestions are shown, when they are shown, and how they are phrased or grouped, but it does not create the underlying facts.

## Hard architecture boundary

The integration direction is one-way:

```text
flowd -> flowd-intelligence
```

This boundary is intentionally strict:

- the private layer never owns storage, execution, or event capture
- the private layer never becomes the source of truth for sessions, patterns, or suggestions
- `flowd` exports narrow contract/value objects rather than internal storage rows
- `flowd` maps private decisions back into open-core display behavior inside one explicit intelligence boundary
- open-core must remain fully functional without private intelligence

In short: `flowd` owns facts and actions; `flowd-intelligence` may improve display decisions over those facts.

For the boundary contract in code, see [crates/flow-analysis/src/intelligence_boundary.rs](/Users/nickvandort/Documents/Coding/flowd/crates/flow-analysis/src/intelligence_boundary.rs).

## Practical integration flow

### Baseline open-core flow

Without any private integration, `flowd` still provides the full product loop:

1. capture local events
2. persist raw and normalized history
3. rebuild sessions
4. detect repeated patterns
5. create baseline suggestions
6. show suggestions in the CLI
7. approve, execute, and undo supported automations

This is the required default path and the main contributor model for the public repository.

### Optional intelligence-enhanced flow

When `flowd-intelligence` is present, `flowd` may package candidate suggestions and local interaction history into stable DTOs, call a single evaluation entry point, and receive display-oriented decisions back. Those decisions may affect:

- ordering
- delay timing
- suppression
- clustering/grouping
- wording
- final display orchestration

The private layer evaluates candidates; `flowd` still renders the CLI, records user actions, and executes approved automations.

### Fallback behavior

Fallback behavior must always be deterministic and safe:

- if no private client is configured, `flowd` uses baseline suggestions directly
- if the private client returns no decisions, `flowd` uses baseline suggestions directly
- if the private client fails, `flowd` degrades to the open-core path rather than blocking suggestions, approval, execution, or undo

## Repository structure

Public repository:

```text
flowd/
├─ crates/
│  ├─ flow-adapters/   local event capture
│  ├─ flow-analysis/   analysis pipeline and intelligence boundary
│  ├─ flow-cli/        terminal interface
│  ├─ flow-core/       shared domain types and config
│  ├─ flow-daemon/     observation loop and analysis triggers
│  ├─ flow-db/         SQLite persistence and migrations
│  ├─ flow-dsl/        automation specification
│  ├─ flow-exec/       approval, dry-run, execution, undo
│  └─ flow-patterns/   normalization, sessions, pattern detection
├─ docs/
│  ├─ system-overview.md
│  ├─ architecture.md
│  ├─ architecture-diagram.md
│  ├─ event-model.md
│  └─ automation-dsl.md
└─ fixtures/
```

Private repository:

```text
flowd-intelligence/
├─ crates/
│  └─ flow-intelligence/   ranking and display-quality decision modules
├─ fixtures/               deterministic replay scenarios
└─ docs/                   private implementation notes
```

The public repo documents the complete system shape so contributors understand where the private layer fits, while private implementation details stay outside `flowd`.

## Design rules

- Keep the open core local-first. Observation, storage, analysis, approval, execution, and undo run locally.
- Keep the open core deterministic. The same local history should produce the same baseline analysis result.
- Keep the boundary narrow. Pass DTOs, not internal tables or executor internals.
- Keep the dependency direction one-way. `flowd` may call `flowd-intelligence`; the reverse must not happen.
- Keep open-core functional on its own. Private intelligence is optional enhancement, not required infrastructure.
- Keep the CLI authoritative for user approval and execution flows in the public repo.
- Keep private intelligence out of persistence ownership, event capture, and execution ownership.
- Keep terminology consistent: patterns produce baseline suggestions; intelligence modifies display decisions over those suggestions.

## Next integration steps

- keep the intelligence boundary concentrated in `flow-analysis`
- define and version the DTO contract shared with `flowd-intelligence`
- add deterministic fixture and replay coverage for baseline and intelligence-enhanced suggestion display
- document failure and fallback behavior alongside the boundary contract
- keep contributor-facing docs in `flowd` aligned with any future boundary changes

For lower-level detail, see [docs/architecture.md](/Users/nickvandort/Documents/Coding/flowd/docs/architecture.md), [docs/architecture-diagram.md](/Users/nickvandort/Documents/Coding/flowd/docs/architecture-diagram.md), and [docs/event-model.md](/Users/nickvandort/Documents/Coding/flowd/docs/event-model.md).
