# flowd System Overview

`flowd` is the public open-core workflow engine. It owns facts and actions: event capture, persistence, sessions, patterns, baseline suggestions, automations, execution, undo, feedback history, and explainability normalization.

`flowd-intelligence` is a separate private decision layer. It does not replace the engine. It only evaluates already-generated suggestion candidates and returns decision metadata for prioritization, timing, suppression, personalization, clustering, wording, and display decisions.

The architecture stays local-first and deterministic. `flowd` remains fully functional when no private intelligence is configured.

## 1. System overview

The end-to-end system is:

```text
User activity
  -> event capture
  -> persistence
  -> sessions
  -> patterns
  -> baseline suggestions
  -> optional intelligence evaluation
  -> explainable CLI suggestions
  -> automations
  -> execution
  -> undo
```

Expanded flow:

1. A user performs normal work on the local machine.
2. `flowd` captures local events through adapters.
3. `flowd` persists raw and normalized workflow facts in SQLite.
4. `flowd` groups events into sessions.
5. `flowd` detects repeated patterns across sessions.
6. `flowd` generates deterministic baseline suggestions from those patterns.
7. `flowd` records and maintains suggestion feedback history in open-core storage.
8. `flowd` may send narrow suggestion DTOs plus feedback-history signals through the intelligence boundary for optional private evaluation.
9. `flowd-intelligence` may return display decisions such as prioritization, delay, suppression, wording, clustering, and structured explanation metadata.
10. `flowd` normalizes any returned explanation metadata into local explainability records and falls back to explicit baseline explanations when no private decision exists.
11. `flowd` shows the resulting suggestion list in the CLI.
12. The user may approve a suggestion into an automation.
13. `flowd` owns planning, execution, execution history, and undo data.

The canonical rule is simple: `flowd` owns facts and actions. `flowd-intelligence` owns display decisions over those facts.

## 2. Ownership by repository

`flowd` owns:

- event capture
- persistence
- sessions
- patterns
- baseline suggestions
- automations
- execution
- undo
- feedback history
- explainability normalization

In practice that means the public repository owns adapters, SQLite schema and migrations, event normalization, session rebuilding, pattern detection, suggestion persistence, feedback history persistence, CLI rendering, automation specification, dry-run planning, execution, execution logs, undo logs, and deterministic fallback explanations.

`flowd-intelligence` owns:

- prioritization
- timing
- suppression
- personalization
- clustering
- wording
- display decisions

In practice that means the private repository can decide which existing suggestions should be surfaced first, delayed, hidden, grouped, or reworded. It does not become the system of record for workflow state.

Feedback history belongs to `flowd`, not `flowd-intelligence`, because it is part of suggestion state and is required for deterministic fallback behavior. The current feedback-history fields are:

- `shown_count`
- `accepted_count`
- `rejected_count`
- `snoozed_count`
- `last_shown_ts`
- `last_accepted_ts`
- `last_rejected_ts`
- `last_snoozed_ts`

These fields are persisted by `flow-db`, exposed through repository models, and mapped into the boundary DTO so private intelligence can consume them without owning storage.

Explainability follows the same rule. `flowd-intelligence` may return structured reasons, but `flowd` owns the final local explainability shape shown to users.

## 3. Hard architecture boundary

The dependency direction is one-way:

```text
flowd -> flowd-intelligence
```

Never the reverse.

Hard boundary rules:

- `flowd` may call `flowd-intelligence`.
- `flowd-intelligence` must not call back into `flowd`.
- `flowd` exports narrow DTOs, not internal database rows, migrations, or executor internals.
- `flowd-intelligence` returns display decisions, not persistence commands or execution commands.
- `flowd-intelligence` must never own event capture, persistence, sessions, patterns, baseline suggestion generation, approval, execution, or undo.
- Open-core must remain usable without private intelligence.

The one explicit boundary module inside the public repo is [crates/flow-analysis/src/intelligence_boundary.rs](/Users/nickvandort/Documents/Coding/flowd/crates/flow-analysis/src/intelligence_boundary.rs). All private integration should pass through that module so the rest of the workspace stays free of private contracts.

This boundary is what keeps the open-core engine viable as a standalone product. `flowd-intelligence` can improve decision quality, but it cannot absorb ownership of core system responsibilities.

## 4. Practical integration flow

Baseline open-core flow:

```text
User activity
  -> adapters
  -> SQLite storage
  -> sessions and patterns
  -> baseline suggestions
  -> CLI display
  -> approval
  -> execution
  -> undo
```

In baseline mode:

- `flowd` captures, stores, analyzes, and displays workflow state without any private dependency.
- suggestion ordering remains deterministic and local.
- feedback history still accumulates in `flowd`.
- explainability is explicit and local through baseline fallback messages.

Intelligence-enhanced flow:

```text
User activity
  -> adapters
  -> SQLite storage
  -> sessions and patterns
  -> baseline suggestions
  -> boundary DTO mapping in flowd
  -> flowd-intelligence evaluation
  -> local explainability normalization
  -> CLI display
  -> approval
  -> execution
  -> undo
```

In intelligence mode:

- `flowd` still creates and persists all candidate suggestions.
- `flowd` packages candidate state, recency signals, and feedback history into DTOs.
- `flowd-intelligence` evaluates prioritization, timing, suppression, personalization, clustering, wording, and display decisions.
- `flowd` maps those decisions back into local presentation records.
- `flowd` remains the only owner of approval, execution, undo, and persistence.

Fallback behavior:

- If no private client exists, baseline suggestions are shown directly.
- If the private client returns no decisions, baseline suggestions are shown directly.
- If the private client errors, `flowd` degrades to the open-core path and does not block approval or execution.

## 5. Repository structure

Current public repository structure:

```text
flowd/
├─ crates/
│  ├─ flow-adapters/   local event capture adapters
│  ├─ flow-analysis/   baseline suggestion generation and intelligence boundary
│  ├─ flow-cli/        CLI display and operator commands
│  ├─ flow-core/       shared domain types and configuration
│  ├─ flow-daemon/     observation and analysis orchestration
│  ├─ flow-db/         SQLite persistence, queries, and migrations
│  ├─ flow-dsl/        automation specification
│  ├─ flow-exec/       approval, execution, and undo
│  └─ flow-patterns/   normalization, sessions, and pattern detection
├─ docs/
│  ├─ architecture.md
│  ├─ architecture-diagram.md
│  ├─ automation-dsl.md
│  ├─ event-model.md
│  ├─ privacy.md
│  └─ system-overview.md
└─ fixtures/
```

Private repository shape:

```text
flowd-intelligence/
├─ crates/
│  └─ flow-intelligence/   private decision logic
├─ fixtures/               deterministic replay scenarios
└─ docs/                   private implementation notes
```

Repository ownership should stay easy to reason about: `flowd` owns the engine, and `flowd-intelligence` owns only the optional decision layer.

## 6. Design rules

- Keep the system local-first.
- Keep the baseline open-core path deterministic.
- Keep the dependency direction one-way: `flowd -> flowd-intelligence`.
- Keep `flowd` as the owner of facts and actions.
- Keep `flowd-intelligence` as the owner of prioritization, timing, suppression, personalization, clustering, wording, and display decisions.
- Keep the boundary narrow and DTO-based.
- Keep persistence, migrations, feedback history, execution, and undo inside `flowd`.
- Keep explainability explicit: private reasons may enrich decisions, but `flowd` must always normalize or replace them locally.
- Keep private intelligence optional.
- Do not add cloud dependencies to the core architecture.
- Do not add GUI-only assumptions to the system model.
- Keep contributor-facing and user-facing text in English.

## 7. Next integration steps

- Keep the boundary contract in [crates/flow-analysis/src/intelligence_boundary.rs](/Users/nickvandort/Documents/Coding/flowd/crates/flow-analysis/src/intelligence_boundary.rs) stable and versioned by code review discipline.
- Extend deterministic fixture coverage for boundary DTO mapping, feedback-history propagation, and explainability fallback behavior.
- Keep repository models and migration docs aligned whenever feedback-history fields change.
- Add more private replay scenarios in `flowd-intelligence` without moving state ownership out of `flowd`.
- Preserve graceful degradation so new private decision features never block the open-core baseline path.

For lower-level detail, see [docs/architecture.md](/Users/nickvandort/Documents/Coding/flowd/docs/architecture.md), [docs/architecture-diagram.md](/Users/nickvandort/Documents/Coding/flowd/docs/architecture-diagram.md), and [docs/event-model.md](/Users/nickvandort/Documents/Coding/flowd/docs/event-model.md).
