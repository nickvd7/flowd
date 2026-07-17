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

Open-core config knobs mapped into the adapter:

```toml
intelligence_enabled = true
intelligence_rejected_cooldown_secs = 14400
intelligence_snoozed_cooldown_secs = 7200
intelligence_shown_cooldown_secs = 7200
intelligence_minimum_score_for_show = 12.0
suggestion_daily_cap = 8
local_llm_enabled = false
local_llm_endpoint = "http://127.0.0.1:11434"
local_llm_model = "llama3.2"
```

Delay decisions persist as `freshness = delayed` so they stay out of the current
pending list. Local LLM labeling is localhost-only metadata for
`flowctl suggestions explain` and never executes actions.
