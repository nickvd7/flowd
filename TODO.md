# Open TODO (copy/paste)

Checklist of what is still open after the v0.3 / 1.0 hardening track.
Simulated coverage is largely done; the blockers below are mostly human or release steps.

## Blocks real 1.0

- [ ] Dogfood locally for sustained use (Downloads / Desktop / real workflows)
- [ ] Log false positives, misses, timing issues (see `docs/dogfooding.md`)
- [ ] Tune anti-annoyance defaults from that feedback (`suggestion_daily_cap`, cooldowns, freshness)
- [ ] Confirm false positives are low enough for daily use

## Release / packaging

- [ ] Tag `v0.3.0` (or jump to `v1.0.0` only after dogfood exit criteria)
- [ ] Publish GitHub release assets from the tag workflow
- [ ] Fill Homebrew stable `url` / `sha256` / `version` via `./scripts/homebrew-sha256.sh <version>`
- [ ] Publish or document the stable Homebrew install line (not HEAD-only)
- [ ] (Later) Signed / notarized macOS artifacts

## Private Intelligence / commercial

- [ ] Set real Eval / Pro / Team prices and terms (site currently routes to contact)
- [ ] Eval onboarding: license file + docs checklist for early users
- [ ] Replace unsigned license skeleton with signed token verification
- [ ] Productize explain-reason UX if dogfood asks for clearer copy

## Optional polish (not 1.0 blockers)

- [ ] More simulated dogfood / quality fixtures from real notes
- [ ] Windows support decision (explicitly out of 1.0 today)
- [ ] Crash-recovery soak under real systemd/launchd (unit tests already cover pid restart)

## Quick links

- Dogfooding: `docs/dogfooding.md`
- Roadmap: `docs/roadmap-v1.md`
- Support: `docs/support.md`
- Intelligence / pricing: `docs/intelligence.md` · https://flowd.net/#pricing
- Issues: https://github.com/nickvd7/flowd/issues/new/choose
