# Release Checklist

- [ ] `cargo build` succeeds
- [ ] `cargo test --workspace` succeeds
- [ ] fixture replay tests pass
- [ ] full open-core loop passes in automation: observe -> detect -> suggest -> approve -> dry-run -> run -> undo
- [ ] dry-run behavior reviewed
- [ ] execution behavior reviewed
- [ ] undo behavior reviewed
- [ ] safety filters reviewed
- [ ] sample config matches implementation
- [ ] docs reflect current code
- [ ] all contributor-facing text is in English
