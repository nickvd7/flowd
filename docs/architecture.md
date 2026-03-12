# Architecture

flowd follows a **terminal-first, local-first architecture** where workflow discovery and automation logic run locally.

For the canonical high-level system description, repository ownership split, and integration flow, start with [docs/system-overview.md](/Users/nickvandort/Documents/Coding/flowd/docs/system-overview.md).

## Pipeline layers

1. **Observation layer** captures local events through adapters, stores raw events, and triggers downstream analysis work.
2. **Analysis layer** normalizes raw events, builds sessions, detects repeated patterns, generates baseline suggestions, and optionally evaluates an intelligence boundary.
3. **Execution layer** approves suggestions into automations, plans dry-runs, executes supported actions, and stores automation run results.

## Core responsibilities

1. **Adapters** capture raw local events such as filesystem, terminal, clipboard, or application events.
2. **Core** defines stable shared domain types and configuration.
3. **DB** persists raw events, normalized events, sessions, patterns, suggestions, automations, and execution runs.
4. **Analysis** owns normalization orchestration, session rebuilding, baseline suggestion generation, and the optional intelligence boundary.
5. **Patterns** owns normalization rules, session building, and repeated workflow detection.
6. **DSL** defines safe automation specifications.
7. **Exec** owns suggestion approval, dry-run planning, execution planning, and execution.
8. **CLI** renders analysis results and invokes execution workflows.
9. **Daemon** orchestrates observation and analysis triggers.

## System flow

```text
Observation: local events -> raw events -> raw event persistence
    ↓
Analysis: normalization -> normalized event persistence -> sessions -> pattern detection -> suggestions
    ↓
Execution: CLI inspection -> suggestion approval -> dry-run planning -> execution -> automation_runs
```

## Open core boundary

`flowd` is the open-core system engine. It owns facts:

- event capture adapters
- stored raw and normalized history
- event normalization
- session building
- pattern detection
- baseline suggestion generation
- suggestion persistence and user action history persistence
- CLI inspection tools
- automation DSL
- automations

`flowd` also owns actions:

- approval
- dry-run
- execution
- undo

Private intelligence is optional and should only influence:

- ranking
- timing
- suppression
- personalization
- clustering
- proposal wording
- display orchestration

Open-core should never become dependent on private intelligence for basic workflow functionality. `flowd` must remain able to observe, detect, suggest, approve, execute, and undo without any private dependency.

## Intelligence boundary

The integration direction is one-way:

- `flowd` may call `flowd-intelligence`
- `flowd-intelligence` must not own or pull storage, sessions, patterns, suggestion persistence, approval, execution, or undo into itself

`flowd` never exposes internal storage rows or execution details directly to private intelligence. It maps internal analysis state to narrow contract/value objects, evaluates an optional client, and maps the resulting display decisions back into open-core suggestion behavior in one explicit analysis boundary module.

Architecture note: Open-core owns facts and actions; private-core improves which suggestions are shown, when they are shown, and how they are presented.

## Related documentation

The canonical high-level overview is:

```text
docs/system-overview.md
```

A more detailed system diagram is available in:

```text
docs/architecture-diagram.md
```
