# flowd System Overview

`flowd` is the open-core workflow engine. It owns local observation, persistence, session building, pattern detection, baseline suggestion generation, CLI display, automation approval, execution, and undo.

`flowd-intelligence` is an optional private decision layer. It does not replace the engine. It evaluates already-generated suggestion candidates and returns display-oriented decisions such as ranking, timing, suppression, personalization, clustering, proposal wording, and display orchestration.

The full ecosystem stays local-first. The open core remains deterministic and fully functional even when no private intelligence is configured.

## 1. System overview

The full production flow is:

```text
User activity
  -> event capture
  -> normalization
  -> sessions
  -> pattern detection
  -> suggestion generation
  -> intelligence evaluation (optional)
  -> CLI display
  -> automation execution
```

Expanded end-to-end flow:

1. A user performs normal work on the local machine.
2. `flowd` captures events through adapters such as filesystem, terminal, browser, clipboard, or window observers.
3. `flowd` persists raw events and normalized events in SQLite.
4. `flowd` groups normalized events into sessions.
5. `flowd` detects repeated patterns across sessions.
6. `flowd` generates baseline suggestions from those patterns.
7. `flowd` may optionally send narrow suggestion DTOs through the intelligence boundary for private evaluation.
8. `flowd` maps any returned ranking, timing, suppression, wording, and explainability metadata back into local deterministic display records.
9. `flowd` displays the final suggestion list in the CLI.
10. The user may approve a suggestion and execute the resulting automation.
11. `flowd` records execution results and undo data in the open core.

This is the core architectural rule for the ecosystem: open core creates facts and performs actions; private intelligence may improve the decision layer over those facts.

## 2. Ownership by repository

`flowd` owns:

- event capture
- persistence
- sessions
- patterns
- baseline suggestions
- automation approval
- execution
- undo

In practice that means `flowd` owns local adapters, SQLite schema and migrations, event normalization, session rebuilding, pattern detection, suggestion persistence, feedback history persistence, CLI rendering, automation specification, dry-run planning, execution, and undo logging.

`flowd-intelligence` owns:

- ranking
- timing
- suppression
- personalization
- clustering
- proposal wording
- display orchestration

The private repository can decide which suggestions to surface, when to surface them, how to cluster them, and how to word them. It does not become the system of record for workflow state.

## 3. Hard architecture boundary

The dependency direction is one-way:

```text
flowd -> flowd-intelligence
```

Never the reverse.

The private intelligence layer must never own:

- storage
- migrations
- execution
- event capture

It also must not become the source of truth for sessions, patterns, or baseline suggestions.

Boundary rules:

- `flowd` exports narrow DTOs rather than internal database rows or executor internals.
- `flowd` calls the private layer from one explicit boundary module: [crates/flow-analysis/src/intelligence_boundary.rs](/Users/nickvandort/Documents/Coding/flowd/crates/flow-analysis/src/intelligence_boundary.rs).
- `flowd-intelligence` returns display decisions, not persistence commands or execution commands.
- Explainability is optional and deterministic: the private layer may return structured reasons, but `flowd` always normalizes them locally and falls back explicitly when no explanation is available.
- If the private layer is absent or fails, `flowd` falls back to the baseline open-core flow.

The hard boundary is what keeps the public engine viable as a standalone product and prevents private code from absorbing core system responsibilities.

## 4. Practical integration flow

### Baseline open-core flow

```text
User activity
  -> adapters
  -> SQLite storage
  -> normalization
  -> sessions
  -> pattern detection
  -> baseline suggestion generation
  -> CLI suggestions
  -> approval
  -> automation execution
```

In this mode:

- `flowd` observes and stores the activity history.
- `flowd` generates baseline suggestions directly from repeated patterns.
- `flowd` displays suggestions in the CLI without any private dependency.
- `flowd` owns approval, execution, run logging, and undo.

### Intelligence-enhanced flow

```text
User activity
  -> adapters
  -> SQLite storage
  -> normalization
  -> sessions
  -> pattern detection
  -> baseline suggestion generation
  -> intelligence boundary in flowd
  -> flowd-intelligence evaluation
  -> CLI suggestions
  -> approval
  -> automation execution
```

In this mode:

- `flowd` still creates and persists suggestion candidates.
- `flowd` packages candidate data and local feedback history into DTOs.
- `flowd-intelligence` evaluates ranking, timing, suppression, personalization, clustering, wording, and orchestration.
- `flowd` applies those decisions to CLI presentation.
- `flowd` remains the only owner of approval, execution, undo, and persistence.

Fallback rules:

- If no private client exists, baseline suggestions are shown directly.
- If the private client returns no decisions, baseline suggestions are shown directly.
- If the private client errors, `flowd` degrades to the open-core path and does not block approval or execution.

## 5. Repository structure

Intended public repository structure:

```text
flowd/
├─ crates/
│  ├─ flow-adapters/   event capture adapters
│  ├─ flow-analysis/   baseline suggestion generation and intelligence boundary
│  ├─ flow-cli/        CLI display and operator commands
│  ├─ flow-core/       shared domain types and configuration
│  ├─ flow-daemon/     long-running observation and analysis orchestration
│  ├─ flow-db/         SQLite persistence, queries, and migrations
│  ├─ flow-dsl/        automation specification
│  ├─ flow-exec/       approval, execution, and undo
│  └─ flow-patterns/   normalization, sessions, and pattern detection
├─ docs/
│  ├─ system-overview.md
│  ├─ architecture.md
│  ├─ architecture-diagram.md
│  ├─ event-model.md
│  └─ automation-dsl.md
└─ fixtures/
```

Intended private repository structure:

```text
flowd-intelligence/
├─ crates/
│  └─ flow-intelligence/   ranking and display decision modules
├─ fixtures/               deterministic replay scenarios
└─ docs/                   private implementation notes
```

`flowd` stays responsible for the end-to-end engine. `flowd-intelligence` stays responsible for private decision logic only.

## 6. Feedback history integration

Feedback history belongs to the open core because it is part of suggestion state and is needed for deterministic fallback behavior.

Current feedback-history fields:

- `shown_count`
- `accepted_count`
- `rejected_count`
- `snoozed_count`
- `last_shown_ts`
- `last_accepted_ts`
- `last_rejected_ts`
- `last_snoozed_ts`

Design rules for these fields:

- They are persisted in the `suggestions` table by `flow-db`.
- They are exposed through repository-layer models such as `StoredSuggestion` and `SuggestionDetails`.
- They are included in the intelligence boundary DTO so they can be passed through `flowd -> flowd-intelligence` without exposing storage internals.
- They remain owned by `flowd` even when private intelligence consumes them for ranking, suppression, or personalization decisions.

This means feedback history can influence private decision-making later without moving storage or lifecycle ownership out of the public repository.

## 7. Design rules

- Keep the open core local-first.
- Keep the baseline open-core path deterministic.
- Keep the dependency direction one-way: `flowd` may call `flowd-intelligence`, never the reverse.
- Keep the boundary narrow and DTO-based.
- Keep persistence, migrations, execution, and event capture inside `flowd`.
- Keep approval and undo inside `flowd`.
- Keep private intelligence optional.
- Keep terminology consistent: patterns produce baseline suggestions; intelligence evaluates those suggestions for display decisions.

For lower-level detail, see [docs/architecture.md](/Users/nickvandort/Documents/Coding/flowd/docs/architecture.md), [docs/architecture-diagram.md](/Users/nickvandort/Documents/Coding/flowd/docs/architecture-diagram.md), and [docs/event-model.md](/Users/nickvandort/Documents/Coding/flowd/docs/event-model.md).
