# Fixtures

Add JSON or YAML fixture files here for replay-driven tests.

`fixtures/demo_scenarios/` now contains the canonical deterministic demo scenarios used for tests, docs references, and future replay tooling.

Current scenario set:

- `invoice_organization`
- `screenshot_cleanup`
- `downloads_sorting`
- `terminal_file_organization`

The manifest at [`fixtures/demo_scenarios/manifest.json`](demo_scenarios/manifest.json) is the entry point for loading those fixtures programmatically.
