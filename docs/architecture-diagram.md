# FLOWD_ARCHITECTURE_DIAGRAM.md

# flowd architecture diagram

## High-level flow

```text
┌─────────────────────┐
│   Local event       │
│   sources           │
│                     │
│ - file watcher      │
│ - terminal hook     │
│ - clipboard         │
│ - active window     │
│ - browser bridge    │
└─────────┬───────────┘
          │
          ▼
┌─────────────────────┐
│   Raw events        │
│                     │
│ Stored as source-   │
│ specific payloads   │
│ before interpretation│
└─────────┬───────────┘
          │
          ▼
┌─────────────────────┐
│   Normalization     │
│                     │
│ Convert raw events  │
│ into stable action  │
│ types such as:      │
│ - CreateFile        │
│ - RenameFile        │
│ - MoveFile          │
│ - RunCommand        │
│ - VisitUrl          │
└─────────┬───────────┘
          │
          ▼
┌─────────────────────┐
│   SQLite storage    │
│                     │
│ - raw_events        │
│ - normalized_events │
│ - sessions          │
│ - patterns          │
│ - suggestions       │
│ - automations       │
└─────────┬───────────┘
          │
          ▼
┌─────────────────────┐
│   Session builder   │
│                     │
│ Group nearby events │
│ into workflow       │
│ sessions using:     │
│ - inactivity gaps   │
│ - context switches  │
└─────────┬───────────┘
          │
          ▼
┌─────────────────────┐
│ Pattern detection   │
│                     │
│ Deterministically   │
│ detect repeated     │
│ workflows, e.g.     │
│ download -> rename  │
│ -> move             │
└─────────┬───────────┘
          │
          ▼
┌─────────────────────┐
│ Suggestion engine   │
│                     │
│ Turn repeated       │
│ patterns into       │
│ human-readable      │
│ automation ideas    │
└─────────┬───────────┘
          │
          ▼
┌─────────────────────┐
│ flow-cli            │
│                     │
│ Commands:           │
│ - status            │
│ - patterns          │
│ - suggest           │
│ - tail              │
└─────────┬───────────┘
          │
          ▼
┌─────────────────────┐
│ Automation DSL      │
│ + execution layer   │
│                     │
│ - dry-run preview   │
│ - safe file actions │
│ - undo log          │
└─────────────────────┘
```

---

## Open-core vs private-core boundary

```text
OPEN CORE
├─ adapters
├─ raw events
├─ normalized events
├─ SQLite schema
├─ session builder
├─ baseline pattern detector
├─ CLI
├─ DSL
├─ dry-run executor
└─ undo log

PRIVATE CORE
├─ ranking
├─ semantic clustering
├─ proposal wording
├─ suggestion timing
├─ personalization
└─ advanced safety intelligence
```

---

## First vertical slice

This is the first feature path you want to implement:

```text
File watcher
   ↓
Raw file event
   ↓
NormalizedEvent(CreateFile / RenameFile / MoveFile)
   ↓
Session builder
   ↓
Repeated pattern detector
   ↓
Suggestion record in SQLite
   ↓
flow-cli suggest
```

Example target workflow:

```text
~/Downloads/invoice.pdf
   ↓
rename to 2026-03-invoice.pdf
   ↓
move to ~/Documents/Invoices
```

After this happens repeatedly, `flowd` should suggest:

```text
"I noticed you often rename and move invoice files from Downloads to Invoices.
Would you like to automate this?"
```

---

## Recommended module ownership

```text
flow-adapters   -> local event capture
flow-core       -> shared domain types and config
flow-db         -> SQLite persistence and migrations
flow-patterns   -> normalization, sessions, repeated-pattern detection
flow-cli        -> terminal interface
flow-daemon     -> background orchestration
flow-dsl        -> automation specification
flow-exec       -> dry-run and execution
```

---

## Safety model

Only allow safe, inspectable actions in v1:

- file rename
- file move
- dry-run preview
- undo support

Do not allow in v1:

- delete actions
- arbitrary shell execution
- browser automation
- hidden background remote calls
- destructive workflows

---

## Codex implementation order

1. file watcher adapter
2. raw file event persistence
3. normalization to `NormalizedEvent`
4. session builder
5. repeated pattern detector
6. suggestion persistence
7. `flow-cli suggest` rendering
8. dry-run preview

That order gives you the fastest path to the first real demo.
