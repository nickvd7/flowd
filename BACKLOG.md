# BACKLOG

## Now

### release/v1-human-dogfood
Sustained real Downloads/Desktop use and false-positive notes.
Simulated stand-in: `cargo test -p flow-analysis --test simulated_dogfood`.
See `docs/dogfooding.md` and `docs/roadmap-v1.md`.

## Next

### release/v1-tag
Tag `v0.3.0` / `v1.0.0` after human dogfood; fill Homebrew stable sha via
`./scripts/homebrew-sha256.sh <version>`.

## Later

- signed/notarized macOS artifacts

## Done recently
- audit run summaries (`flowctl runs show`), daemon crash-recovery soak tests, log rotation docs, desktop_pdf_archive dogfood scenario, v1.0 candidate release notes, Homebrew sha helper
- simulated dogfood harness + symlink allowlist matrix + Homebrew formula + upgrade-to-v1 docs
- v1 hardening baseline: path allowlist, daemon lifecycle, release workflow, install script, roadmap-v1
- v0.3 slices: event-triggered auto-run polish, anti-annoyance daily cap + Delay freshness, preference memory CLI, clustering context wiring, local LLM labeling (localhost), browser visit observation bridge, local team policy packs
- workflow pack registry client
- terminal `mv`/`cp` into directory destinations
- optional intelligence client wiring
