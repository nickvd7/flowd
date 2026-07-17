# Roadmap to flowd 1.0

This document is the production checklist for moving from the current `0.3.x`
pre-release line to a trustworthy `1.0.0` release.

## Product promise for 1.0

`flowd` 1.0 is a **local-first file workflow engine**:

- observe repeated rename/move workflows
- suggest automations in the CLI
- require explicit approval
- dry-run before run
- undo completed runs
- keep all capture and state on-device by default

It is **not** a general desktop agent, browser controller, or cloud sync product.

## Exit criteria

Copied and expanded from `docs/PLAN.md`:

- [ ] daemon runs reliably in the background on Linux and macOS
- [ ] repeated download/file workflows are detected without special demo setup
- [ ] at least one safe automation can be approved, dry-run, run, and undone
- [ ] false positives stay low enough for daily use
- [ ] execution paths cannot escape the configured allowlist
- [ ] install path is documented and works from a tagged release
- [ ] privacy defaults remain safe (adapters off, auto-run off, redaction on)

## Workstreams

### 1. Dogfooding and quality bar

- Run real Downloads/Desktop workflows for sustained use
- Track false positives, misses, and timing issues in notes
- Tune anti-annoyance defaults from real feedback

### 2. Release hardening

- [x] versioned release workflow for tagged builds
- [x] `scripts/install.sh` source installer
- [x] `flowctl` binary alias
- [ ] signed/notarized macOS artifacts (post-1.0 candidate if needed)
- [ ] Homebrew formula or equivalent package once tags are stable

### 3. Daemon lifecycle

- [x] `flowctl daemon start|stop|status`
- [x] `flowctl daemon install-service` (systemd user / launchd)
- [x] contrib unit templates under `contrib/`
- [ ] crash-recovery soak tests
- [ ] log rotation guidance validated on both platforms

### 4. Safety

- [x] execution path allowlist (`observed_folders` / `execution_allowed_roots`)
- [x] enforce allowlist on dry-run, run, undo, and daemon auto-run
- [ ] stronger canonicalize edge-case matrix for symlinks
- [ ] audit-friendly run summaries for every apply/undo

### 5. Packaging and docs

- [x] `docs/daemon.md`
- [x] `docs/release-notes-v0.3.md`
- [ ] upgrade guide from 0.2/0.3 to 1.0
- [ ] supported platforms matrix in README

## Suggested release train

1. `0.3.0` — v0.3 features + hardening baseline (this branch)
2. `0.3.x` — dogfood fixes only
3. `1.0.0` — after exit criteria above are checked off with real usage evidence

## Explicit non-goals for 1.0

- cloud sync of capture data
- remote team admin dashboards
- browser control / CDP automation
- destructive shell automation
- delete actions in the automation DSL
