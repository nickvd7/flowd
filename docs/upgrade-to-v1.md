# Upgrade guide: 0.2 / 0.3 → 1.0

This guide covers upgrading an existing local `flowd` install toward the 1.0
safety and packaging baseline. It applies to current `0.3.x` pre-releases and to
older `0.2` setups.

## Before you upgrade

1. Stop the daemon:

```bash
flowctl daemon stop
# or stop your systemd/launchd unit if you installed one earlier
```

2. Back up local state:

```bash
cp ./flowd.db ./flowd.db.bak
cp ./flowd.toml ./flowd.toml.bak 2>/dev/null || true
cp ~/.config/flowd/config.toml ~/.config/flowd/config.toml.bak 2>/dev/null || true
```

3. Note which folders you observe and which automations you trust.

## Install the new binaries

Prefer one of:

```bash
# source installer
./scripts/install.sh

# or cargo
cargo install --path crates/flow-cli --force
cargo install --path crates/flow-daemon --force

# or Homebrew (HEAD until a stable tap formula is published)
brew install --HEAD --formula Formula/flowd.rb

# After tagging a stable release, compute the bottle/source sha:
#   ./scripts/homebrew-sha256.sh 1.0.0
# then uncomment url/sha256/version in Formula/flowd.rb
```

Confirm:

```bash
flowctl --version || flow-cli --version
flow-daemon --help >/dev/null
```

## Config changes to review

### Execution path allowlist

`1.0` hardening enforces an execution allowlist on dry-run, run, undo, and
daemon auto-run.

- Default roots come from `observed_folders`
- Override with `execution_allowed_roots` when destinations live outside watch folders
- Tests may set `FLOWD_UNRESTRICTED_EXECUTION=1`; do **not** use that in production

Example:

```toml
observed_folders = ["~/Downloads", "~/Desktop"]
execution_allowed_roots = [
  "~/Downloads",
  "~/Desktop",
  "~/Documents/Accounting",
]
enforce_execution_path_allowlist = true
```

If an automation destination is outside those roots, `flowctl dry-run` / `run`
will fail closed with an allowlist error. That is intentional.

### Daemon lifecycle

Prefer the managed commands:

```bash
flowctl daemon install-service
flowctl daemon start
flowctl daemon status
```

See [Daemon lifecycle](./daemon.md).

### Privacy defaults

Keep adapters and auto-run off unless you explicitly need them:

- `observe_clipboard = false`
- `observe_browser_downloads = false`
- `observe_terminal = false`
- `auto_run_approved_automations = false`
- `intelligence_enabled = false`
- redaction flags remain on by default

## Database and automations

- Existing SQLite databases migrate forward through the normal migration path
- Approved automations remain; re-run `flowctl dry-run <id>` once after upgrade
- If a dry-run now fails on allowlist escapes, widen `execution_allowed_roots` or
  narrow the automation destination

## Validation checklist

```bash
flowctl config validate
flowctl status
flowctl suggestions
flowctl automations
flowctl dry-run <id>
```

Optional simulated regression (developer checkout):

```bash
cargo test -p flow-analysis --test simulated_dogfood
cargo test -p flow-core paths
```

## Breaking / behavioral notes

| Area | 0.2 / early 0.3 | 1.0 baseline |
| --- | --- | --- |
| Path safety | weaker / inconsistent | allowlist + symlink canonicalize |
| Daemon control | mostly manual process | `flowctl daemon *` + contrib units |
| Install | cargo only | `install.sh`, release workflow, Homebrew formula |
| Binary name | `flow-cli` | `flowctl` preferred (`flow-cli` still built) |

## After upgrade

1. Re-approve only what you still trust
2. Keep auto-run off until dry-runs look right for a few days
3. Use [Dogfooding](./dogfooding.md) for real-world notes; CI also runs the
   simulated dogfood harness as a deterministic stand-in
