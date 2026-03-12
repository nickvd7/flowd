┌─────────────────────────────────────────────────────────────────────┐
│                              USER                                   │
│                                                                     │
│  files / folders / shell / apps / clipboard / browser behavior      │
└─────────────────────────────────────────────────────────────────────┘
                                 │
                                 ▼
┌─────────────────────────────────────────────────────────────────────┐
│                              flowd                                  │
│                    Open-core workflow engine                        │
└─────────────────────────────────────────────────────────────────────┘

  ┌────────────────────── Observation layer ───────────────────────┐
  │                                                                │
  │  flow-adapters                                                 │
  │   - filesystem watcher                                         │
  │   - future terminal hooks                                      │
  │   - future clipboard/browser inputs                            │
  │                                                                │
  │  output: RawEvent                                              │
  └────────────────────────────────────────────────────────────────┘
                                 │
                                 ▼
  ┌────────────────────── Persistence layer ───────────────────────┐
  │                                                                │
  │  flow-db                                                       │
  │   - SQLite schema                                              │
  │   - migrations                                                 │
  │   - repositories                                               │
  │                                                                │
  │  tables:                                                       │
  │   - raw_events                                                 │
  │   - normalized_events                                          │
  │   - sessions                                                   │
  │   - session_events                                             │
  │   - patterns                                                   │
  │   - suggestions                                                │
  │   - automations                                                │
  │   - automation_runs                                            │
  └────────────────────────────────────────────────────────────────┘
                                 │
                                 ▼
  ┌────────────────────── Analysis layer ──────────────────────────┐
  │                                                                │
  │  flow-patterns                                                 │
  │   - normalization                                              │
  │   - session builder                                            │
  │   - repeated pattern detection                                 │
  │   - baseline suggestion generation                             │
  │                                                                │
  │  output:                                                       │
  │   - NormalizedEvent                                            │
  │   - EventSession                                               │
  │   - PatternCandidate                                           │
  │   - baseline suggestions                                       │
  └────────────────────────────────────────────────────────────────┘
                                 │
                                 ▼
  ┌────────────────────── Intelligence boundary ───────────────────┐
  │                                                                │
  │  flowd intelligence client boundary                            │
  │   - maps internal flowd models → DTO contracts                 │
  │   - calls private intelligence layer if available              │
  │   - maps decisions back → flowd display behavior               │
  │                                                                │
  │  IMPORTANT:                                                    │
  │   flowd remains fully functional without this step             │
  └────────────────────────────────────────────────────────────────┘
                                 │
                                 ▼
┌─────────────────────────────────────────────────────────────────────┐
│                        flowd-intelligence                           │
│                  Private decision-quality layer                     │
└─────────────────────────────────────────────────────────────────────┘

  ┌────────────────────── Contract layer ──────────────────────────┐
  │                                                                │
  │  contracts / value objects                                     │
  │   - SuggestionCandidate                                        │
  │   - SuggestionContext                                          │
  │   - RankedSuggestion                                           │
  │   - TimingDecision                                             │
  │   - SuppressionDecision                                        │
  │   - DisplayDecision                                            │
  │                                                                │
  │  pure DTO boundary only                                        │
  └────────────────────────────────────────────────────────────────┘
                                 │
                                 ▼
  ┌────────────────────── Decision modules ────────────────────────┐
  │                                                                │
  │  ranking            -> what matters most                       │
  │  timing             -> when to show                            │
  │  suppression        -> when not to show                        │
  │  personalization    -> how past user behavior should matter    │
  │  clustering         -> which workflows are semantically alike  │
  │  proposal wording   -> how suggestions should be phrased       │
  │  display engine     -> final orchestration                     │
  │                                                                │
  │  output: final display-quality decisions                       │
  └────────────────────────────────────────────────────────────────┘
                                 │
                                 ▼
  ┌────────────────────── Adapter surface ─────────────────────────┐
  │                                                                │
  │  adapter / evaluate_for_display(...)                           │
  │   - one high-level entry point for flowd                       │
  │   - hides internal intelligence complexity                     │
  └────────────────────────────────────────────────────────────────┘
                                 │
                                 ▼
  ┌────────────────────── Replay harness ──────────────────────────┐
  │                                                                │
  │  replay / evaluation                                           │
  │   - fixture scenarios                                          │
  │   - deterministic policy regression checks                     │
  │   - ranking/timing/suppression/display validation              │
  └────────────────────────────────────────────────────────────────┘

                                 │
                                 ▼
┌─────────────────────────────────────────────────────────────────────┐
│                              flowd                                  │
│                     Back from intelligence layer                    │
└─────────────────────────────────────────────────────────────────────┘

  ┌────────────────────── CLI / UX layer ──────────────────────────┐
  │                                                                │
  │  flow-cli                                                      │
  │   - patterns                                                   │
  │   - suggestions                                                │
  │   - sessions                                                   │
  │   - automations                                                │
  │   - runs                                                       │
  │   - approve / reject / snooze                                  │
  │   - dry-run / run / undo                                       │
  │                                                                │
  │  intelligence may influence:                                   │
  │   - ordering                                                   │
  │   - wording                                                    │
  │   - delay / suppress decisions                                 │
  │                                                                │
  │  but flow-cli still works without private intelligence         │
  └────────────────────────────────────────────────────────────────┘
                                 │
                                 ▼
  ┌────────────────────── Execution layer ─────────────────────────┐
  │                                                                │
  │  flow-dsl                                                      │
  │   - automation specification                                   │
  │                                                                │
  │  flow-exec                                                     │
  │   - planner                                                    │
  │   - dry-run                                                    │
  │   - execution                                                  │
  │   - undo                                                       │
  │                                                                │
  │  supported safe actions now:                                   │
  │   - rename                                                     │
  │   - move                                                       │
  └────────────────────────────────────────────────────────────────┘