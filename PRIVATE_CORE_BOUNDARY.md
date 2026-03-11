# PRIVATE_CORE_BOUNDARY.md — open-core vs proprietary boundary for flowd

## Goal
Define a clean product and repository boundary so `flowd` can be open source without giving away the parts that are most defensible.

The rule is simple:

- **Open source the substrate**
- **Keep the workflow intelligence proprietary**

---

## 1. What should stay open source

These parts increase trust, contributions, and adoption if they are public.

### A. Core infrastructure
Open:
- daemon skeleton
- CLI
- config loader
- SQLite schema and migrations
- repositories
- raw event model
- normalized event model
- session model
- replay harness
- fixtures
- logging and diagnostics

Why:
- contributors can inspect what is collected
- easier community testing
- credibility for privacy claims

### B. Adapters and plugin interfaces
Open:
- file watcher adapter
- terminal ingestion format
- browser event bridge format
- active-window provider traits
- clipboard adapter interface
- plugin SDK
- extension points

Why:
- ecosystem growth
- users can add integrations you would not build yourself
- strong fit for open-core

### C. Baseline workflow logic
Open:
- deterministic sessionization
- deterministic pattern signatures
- basic repeated-sequence detection
- basic safety filters
- public fixture cases
- public test harness

Why:
- gives the project real utility
- makes the public repo valuable on its own
- good for transparency

### D. Automation substrate
Open:
- automation DSL
- dry-run engine
- executor for safe file automations
- undo log
- audit trail
- validation rules for supported open actions

Why:
- users need to trust execution behavior
- contributors can improve reliability

### E. Docs and examples
Open:
- architecture docs
- privacy docs
- config samples
- local setup guides
- example fixtures
- contributor docs

---

## 2. What should stay proprietary

These parts are your real moat because they determine *quality*, not just capability.

### A. Ranking engine
Private:
- pattern ranking
- confidence scoring
- relevance scoring
- prioritization heuristics
- “worth automating” classifier

Why:
- this is where noise gets filtered out
- hard to copy from outside behavior alone
- directly affects user delight

### B. Suggestion timing engine
Private:
- when to show a suggestion
- suppression logic
- cool-down logic
- anti-annoyance policies
- saturation control
- interruptibility model

Why:
- bad timing ruins the product
- this is subtle and highly product-defining

### C. Semantic clustering and abstraction
Private:
- workflow abstraction layer
- similarity scoring
- canonical pattern generation
- semantic clustering of “same flow, different filenames/paths”
- edge-case collapse logic

Why:
- this turns raw repetitions into human-meaningful workflows
- very hard to get right

### D. Proposal generation quality layer
Private:
- pattern-to-proposal transformation
- concise label generation
- proposal wording
- suggestion summarization
- user-facing explanation quality

Why:
- users judge intelligence by the quality of the explanation
- easy to underestimate, hard to reproduce well

### E. Personalization engine
Private:
- accept/reject learning
- per-user preference adaptation
- scope learning
- category boosts
- long-term suppression memory

Why:
- this is where the product starts feeling personal
- creates lock-in through quality

### F. Expanded safety intelligence
Private:
- advanced danger scoring
- contextual suppression
- hidden high-risk pattern families
- secret-sensitive context heuristics

Why:
- important for production quality
- better not to publish every internal detection rule

---

## 3. Boundary rule of thumb

If a component answers:

- “**How does flowd work?**” → probably open
- “**Why is flowd better than alternatives?**” → probably private

---

## 4. Recommended repo structure

### Public repo
Suggested name:
- `flowd`
- or `flowd-open`

Contains:
- workspace and crates
- adapters
- db
- dsl
- exec
- baseline patterns
- docs
- fixtures
- examples
- CI

### Private repo
Suggested name:
- `flowd-intelligence`
- or `flowd-proprietary`

Contains:
- ranking policy
- clustering engine
- proposal generator
- timing engine
- personalization
- advanced safety rules

---

## 5. Integration model

Use a clean crate boundary.

### Public workspace
Examples:
- `flow-core`
- `flow-db`
- `flow-adapters`
- `flow-patterns`
- `flow-dsl`
- `flow-exec`
- `flow-cli`
- `flow-daemon`

### Private crate
Examples:
- `flow-intelligence`
- `flow-ranking`
- `flow-policy`

The public CLI/daemon should call a small interface such as:

- `rank_patterns(...)`
- `cluster_workflows(...)`
- `generate_proposals(...)`
- `should_show_suggestion(...)`

This lets you keep your private logic swappable and isolated.

---

## 6. What not to keep private

Do **not** make these proprietary unless you want to damage trust:

- what events are collected
- how local storage works
- what file actions are executed
- what the DSL syntax is
- what safety guarantees exist at the execution layer
- whether data leaves the machine

Users need to inspect these.

---

## 7. Licensing suggestion

A practical model:

### Public repo
- MIT or Apache-2.0 if you want maximum adoption
- or MPL-2.0 if you want modifications to core files disclosed

### Private repo
- fully closed source
- distributed only in binary or private crate form

If your goal is fastest ecosystem growth, MIT is the simplest.
If you want a bit more reciprocity in core modifications, MPL-2.0 is a good middle ground.

---

## 8. What v1 should keep private

For the first version, the most important private pieces are:

1. ranking
2. proposal generation
3. suggestion timing
4. feedback memory
5. semantic grouping

If you keep only one thing private, keep **ranking + timing** private.

That pair usually defines whether the product feels magical or annoying.

---

## 9. What v1 should absolutely keep open

For the first version, definitely keep open:

1. config and privacy controls
2. event schema
3. normalization schema
4. automation DSL
5. dry-run and undo behavior
6. baseline tests and fixtures

This builds trust and makes the open repo genuinely useful.

---

## 10. Security and trust boundary

Public promise:
- local-first
- inspectable execution
- no hidden remote calls in open core
- safe automations only in public executor
- explicit approval required

Private promise:
- better ranking
- better relevance
- better wording
- better suppression
- better personalization

This is the right split.

---

## 11. Suggested release sequence

### Step 1
Release public repo with:
- workspace
- CLI
- DB
- adapters
- baseline detector
- DSL
- dry-run
- fixtures

### Step 2
Privately develop:
- ranking
- clustering
- proposal generation
- timing engine

### Step 3
Wire private crate into local builds for your own alpha users

### Step 4
Later decide whether to:
- keep proprietary forever
- offer a paid local add-on
- or ship a “community vs pro” split

---

## 12. Final rule

**Open source the parts users need to trust.  
Keep proprietary the parts users pay for because they work better.**
