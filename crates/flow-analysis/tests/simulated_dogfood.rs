//! Simulated dogfooding harness for the 1.0 quality bar.
//!
//! Real multi-week dogfood still needs a human on a real machine. This suite
//! replays the canonical demo scenarios as a deterministic stand-in:
//! - low-noise suggestions (one pattern / one suggestion per scenario)
//! - multi-scenario isolation (no cross-contamination)
//! - approve → dry-run → run → undo on rewritten filesystem paths
//! - execution allowlist enforcement during the live cycle

use flow_analysis::catch_up_analysis;
use flow_core::events::RawEvent;
use flow_core::PathAllowlist;
use flow_db::{
    migrations::run_migrations,
    repo::{
        insert_raw_event, list_patterns, list_pending_observation_raw_events, list_suggestions,
    },
};
use flow_exec::{
    approve_suggestion, dry_run_automation, execute_automation, list_runs, undo_automation_run,
};
use rusqlite::Connection;
use serde::Deserialize;
use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
};
use tempfile::tempdir;

#[derive(Debug, Deserialize)]
struct ScenarioManifest {
    scenarios: Vec<ScenarioSpec>,
}

#[derive(Debug, Deserialize, Clone)]
struct ScenarioSpec {
    #[allow(dead_code)]
    id: String,
    title: String,
    file: String,
    expected_proposal_contains: String,
    expected_pattern_signature: String,
}

#[test]
fn simulated_dogfood_quality_bar_across_demo_scenarios() {
    let manifest = load_manifest();
    assert!(
        !manifest.scenarios.is_empty(),
        "demo scenario manifest must not be empty"
    );

    let mut conn = Connection::open_in_memory().unwrap();
    run_migrations(&conn).unwrap();

    let mut expected_signatures = BTreeSet::new();
    for scenario in &manifest.scenarios {
        expected_signatures.insert(scenario.expected_pattern_signature.clone());
        for event in load_events(&scenario.file) {
            insert_raw_event(&conn, &event).unwrap();
        }
    }

    catch_up_analysis(&mut conn, 300).unwrap();

    assert!(
        list_pending_observation_raw_events(&conn)
            .unwrap()
            .is_empty(),
        "dogfood replay left pending raw events"
    );

    let patterns = list_patterns(&conn).unwrap();
    let signatures: BTreeSet<_> = patterns
        .iter()
        .map(|pattern| pattern.signature.clone())
        .collect();
    assert_eq!(
        signatures, expected_signatures,
        "combined demo scenarios should not merge or invent patterns"
    );
    assert_eq!(
        patterns.len(),
        manifest.scenarios.len(),
        "each demo scenario should yield exactly one pattern"
    );

    let suggestions = list_suggestions(&conn).unwrap();
    assert_eq!(
        suggestions.len(),
        manifest.scenarios.len(),
        "dogfood replay should stay at one suggestion per scenario (anti-noise)"
    );

    for scenario in &manifest.scenarios {
        assert!(
            suggestions.iter().any(|suggestion| {
                suggestion
                    .proposal_text
                    .contains(&scenario.expected_proposal_contains)
            }),
            "missing recognizable suggestion for {}",
            scenario.title
        );
    }

    // Approving every pending suggestion must succeed and stay inspectable.
    let mut automation_ids = Vec::new();
    for suggestion in &suggestions {
        let automation_id = approve_suggestion(&mut conn, suggestion.suggestion_id).unwrap();
        automation_ids.push(automation_id);
    }
    assert_eq!(automation_ids.len(), manifest.scenarios.len());
}

#[test]
fn simulated_dogfood_invoice_lifecycle_with_allowlist() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    let downloads = root.join("Downloads");
    let invoices = root.join("Documents/Accounting/Invoices");
    fs::create_dir_all(&downloads).unwrap();
    fs::create_dir_all(&invoices).unwrap();

    // Seed a fresh matching file that the approved automation should organize.
    fs::write(downloads.join("invoice-2001.pdf"), "invoice-2001").unwrap();

    let mut conn = Connection::open_in_memory().unwrap();
    run_migrations(&conn).unwrap();

    let events = rewrite_demo_root(&load_events("invoice_organization.raw_events.json"), root);
    for event in &events {
        insert_raw_event(&conn, event).unwrap();
    }
    catch_up_analysis(&mut conn, 300).unwrap();

    let suggestions = list_suggestions(&conn).unwrap();
    assert_eq!(suggestions.len(), 1);
    assert!(suggestions[0]
        .proposal_text
        .contains("Repeated invoice file workflow detected"));

    let automation_id = approve_suggestion(&mut conn, suggestions[0].suggestion_id).unwrap();
    let allowlist = PathAllowlist::from_roots([root.to_path_buf()]);

    let dry = dry_run_automation(&conn, automation_id, &allowlist).unwrap();
    assert!(
        !dry.preview.is_empty(),
        "dry-run should describe predicted work"
    );

    let outcome = execute_automation(&conn, automation_id, &allowlist).unwrap();
    assert!(
        !outcome.report.operations.is_empty(),
        "run should apply at least one filesystem operation"
    );
    assert!(outcome.run_id.is_some(), "completed run should be auditable");
    assert!(
        !downloads.join("invoice-2001.pdf").exists(),
        "source invoice should be consumed"
    );
    assert!(
        find_reviewed_invoice(root).is_some(),
        "reviewed invoice should land under the allowlisted tree"
    );

    let runs = list_runs(&conn).unwrap();
    let completed = runs
        .iter()
        .find(|run| run.automation_id == automation_id && run.result == "completed")
        .expect("completed run record");
    let undo = undo_automation_run(&conn, completed.run_id, &allowlist).unwrap();
    assert!(
        !undo.report.operations.is_empty(),
        "undo should reverse the completed run"
    );
    assert!(
        downloads.join("invoice-2001.pdf").exists(),
        "undo should restore the original invoice path"
    );

    // Destinations outside a tight Downloads-only allowlist must fail closed.
    let tight = PathAllowlist::from_roots([downloads]);
    let blocked = dry_run_automation(&conn, automation_id, &tight);
    assert!(
        blocked.is_err(),
        "destination outside the tight allowlist must fail closed"
    );
}

fn find_reviewed_invoice(root: &Path) -> Option<PathBuf> {
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.contains("invoice-2001") && name.ends_with(".pdf"))
            {
                return Some(path);
            }
        }
    }
    None
}

fn rewrite_demo_root(events: &[RawEvent], root: &Path) -> Vec<RawEvent> {
    let raw = serde_json::to_string(events).unwrap();
    let rewritten = raw.replace("/demo", &root.display().to_string());
    serde_json::from_str(&rewritten).unwrap()
}

fn load_manifest() -> ScenarioManifest {
    let content = fs::read_to_string(fixtures_dir().join("manifest.json")).unwrap();
    serde_json::from_str(&content).unwrap()
}

fn load_events(file_name: &str) -> Vec<RawEvent> {
    let content = fs::read_to_string(fixtures_dir().join(file_name)).unwrap();
    serde_json::from_str(&content).unwrap()
}

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/demo_scenarios")
}
