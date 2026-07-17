# BACKLOG

Canonical open checklist (copy/paste friendly): **[`TODO.md`](./TODO.md)**.

## Now

### release/v1-human-dogfood
Sustained real Downloads/Desktop use and false-positive notes.
Simulated stand-in: `cargo test -p flow-analysis --test simulated_dogfood`.
See `docs/dogfooding.md`, `docs/roadmap-v1.md`, and `TODO.md`.

## Next

### release/v1-tag
Tag `v0.3.0` / `v1.0.0` after human dogfood; fill Homebrew stable sha via
`./scripts/homebrew-sha256.sh <version>`.

### intelligence/commercial
Real Eval/Pro/Team terms, eval onboarding, signed license tokens.
See `TODO.md` and `docs/intelligence.md`.

## Later

- signed/notarized macOS artifacts

## Done recently
- intelligence entitlement skeleton, homepage pricing tiers, stable explain reason codes
- support process + GitHub issue templates
- Private Intelligence product messaging (site + docs)
- homepage refresh (responsive hamburger nav)
- audit run summaries, daemon soak tests, log rotation docs, desktop_pdf_archive scenario
- simulated dogfood harness + symlink allowlist matrix + Homebrew formula + upgrade-to-v1 docs
- v1 hardening baseline: path allowlist, daemon lifecycle, release workflow, install script
