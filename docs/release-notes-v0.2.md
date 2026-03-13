# flowd v0.2.0-pre

`flowd` remains a pre-release project, but `v0.2.0-pre` makes it substantially more usable as a local-first, terminal-first workflow engine. This update expands the open-core pipeline with broader local observation, clearer explainability, richer local history and usage signals, easier setup, and stronger documentation for safe, deterministic automation.

## Highlights

- Added the first public intelligence boundary integration for optional private display decisions without moving core ownership out of `flowd`.
- Improved suggestion scoring and CLI display so repeated workflows surface more clearly and remain inspectable.
- Added explainability paths for suggestion output, including deterministic fallback explanations when no intelligence layer is available.
- Added local feedback history and usage stats to make suggestion behavior and system value easier to inspect from the terminal.
- Improved first-run setup and onboarding for faster local evaluation.
- Expanded observation with terminal, clipboard, and browser download adapters alongside filesystem observation.
- Added deterministic demo scenario fixtures and example workflow documentation for docs, demos, and regression coverage.

## Core Workflow Engine

- Expanded the open-core engine to persist suggestion feedback history as part of local workflow state.
- Improved deterministic suggestion scoring and display so useful repeated patterns are surfaced more consistently.
- Continued to keep the baseline engine fully functional without any private intelligence dependency.
- Preserved the project’s core operating model: local-first storage, terminal-first control, deterministic analysis, explicit approval, and safe execution with undo.

## Intelligence And Explainability

- Added the initial intelligence boundary integration between `flowd` and the optional private `flowd-intelligence` layer.
- Kept the boundary narrow and one-way: `flowd` owns facts and actions, while private intelligence can only influence display decisions such as ranking, timing, suppression, wording, and clustering.
- Added local explainability normalization so returned decision metadata can be rendered clearly in the CLI.
- Added deterministic fallback explanations so suggestion output stays understandable even when intelligence is disabled, unavailable, or returns no decision.

## Onboarding And Usability

- Improved installer and first-run setup flow with `flowctl setup`, including support for selecting initial watched folders.
- Added clearer terminal workflows for inspecting config, suggestions, explainability, history, automations, and local stats.
- Added local usage stats reporting to help users understand adoption and value without sending data off-machine.
- Added suggestion history views so feedback state remains inspectable and audit-friendly.

## New Adapters And Signal Sources

- Continued filesystem observation as the core event source for repeated file workflows.
- Added terminal observation signals to capture repeated terminal-driven file workflows.
- Added a clipboard adapter with privacy-aware capture modes and deterministic normalization.
- Added a browser downloads adapter to bring download activity into the workflow pipeline.
- Continued to treat these adapters as local signal sources that feed the same deterministic analysis and approval flow.

## Documentation

- Added system and architecture documentation for the public open-core engine and the private intelligence boundary.
- Added example workflows that show realistic repeated file workflows `flowd` can detect and automate safely.
- Added deterministic demo scenario fixtures and manifest documentation to keep demos and docs aligned with tests.
- Continued to keep contributor-facing and user-facing documentation in English.

## Notes For Early Adopters

- `flowd` remains an MVP-stage project focused on safe, inspectable workflow automation rather than broad autonomous computer control.
- The current product scope stays centered on local workflow observation, CLI suggestions, and deterministic file-oriented automations.
- Private intelligence remains optional. The open-core path continues to work on its own.
- Source installation remains the primary path for this pre-release.
