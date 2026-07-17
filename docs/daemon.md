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

`flowctl daemon status` prints the resolved log path.

## Crash recovery

- `flowctl daemon start` writes a pid file and refuses a second live process
- `flowctl daemon status` clears a stale pid file after a crash (`kill -9`)
- systemd user units use `Restart=on-failure` / `RestartSec=2`
- launchd agents use `KeepAlive=true`

Automated soak coverage lives in
`crates/flow-cli/tests/daemon_lifecycle.rs` (stale-pid clear + restart cycle,
plus contrib restart-policy checks).

## Log rotation

`flowd` appends to a single log file and does not rotate in-process. Use the
platform logger.

### Linux (pid-managed via `flowctl daemon start`)

Example `/etc/logrotate.d/flowd` (adjust the path to your runtime dir):

```
/home/YOU/.flowd/flow-daemon.log {
    weekly
    rotate 4
    compress
    missingok
    notifempty
    copytruncate
}
```

`copytruncate` avoids needing to restart the daemon after rotation.

If you use `$XDG_RUNTIME_DIR/flowd/flow-daemon.log`, prefer a user logrotate
snippet or move logging to a durable directory under `~/.flowd`.

### Linux (systemd user unit)

Prefer journald and limit retention:

```bash
mkdir -p ~/.config/systemd/user/flow-daemon.service.d
cat > ~/.config/systemd/user/flow-daemon.service.d/logging.conf <<'EOF'
[Service]
StandardOutput=journal
StandardError=journal
EOF
systemctl --user daemon-reload
systemctl --user restart flow-daemon.service
```

Then keep journal size bounded, for example in `/etc/systemd/journald.conf.d/flowd.conf`:

```
[Journal]
SystemMaxUse=200M
```

### macOS (launchd)

The generated agent writes through the daemon's own stdout/stderr when started
by `flowctl`. For a dedicated path, set in the plist:

```xml
<key>StandardOutPath</key>
<string>/Users/YOU/.flowd/flow-daemon.log</string>
<key>StandardErrorPath</key>
<string>/Users/YOU/.flowd/flow-daemon.log</string>
```

Rotate with `newsyslog` (system) or a periodic `truncate`/`mv` script. Example
`newsyslog` line:

```
/Users/YOU/.flowd/flow-daemon.log  644  5  1024  *  JC
```

### Homebrew service

The formula service block logs to `$(brew --prefix)/var/log/flowd.log`. Rotate
that file with the same `copytruncate` pattern as Linux pid-managed installs.

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
