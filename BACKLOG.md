# BACKLOG

## Now

### feat/terminal-command-patterns
PR: feat: improve terminal command workflow understanding

Goal:
Improve parsing and normalization of repeated shell workflows beyond the NDJSON bridge.

Acceptance:
- cargo build passes
- cargo test passes
- richer terminal sequence signatures for repeated file-oriented shell workflows

## Next

### feat/workflow-pack-registry-client
PR: feat: add workflow pack registry client

Goal:
Support installing packs from a remote registry / hub.

---

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
- optional `--features intelligence` client wiring to `flowd-intelligence`
- richer `flowctl suggestions explain` output
- workflow pack install (`flowctl packs install`)
- `flowctl doctor` / `flowctl status`
- `flowctl insights`
- dry-run-first enforcement
- terminal history bridge observer
- teach-from-session
- CI workflow for cargo build/test
