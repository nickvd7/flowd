use flow_analysis::catch_up_analysis;
use flow_core::events::RawEvent;
use flow_db::{
    migrations::run_migrations,
    repo::{
        insert_raw_event, list_normalized_events, list_patterns,
        list_pending_observation_raw_events, list_suggestions,
    },
};
use rusqlite::Connection;
use serde::Deserialize;
use std::{collections::BTreeSet, fs, path::PathBuf};

#[derive(Debug, Deserialize)]
struct ScenarioManifest {
    scenarios: Vec<ScenarioSpec>,
}

#[derive(Debug, Deserialize)]
struct ScenarioSpec {
    id: String,
    title: String,
    file: String,
    expected_sources: Vec<String>,
    expected_normalized_events: usize,
    expected_pattern_signature: String,
    expected_proposal_contains: String,
}

#[test]
fn demo_scenarios_replay_into_deterministic_suggestions() {
    let manifest = load_manifest();

    for scenario in manifest.scenarios {
        let mut conn = Connection::open_in_memory().unwrap();
        run_migrations(&conn).unwrap();

        let events = load_events(&scenario.file);
        let observed_sources: BTreeSet<_> = events
            .iter()
            .map(|event| format!("{:?}", event.source))
            .collect();
        let expected_sources: BTreeSet<_> = scenario.expected_sources.iter().cloned().collect();
        assert_eq!(
            observed_sources, expected_sources,
            "scenario {} should only use the declared sources",
            scenario.id
        );

        for event in &events {
            insert_raw_event(&conn, event).unwrap();
        }

        catch_up_analysis(&mut conn, 300).unwrap();

        let pending = list_pending_observation_raw_events(&conn).unwrap();
        assert!(
            pending.is_empty(),
            "scenario {} left pending raw events after replay",
            scenario.id
        );

        let normalized_events = list_normalized_events(&conn).unwrap();
        assert_eq!(
            normalized_events.len(),
            scenario.expected_normalized_events,
            "scenario {} normalized event count changed",
            scenario.id
        );

        let patterns = list_patterns(&conn).unwrap();
        assert_eq!(
            patterns.len(),
            1,
            "scenario {} should produce exactly one repeated pattern",
            scenario.id
        );
        assert_eq!(
            patterns[0].signature, scenario.expected_pattern_signature,
            "scenario {} produced an unexpected pattern signature",
            scenario.id
        );

        let suggestions = list_suggestions(&conn).unwrap();
        assert_eq!(
            suggestions.len(),
            1,
            "scenario {} should produce exactly one suggestion",
            scenario.id
        );
        assert!(
            suggestions[0]
                .proposal_text
                .contains(&scenario.expected_proposal_contains),
            "scenario {} suggestion text drifted for {}",
            scenario.id,
            scenario.title
        );
    }
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
