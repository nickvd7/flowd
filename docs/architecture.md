# Architecture

This template follows a terminal-first, local-first architecture:

1. **Adapters** capture raw local events.
2. **Core** defines stable shared types.
3. **DB** persists events, sessions, patterns, suggestions, and automation runs.
4. **Patterns** groups events into sessions and detects repeated workflows.
5. **DSL** defines safe automation specifications.
6. **Exec** runs dry-runs and supported automations.
7. **CLI** renders insights and accepts approvals.
8. **Daemon** orchestrates ingestion and background processing.

The intended boundary for a future private intelligence layer sits above baseline pattern detection and below final suggestion presentation.
