# Private Intelligence

`flowd` is an **open-core** workflow engine.  
`flowd-intelligence` is the **private decision-quality layer** — the product value that makes suggestions quiet, timely, and worth acting on.

## The business split

| Layer | What it owns | Distribution |
| --- | --- | --- |
| **Open core (`flowd`)** | Capture, SQLite, sessions, patterns, baseline suggestions, approve / dry-run / run / undo, allowlist safety | MIT, this repository |
| **Private Intelligence (`flowd-intelligence`)** | Ranking, timing, suppression, personalization, clustering, proposal wording / labeling, display decisions | Private product layer |

One-line rule:

> Open-core owns **facts and actions**. Private Intelligence owns **which suggestions you see, when, and how they are worded**.

That is intentional: infrastructure can be open; **decision quality is the product**.

## Why it exists

Without intelligence, open-core still detects repeated file workflows and can suggest automations. That baseline is honest and useful — and also noisier.

Private Intelligence answers the product questions that turn detection into daily trust:

- Which suggestion should appear first?
- Is now the right moment, or should this wait?
- Should this be suppressed after a reject / snooze / recent show?
- Do similar workflows belong together?
- How should the proposal be labeled so a human recognizes it instantly?

Those decisions are the moat. They are not required for the engine to run.

## What users get

With Private Intelligence enabled (local evaluation, still on-device):

- Fewer low-value suggestions in the CLI
- Better ordering by usefulness and feedback history
- Cooldowns after reject / snooze / recent shows
- Optional daily suggestion cap (anti-annoyance)
- Clearer wording / labels (optional localhost LLM for explain metadata only)

Without it:

- Deterministic baseline suggestions still work
- Approve → dry-run → run → undo still work
- Nothing about capture or execution moves to the cloud

**Private does not mean cloud.** It means a closed decision policy, evaluated locally against your SQLite history.

## Dual gate (+ local entitlement)

All of the following must pass:

1. Build with Cargo feature `intelligence`
2. Set `intelligence_enabled = true` in config
3. Present a valid local entitlement **or** enable dev mode

If any gate fails, suggestions use the open-core baseline and `NoopIntelligenceClient`.

### Local entitlement (skeleton)

License file (default path):

```text
~/.config/flowd/intelligence.license.toml
```

Override path with `FLOWD_INTELLIGENCE_LICENSE`. Example:

```bash
cp examples/intelligence.license.toml ~/.config/flowd/intelligence.license.toml
```

```toml
schema_version = 1
tier = "eval"          # eval | pro | team | dev
issued_to = "you@example.com"
expires_at = "2099-12-31T23:59:59Z"
token = "local-unsigned-v1:replace-me"
```

Token verification is **unsigned for now** (file shape + expiry only). Signed
tokens can land later without changing the public path.

Developer override (sibling checkout / CI local work):

```bash
export FLOWD_INTELLIGENCE_DEV=1
```

Check status:

```bash
flowctl doctor
# intelligence layer: connected · licensed (eval), … 
# or: blocked · entitlement: missing license (…)
```

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

Intelligence may only influence **presentation**. It must not:

- invent filesystem facts
- bypass approval
- execute automations
- sync capture data off-device

## Config knobs

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

Team policy packs can force intelligence off on shared machines — see
[Team Policy](./team-policy.md).

## Explain reasons

When intelligence hides or delays a suggestion, `flowctl suggestions --explain`
and `flowctl suggestions explain <id>` show stable codes plus a short message:

| Code | Meaning |
| --- | --- |
| `timing.too_new` | Pattern is still too new |
| `timing.too_weak` | Signal too weak to show yet |
| `timing.low_confidence` | Below confidence threshold |
| `suppression.recently_shown` | Shown recently |
| `suppression.cooldown_after_reject` | Reject cooldown |
| `suppression.cooldown_after_snooze` | Snooze cooldown |
| `suppression.too_many_dismissals` | Dismissed too often |
| `suppression.low_score` | Score not credible enough |
| `suppression.low_freshness` | Workflow too stale |

Factors may also include `delay_until_ts` / `suppress_until_ts`.

## Pricing tiers (product)

| Tier | Includes |
| --- | --- |
| **Open core** | MIT engine: observe, baseline suggestions, approve → dry-run → run → undo |
| **Eval** | Time-boxed Private Intelligence for evaluation (`tier = "eval"`) |
| **Pro** | Individual Private Intelligence: ranking, timing, suppression, explain codes |
| **Team** | Pro + shared policy packs / commercial support lane |

Commercial access and partner questions: [flowd.net/#contact](https://flowd.net/#contact).

## Product messaging (canonical)

Use these lines in homepage, README, and release notes:

- **Engine:** “flowd watches repeated local file work and suggests automations you approve.”
- **Intelligence:** “Private Intelligence decides which suggestions surface, when, and how they’re worded — so the CLI stays quiet and useful.”
- **Boundary:** “Turn intelligence off and the engine still works. Decision quality is the paid layer; capture and execution stay open and local.”
- **Entitlement:** “Open-core is free. Private Intelligence unlocks with a local license file — still on-device.”

## Support

- Open-core bugs / usage → public [GitHub Issues](https://github.com/nickvd7/flowd/issues/new/choose)
- Intelligence evaluation / commercial → [flowd.net contact](https://flowd.net/#contact)
- Process: [Support](./support.md)

## Related docs

- [Support](./support.md)
- [System Overview](./system-overview.md)
- [Architecture](./architecture.md)
- [Privacy](./privacy.md)
- [Team Policy](./team-policy.md)
