# Demo Scenarios

These fixtures are deterministic replay scenarios for tests, local demos, and documentation.

Each scenario:

- uses only local event sources that `flowd` already supports
- replays a realistic repeated workflow
- stays deterministic by using fixed timestamps and local-only paths
- avoids GUI steps and cloud dependencies

The canonical index is [`manifest.json`](/Users/nickvandort/Documents/Coding/flowd/fixtures/demo_scenarios/manifest.json). It lists the included scenarios, the fixture file for each one, and the expected replay result used by regression tests.

Included scenarios:

- `invoice_organization`: browser download plus file rename and move into accounting
- `screenshot_cleanup`: desktop screenshot creation followed by archival move
- `downloads_sorting`: browser download plus file move into a finance statements folder
- `terminal_file_organization`: terminal-observed rename and move workflow

These files are intended to stay readable enough to reference directly in docs and future demo tooling.
