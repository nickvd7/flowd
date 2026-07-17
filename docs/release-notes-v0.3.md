# flowd v0.3.0

`v0.3.0` is the production-hardening step after the v0.2 local workflow MVP. It
keeps the same product promise — local-first file workflows with approval,
dry-run, and undo — while adding the controls needed for a trustworthy path to
`1.0.0`.

## Highlights

- Execution path allowlist for dry-run, run, undo, and daemon auto-run
- `flowctl daemon start|stop|status|install-service`
- systemd user unit and launchd agent templates
- Source installer script and tagged release workflow
- `flowctl` binary alias alongside `flow-cli`
- v0.3 intelligence/policy polish carried from recent mainline work

## Upgrade notes

- New config keys:
  - `execution_allowed_roots` (empty = use `observed_folders`)
  - `enforce_execution_path_allowlist` (default `true`)
- Automations whose `from`/`to` paths leave the allowlist will now fail closed
- Prefer `flowctl daemon start` instead of a bare `flow-daemon` process

See `docs/roadmap-v1.md` for the remaining checklist before `1.0.0`.
