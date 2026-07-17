# Privacy

## Principles
- local-first by default
- explicit observed zones
- redact sensitive payloads where possible
- no cloud dependency in the open-core engine
- start with reversible actions only (rename / move)

**Private Intelligence** (optional product layer) also evaluates locally.
“Private” means a closed decision policy — ranking, timing, suppression,
wording — not uploading capture data. See [Private Intelligence](./intelligence.md).

## What flowd collects

| Source | Default | Stored locally | Notes |
|---|---|---|---|
| Filesystem events in `observed_folders` | on (`~/Downloads`) | yes (`raw_events`, `normalized_events`) | paths and event kinds |
| Clipboard | off | optional | metadata-only by default; redacted/plaintext only if configured |
| Browser downloads bridge | off | optional | NDJSON bridge; query strings stripped by default |
| Terminal history bridge | off | optional | NDJSON bridge; command args redacted by default |
| Active window | off / not wired | no | reserved for a future privacy-safe design |

Nothing is uploaded. There is no account, sync service, or telemetry in open-core.

## Where state lives
- SQLite database at `database_path` (default `./flowd.db`)
- Optional config at `./flowd.toml` or `~/.config/flowd/config.toml`
- Optional bridge files under `~/.flowd/` when those observers are enabled
- Installed workflow packs under `~/.config/flowd/packs/`

You can inspect tables directly:

```bash
sqlite3 flowd.db ".tables"
sqlite3 flowd.db "select source, payload_json from raw_events limit 5;"
```

## Redaction matrix

| Field | Behavior |
|---|---|
| Clipboard content | metadata-only by default; optional redacted preview; plaintext only when redaction is disabled |
| Terminal args | redacted tokens (`redact_command_args = true`) |
| Browser URLs | query strings stripped (`strip_browser_query_strings = true`) |
| File paths | stored as observed; limit folders with `observed_folders` |

## What can execute
Approved automations may rename or move matching files. Execution requires:
1. explicit approval (`flowctl approve`)
2. a prior dry-run by default (`flowctl dry-run`, unless `--force`)
3. optional undo of a completed run (`flowctl undo`)

Daemon auto-run is off by default (`auto_run_approved_automations = false`) and still respects the dry-run gate.

## Retention and deletion
flowd does not expire data automatically. Delete the database, bridge files, packs directory, or config to remove local state. Uninstalling the binaries does not by itself delete `flowd.db`.

## Trust model
Users should be able to inspect:
- what is collected
- where it is stored
- what actions can be executed
- what safety constraints exist

Use `flowctl status`, `flowctl config show`, `flowctl suggestions --explain`, and direct SQLite access.
