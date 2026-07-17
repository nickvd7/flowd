# BACKLOG

## Now

### feat/intelligence-client-wiring
PR: feat: wire optional private intelligence client

Goal:
Connect `flowd-intelligence` behind a feature flag / client implementation so
`intelligence_enabled = true` does more than the local noop boundary.

Acceptance:
- cargo build passes
- cargo test passes
- open-core remains fully functional with intelligence disabled

---

### feat/suggestions-explain-improvements
PR: feat: improve suggestion explainability output

Goal:
Make explanations clearer with:
- repetitions
- recency
- confidence
- estimated usefulness
- representative traces

## Next

### feat/workflow-pack-registry-client
PR: feat: add workflow pack registry client

Goal:
Support installing packs from a remote registry / hub.

---

### feat/terminal-command-patterns
PR: feat: improve terminal command workflow understanding

Goal:
Improve parsing and normalization of repeated shell workflows beyond the NDJSON bridge.

### docs/example-workflows
PR: docs: expand example workflows for flowd

Goal:
Keep 8–10 realistic workflow examples current and linked from README.

## Later

### feat/browser-automation-bridge
Only consider if there is a very strong privacy-safe local design.

### feat/team-admin-controls
Enterprise-oriented, not needed for near-term MVP.

## Done recently
- workflow pack install (`flowctl packs install`)
- `flowctl doctor` / `flowctl status`
- `flowctl insights`
- dry-run-first enforcement
- terminal history bridge observer
- teach-from-session
- CI workflow for cargo build/test
