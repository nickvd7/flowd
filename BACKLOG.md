# BACKLOG

## Now

### feat/workflow-pack-registry-client
PR: feat: add workflow pack registry client

Goal:
Support installing packs from a remote registry / hub.

Acceptance:
- cargo build passes
- cargo test passes
- `flowctl packs search` / install by pack id against a local or HTTPS registry index

## Next

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
- terminal `mv`/`cp` into directory destinations and multi-source transfers
- optional `--features intelligence` client wiring to `flowd-intelligence`
- richer `flowctl suggestions explain` output
- workflow pack install (`flowctl packs install`)
- `flowctl doctor` / `flowctl status`
- `flowctl insights`
- dry-run-first enforcement
- terminal history bridge observer
- teach-from-session
- CI workflow for cargo build/test
