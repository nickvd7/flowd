# Release Checklist

- [x] `cargo build` succeeds
- [x] `cargo test --workspace` succeeds
- [x] fixture replay tests pass
- [x] full open-core loop passes in automation: observe -> detect -> suggest -> approve -> dry-run -> run -> undo
- [x] dry-run behavior reviewed (required before `run` unless `--force`)
- [x] execution behavior reviewed
- [x] undo behavior reviewed
- [x] safety filters reviewed
- [x] sample config matches implementation
- [x] docs reflect current code
- [x] all contributor-facing text is in English
- [ ] GitHub release notes drafted for `v0.2.0`
- [ ] tagged release cut after merge
