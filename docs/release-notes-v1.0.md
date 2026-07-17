# flowd v1.0.0 (candidate notes)

`v1.0.0` is the trustworthy local-first release of the file workflow engine:
observe repeated rename/move workflows, suggest automations, require approval,
dry-run before run, undo completed runs, and keep capture on-device.

These notes track the 1.0 baseline. Tag only after human dogfood confirms the
false-positive bar in `docs/roadmap-v1.md`.

## Highlights

- Clear open-core + **Private Intelligence** product split (decision quality
  layer; still local) — see `docs/intelligence.md`
- Execution path allowlist with symlink canonicalize matrix
- `flowctl daemon start|stop|status|install-service` plus contrib units
- Audit trail for every dry-run / apply / undo via `flowctl runs` and
  `flowctl runs show <id>`
- Simulated dogfood harness for deterministic quality regression
- Source installer, tagged Linux release workflow, Homebrew formula

## Safety and trust

- Dry-run-first remains the default safety gate
- Undo is per-run and inspectable
- Destinations outside `observed_folders` /
  `execution_allowed_roots` fail closed
- Privacy defaults stay safe (adapters off, auto-run off, redaction on)

## Packaging

- `./scripts/install.sh`
- GitHub Actions release assets on `v*` tags
- Homebrew: `Formula/flowd.rb` (HEAD today; fill stable `url`/`sha256` after tag)

## Upgrade notes

See [Upgrade to 1.0](./upgrade-to-v1.md).

Key config keys:

- `execution_allowed_roots`
- `enforce_execution_path_allowlist`

Prefer `flowctl` over `flow-cli`. Prefer `flowctl daemon *` over a bare
`flow-daemon` process.

## Known limits / non-goals

Still out of scope for 1.0:

- cloud sync of capture data
- remote team admin dashboards
- browser control / CDP automation
- destructive shell automation / delete actions in the DSL
- signed/notarized macOS artifacts (post-1.0 candidate)
