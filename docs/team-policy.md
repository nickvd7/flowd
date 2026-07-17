# Local Team Policy Packs

Team admin for early `flowd` stays local-first. Instead of cloud accounts or
remote device control, a team can share a small policy pack that clamps risky
settings on a machine.

## Export / import

```bash
flowctl policy export --output ./team-policy.toml --name accounting-laptops
flowctl policy import --path ./examples/policy/safe-defaults.toml
flowctl policy import --path ./examples/policy/safe-defaults.toml --write
```

Without `--write`, import shows the clamped config. With `--write`, it updates
the active config file (or the preferred setup path when using defaults).

## What a policy can clamp

- force auto-run off
- force intelligence off
- maximum suggestion daily cap
- observed folder allowlist
- local LLM labeling off
- browser visit observation off

See `examples/policy/safe-defaults.toml`.

## Related team catalog

Shared workflow packs can still be distributed through a pack registry index
(`docs/pack-registry.md`) without giving up local approval and dry-run gates.
