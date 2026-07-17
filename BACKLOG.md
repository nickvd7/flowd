# BACKLOG

## Now

### release/v1-human-dogfood
Sustained real Downloads/Desktop use and false-positive notes.
Simulated stand-in: `cargo test -p flow-analysis --test simulated_dogfood`.
See `docs/dogfooding.md` and `docs/roadmap-v1.md`.

## Next

### release/v1-stable-homebrew
Publish a versioned Homebrew tap entry once `v1.0.0` (or chosen `v0.3.x`) tags
have a stable tarball SHA (`Formula/flowd.rb` already supports HEAD).

### release/v1-audit-summaries
Audit-friendly run summaries for every apply/undo.

## Later

- crash-recovery soak tests
- log rotation guidance validated on Linux and macOS
- signed/notarized macOS artifacts

## Done recently
- simulated dogfood harness + symlink allowlist matrix + Homebrew formula + upgrade-to-v1 docs
- v1 hardening baseline: path allowlist, daemon lifecycle, release workflow, install script, roadmap-v1
- v0.3 slices: event-triggered auto-run polish, anti-annoyance daily cap + Delay freshness, preference memory CLI, clustering context wiring, local LLM labeling (localhost), browser visit observation bridge, local team policy packs
- workflow pack registry client
- terminal `mv`/`cp` into directory destinations
- optional intelligence client wiring
