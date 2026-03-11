# Architecture

flowd follows a **terminal-first, local-first architecture** where workflow discovery and automation logic run locally.

## Pipeline layers

1. **Observation layer** captures local events through adapters, stores raw events, and triggers downstream analysis work.
2. **Analysis layer** normalizes raw events, builds sessions, detects repeated patterns, and generates suggestions.
3. **Execution layer** approves suggestions into automations, plans dry-runs, executes supported actions, and stores automation run results.

## Core responsibilities

1. **Adapters** capture raw local events such as filesystem, terminal, clipboard, or application events.
2. **Core** defines stable shared domain types and configuration.
3. **DB** persists raw events, normalized events, sessions, patterns, suggestions, automations, and execution runs.
4. **Patterns** owns normalization, session building, and repeated workflow detection.
5. **DSL** defines safe automation specifications.
6. **Exec** owns suggestion approval, dry-run planning, execution planning, and execution.
7. **CLI** renders analysis results and invokes execution workflows.
8. **Daemon** orchestrates observation and analysis triggers.

## System flow

```text
Observation: local events -> raw events -> raw event persistence
    ↓
Analysis: normalization -> normalized event persistence -> sessions -> pattern detection -> suggestions
    ↓
Execution: CLI inspection -> suggestion approval -> dry-run planning -> execution -> automation_runs
```

## Open core boundary

The open source portion of flowd contains:

- event capture adapters
- event normalization
- SQLite persistence
- session building
- baseline pattern detection
- CLI inspection tools
- automation DSL
- safe execution layer

Future intelligence improvements such as ranking, clustering, and personalization are designed to sit **above pattern detection and below suggestion presentation**.

## Related documentation

A more detailed system diagram is available in:

```text
docs/architecture-diagram.md
```
