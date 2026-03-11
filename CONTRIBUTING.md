# Contributing

## Language requirement

All project text must be written in English, including:
- documentation
- code comments
- commit messages
- issue discussions
- pull request descriptions

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
