# Contributing

## Language requirement

All project text must be written in English, including:
- documentation
- code comments
- commit messages
- issue discussions
- pull request descriptions

## Support and issues

User support and bug reports go through GitHub Issue templates (not blank
issues). Read [Support](docs/support.md) for channels, labels, and triage.

- Open-core bugs / usage → [new issue](https://github.com/nickvd7/flowd/issues/new/choose)
- Private Intelligence commercial / evaluation → [flowd.net contact](https://flowd.net/#contact)

When picking up work from issues: prefer items that left `needs-triage`, include
a repro, and link PRs with `Fixes #NNN`.

## Local setup

```bash
cargo build
cargo test
cargo run -p flow-cli -- --help
```

## Development rules
- keep tasks small
- add tests with every change
- prefer deterministic logic over model-dependent behavior
- do not add cloud dependencies to the open core
- avoid scope creep in v1

## Suggested workflow
1. pick one task from `TASKS.md`
2. make one focused branch or PR
3. add or update fixtures when behavior changes
4. run workspace tests
