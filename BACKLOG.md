
## `flowd/BACKLOG.md`
```md
# BACKLOG

## Now

### feat/workflow-pack-support
PR: feat: add workflow pack support

Goal:
Add installable workflow packs with:
- workflow-pack.toml
- validation
- local installation
- compatibility with existing automation specs

Acceptance:
- cargo build passes
- cargo test passes
- local workflow packs can be installed safely

---

### feat/flowctl-doctor
PR: feat: add flowctl doctor diagnostics command

Goal:
Add a CLI diagnostic command that checks:
- daemon
- database
- watch paths
- event ingestion
- patterns
- suggestions
- automations
- intelligence client state if enabled

Acceptance:
- cargo build passes
- cargo test passes
- doctor output is clear and deterministic

---

### feat/flowctl-watch-improvements
PR: feat: improve flowctl watch event visibility and filtering

Goal:
Improve `flowctl watch` with:
- category labels
- less noise
- optional filters

Acceptance:
- cargo build passes
- cargo test passes
- output is more useful for dogfooding and debugging

## Next

### feat/suggestions-explain-improvements
PR: feat: improve suggestion explainability output

Goal:
Make explanations clearer with:
- repetitions
- recency
- confidence
- estimated usefulness
- representative traces

---

### feat/usage-insights
PR: feat: add flowctl insights command

Goal:
Show:
- most common workflows
- top automations
- estimated time saved
- unused suggestions

---

### docs/example-workflows
PR: docs: add example workflows for flowd

Goal:
Add 8–10 realistic workflow examples and link them from README.

## Later

### feat/workflow-pack-registry-client
PR: feat: add workflow pack registry client

Goal:
Support installing packs from a remote registry / hub.

---

### feat/terminal-command-patterns
PR: feat: improve terminal command workflow understanding

Goal:
Improve parsing and normalization of repeated shell workflows.

## Icebox

### feat/browser-automation-bridge
Only consider if there is a very strong privacy-safe local design.

### feat/team-admin-controls
Enterprise-oriented, not needed for near-term MVP.