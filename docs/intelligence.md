# Optional intelligence integration

`flowd` stays fully functional without private ranking. Open-core always owns
facts and actions. Private intelligence may only influence presentation.

## Dual gate

1. Build with Cargo feature `intelligence`
2. Set `intelligence_enabled = true` in config

If either gate is off, suggestions use the deterministic open-core baseline and
`NoopIntelligenceClient`.

## Local build

Requires a sibling checkout of `flowd-intelligence` next to `flowd`:

```text
repos/
  flowd/
  flowd-intelligence/
```

```bash
cargo install --path crates/flow-cli --features intelligence
cargo install --path crates/flow-daemon --features intelligence
```

Public CI runs without the feature so it never depends on the private crate.

## Boundary

```text
flowd -> flowd-intelligence
```

Mapping lives in `crates/flow-analysis/src/private_intelligence_client.rs` and
calls `flowd_intelligence::contracts::evaluate_for_display`.
