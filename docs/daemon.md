# Daemon lifecycle

`flow-daemon` observes local folders and optional bridges. Prefer managing it
through `flowctl` instead of launching the binary by hand.

## Quick start

```bash
flowctl setup --watch ~/Downloads
flowctl daemon start
flowctl daemon status
flowctl status
```

Stop a daemon started by `flowctl`:

```bash
flowctl daemon stop
```

Logs for pid-file managed processes land in:

- `$XDG_RUNTIME_DIR/flowd/flow-daemon.log` when available
- otherwise `~/.flowd/flow-daemon.log`

## Install as a user service

```bash
flowctl daemon install-service
```

- Linux: writes `~/.config/systemd/user/flow-daemon.service`
  then enable with `systemctl --user enable --now flow-daemon.service`
- macOS: writes `~/Library/LaunchAgents/dev.flowd.daemon.plist`
  then load with `launchctl load ~/Library/LaunchAgents/dev.flowd.daemon.plist`

Templates also live in:

- `contrib/systemd/flow-daemon.service`
- `contrib/launchd/dev.flowd.daemon.plist`

## Safety defaults

Auto-run stays off unless you set `auto_run_approved_automations = true`.
Even then, dry-run-first and the execution path allowlist still apply.
