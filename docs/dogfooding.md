# Dogfooding And Real-World Evaluation

`flowd` is ready for real usage, not just demos. The next step is to run it in normal workflows for 1 to 2 weeks and capture where the MVP feels useful, noisy, confusing, or unsafe.

This guide keeps the process lightweight:

- use `flowd` during real work
- check suggestions regularly
- record friction while it is fresh
- look for repeatable product gaps, not perfect metrics

For realistic workflow ideas, see [Example Workflows](./example-workflows.md). For current MVP scope, see [v0.2.0-pre release notes](./release-notes-v0.2.md).

## Goal

By the end of dogfooding, the maintainer should be able to answer:

- Which real workflows does `flowd` detect well?
- Which suggestions are useful enough to approve?
- Where does trust break down?
- What setup or inspection steps still feel too heavy?

## Recommended 1-2 Week Plan

### Days 1-2: Setup And Baseline

- Run `flowctl setup` and start `flow-daemon` in the folders you actually use.
- Prefer 1 to 3 high-frequency folders such as `~/Downloads`, `~/Desktop`, or a document inbox.
- Keep the first setup simple. Do not tune everything up front.
- Confirm you can inspect the core surfaces:

```bash
flowctl suggestions
flowctl suggestions --explain
flowctl stats
flowctl automations
```

Focus on whether onboarding and config feel obvious enough for a contributor or early adopter.

### Days 3-7: Daily Use

Use `flowd` in normal work instead of forcing synthetic scenarios.

Good workflows to observe:

- renaming and filing downloaded PDFs
- sorting files from `~/Downloads`
- cleaning up screenshots
- moving reviewed files from an inbox to archive folders
- terminal-driven rename and move workflows
- repeated export cleanup from another tool

During daily use, pay attention to:

- whether repeated workflows are detected at all
- whether suggestions show up soon enough to matter
- whether the suggestion wording matches the actual pattern
- whether explainability helps you understand why something appeared
- whether approval feels safe enough to use
- whether undo feels like real protection

Check suggestions once or twice per day. Record quick notes immediately after anything notably good or bad.

### Days 8-14: Approval And Trust

Approve only suggestions that feel clearly correct and low risk.

For each approved automation, check:

- whether the dry run matches expectations
- whether the live run feels predictable
- whether you would trust it without rereading everything each time
- whether undo is easy to find and use if needed

At this stage, the main question is not "can automation run?" It is "does this feel safe enough to become part of normal work?"

## What Good Looks Like

The MVP is working well when most of the following are true:

- repeated file workflows surface without special setup
- suggestions are recognizable from real behavior, not vague guesses
- explanation output makes ranking or matching easier to trust
- approval decisions feel informed, not blind
- approved automations save time on future repetitions
- onboarding gets someone to first value without reading large docs
- local config changes are occasional, not constant

## What To Record

Keep notes small and concrete. Good observations usually describe one workflow, one suggestion, and one reason it helped or failed.

Capture:

- false positives: suggestions that should not have appeared
- missed opportunities: repeated workflows that never became suggestions
- suggestion clarity: wording, scope, and whether the suggestion was understandable
- trust in automations: whether approval felt safe and justified
- usefulness of explainability: whether the explanation changed the decision
- onboarding friction: setup, first-run, and command discoverability problems
- config friction: watch folders, adapter settings, or validation issues that were harder than expected

Also note:

- how many times the workflow had to repeat before `flowd` became useful
- whether a suggestion came too early, too late, or at the right time
- whether the wrong source folder, destination, or rename pattern was inferred

## Simple Logging Template

Use this as a daily note, issue comment, or scratch file.

```md
Date:
Workflow:
Observed in:

What happened:
- 

Suggestion quality:
- Good / Mixed / Bad

What felt useful:
- 

What felt wrong:
- 

Approval felt safe:
- Yes / No / Not applicable

Explainability useful:
- Yes / No

Category:
- false positive
- missed opportunity
- suggestion clarity
- automation trust
- explainability
- onboarding friction
- config friction

Follow-up:
- 
```

## Lightweight Review At The End

At the end of the 1 to 2 weeks, summarize findings in a short list:

- top 3 workflows where `flowd` clearly helped
- top 3 reasons suggestions were ignored, rejected, or snoozed
- any automation that felt safe enough for repeated use
- any area where explainability improved trust
- the biggest onboarding or config issue to fix next

If the notes are messy, that is fine. The goal is to identify the next product improvements with evidence from normal usage.
