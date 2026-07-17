# Support

This document is how `flowd` collects and works support — for open-core users
and for Private Intelligence as the product layer.

## Where to ask

| Channel | Use for | Public? |
| --- | --- | --- |
| [GitHub Issues](https://github.com/nickvd7/flowd/issues/new/choose) | Bugs, usage help, suggestion-quality notes, open-core features | Yes |
| [flowd.net contact](https://flowd.net/#contact) | Private Intelligence evaluation, commercial / partner questions, non-public support | No |
| Email (on the contact page) | Same as contact form when the form is inconvenient | No |

Prefer **GitHub Issues** whenever the report can be public and redacted.
That keeps a searchable backlog the project can prioritize.

## Issue types (templates)

When you open an issue, pick a template:

1. **Bug report** — broken or unsafe open-core behavior
2. **Support / how do I…** — setup, config, commands
3. **Suggestion quality / dogfood note** — false positives, misses, timing, trust
4. **Feature request** — concrete capability aligned with roadmap

Blank issues are disabled so every report lands in a triage-friendly shape.

## Labels we use

| Label | Meaning |
| --- | --- |
| `needs-triage` | New; not yet classified |
| `bug` | Confirmed defect track |
| `support` | Usage help |
| `suggestion-quality` / `dogfood` | Real-world quality signal |
| `enhancement` | Feature request |
| `intelligence` | Touches Private Intelligence boundary or ranking/timing |
| `good first issue` | Small, well-scoped for contributors |
| `blocked` | Waiting on info or external dependency |

Maintainers (or Cursor cloud agents) retag during triage.

## Triage loop (maintainers)

Aim for a light weekly cadence:

1. **Inbox** — everything with `needs-triage`
2. **Classify** — bug / support / quality / feature / commercial→contact
3. **Prioritize**
   - P0: safety (allowlist, unintended writes), data loss risk
   - P1: broken core loop (suggest → approve → dry-run → run → undo)
   - P2: suggestion quality / intelligence tuning
   - P3: docs, packaging, nice-to-haves
4. **Respond** — ask for missing repro, or link docs, or accept into backlog
5. **Close** — duplicate, out of scope, or answered support

Dogfood / quality issues often become fixtures or intelligence policy tweaks,
not always immediate code changes.

## Open-core vs Private Intelligence

- **Open-core bugs and usage** → public GitHub Issues
- **“Intelligence ranking feels wrong”** → public issue with template
  *Suggestion quality*, mark intelligence build = yes (helps product tuning)
- **Access, licensing, evaluation, paid support** → [contact form](https://flowd.net/#contact)
  (do not discuss private terms in public issues)

Public CI and default installs do **not** depend on the private crate. Support
answers should say clearly whether a fix belongs in `flowd` or
`flowd-intelligence`.

## What to include (and redact)

Include:

- platform + `flowctl --version` / commit
- minimal repro commands
- whether `--features intelligence` is enabled

Redact:

- secrets, tokens, cookies
- full personal directory listings
- customer / employer-identifying paths when possible

`~/Downloads/invoice-….pdf` style redaction is enough for most bugs.

## Agent / automated pickup

Cloud agents and contributors should:

1. Only pick issues labeled beyond `needs-triage` **or** clearly actionable bugs
2. Prefer issues with repro steps
3. Link the PR back to the issue (`Fixes #NNN`)
4. Not scrape contact-form email into public issues

For quality notes without a hard repro, prefer documenting a fixture or a
follow-up in `docs/dogfooding.md` rather than a speculative code change.

## Related

- [Private Intelligence](./intelligence.md)
- [Dogfooding](./dogfooding.md)
- [Roadmap to 1.0](./roadmap-v1.md)
- [Contributing](../CONTRIBUTING.md)
