# Browser Bridge (privacy-safe observation)

`flowd` can observe browser activity through local NDJSON bridge files. This is
observation only. `flowd` does **not** control the browser, click pages, fill
forms, or automate admin actions.

## Downloads bridge

Config:

```toml
observe_browser_downloads = true
browser_downloads_bridge_path = "~/.flowd/browser-downloads.ndjson"
strip_browser_query_strings = true
```

Each complete line is one download record:

```json
{"ts":"2026-03-13T10:00:02Z","filename":"invoice-1001.pdf","path":"/home/you/Downloads/invoice-1001.pdf","browser":"chrome","source_url":"https://example.test/file.pdf","page_url":"https://example.test/invoices"}
```

Query strings are stripped by default.

When `auto_run_approved_automations` and `auto_run_on_browser_downloads` are
both enabled, a completed download path can trigger path-scoped auto-run for
matching approved file automations (still requires a prior dry-run).

## Visits bridge

Config:

```toml
observe_browser_visits = true
browser_visits_bridge_path = "~/.flowd/browser-visits.ndjson"
strip_browser_query_strings = true
```

Each complete line is one visit record:

```json
{"ts":"2026-03-13T10:00:02Z","url":"https://example.test/docs","title":"Docs","browser":"chrome"}
```

Visit events enrich local observation. They never drive browser automation.

## Non-goals

- Playwright / CDP control
- Password manager or admin UI automation
- Cloud sync of browsing history
