use rusqlite::Connection;

pub fn run_migrations(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS raw_events (
            id INTEGER PRIMARY KEY,
            ts TEXT NOT NULL,
            source TEXT NOT NULL,
            payload_json TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS normalized_events (
            id INTEGER PRIMARY KEY,
            ts TEXT NOT NULL,
            action_type TEXT NOT NULL,
            app TEXT,
            target TEXT,
            metadata_json TEXT NOT NULL,
            raw_event_id INTEGER
        );

        CREATE TABLE IF NOT EXISTS sessions (
            id INTEGER PRIMARY KEY,
            start_ts TEXT NOT NULL,
            end_ts TEXT NOT NULL,
            session_type TEXT
        );

        CREATE TABLE IF NOT EXISTS session_events (
            session_id INTEGER NOT NULL,
            event_id INTEGER NOT NULL,
            ord INTEGER NOT NULL
        );

        CREATE TABLE IF NOT EXISTS patterns (
            id INTEGER PRIMARY KEY,
            signature TEXT NOT NULL,
            count INTEGER NOT NULL,
            avg_duration_ms INTEGER NOT NULL,
            canonical_summary TEXT,
            confidence REAL NOT NULL DEFAULT 0.0,
            last_seen_at TEXT,
            safety_score REAL NOT NULL DEFAULT 0.0,
            is_active INTEGER NOT NULL DEFAULT 1
        );

        CREATE TABLE IF NOT EXISTS suggestions (
            id INTEGER PRIMARY KEY,
            pattern_id INTEGER NOT NULL,
            status TEXT NOT NULL,
            proposal_json TEXT NOT NULL,
            created_at TEXT NOT NULL,
            usefulness_score REAL NOT NULL DEFAULT 0.0,
            freshness TEXT NOT NULL DEFAULT 'current'
        );

        CREATE TABLE IF NOT EXISTS automations (
            id INTEGER PRIMARY KEY,
            suggestion_id INTEGER,
            spec_yaml TEXT NOT NULL,
            state TEXT NOT NULL DEFAULT 'active',
            summary TEXT,
            accepted_at TEXT
        );

        CREATE TABLE IF NOT EXISTS automation_runs (
            id INTEGER PRIMARY KEY,
            automation_id INTEGER NOT NULL,
            started_at TEXT NOT NULL,
            finished_at TEXT,
            result TEXT NOT NULL,
            undo_payload_json TEXT
        );
        "#,
    )?;

    ensure_normalized_events_raw_event_id_column(conn)?;
    ensure_automations_suggestion_id_column(conn)?;
    ensure_automations_state_column(conn)?;
    ensure_automations_summary_column(conn)?;
    ensure_patterns_last_seen_at_column(conn)?;
    ensure_patterns_safety_score_column(conn)?;
    ensure_patterns_is_active_column(conn)?;
    ensure_suggestions_usefulness_score_column(conn)?;
    ensure_suggestions_freshness_column(conn)?;
    normalize_automation_states(conn)?;
    conn.execute(
        "CREATE UNIQUE INDEX IF NOT EXISTS idx_patterns_signature ON patterns(signature)",
        [],
    )?;
    conn.execute(
        "CREATE UNIQUE INDEX IF NOT EXISTS idx_normalized_events_raw_event_id ON normalized_events(raw_event_id) WHERE raw_event_id IS NOT NULL",
        [],
    )?;
    conn.execute(
        "CREATE UNIQUE INDEX IF NOT EXISTS idx_suggestions_active_pending_pattern ON suggestions(pattern_id) WHERE status = 'pending' AND freshness = 'current'",
        [],
    )?;

    Ok(())
}

fn ensure_normalized_events_raw_event_id_column(conn: &Connection) -> rusqlite::Result<()> {
    ensure_column_exists(
        conn,
        "normalized_events",
        "raw_event_id",
        "ALTER TABLE normalized_events ADD COLUMN raw_event_id INTEGER",
    )
}

fn ensure_automations_suggestion_id_column(conn: &Connection) -> rusqlite::Result<()> {
    ensure_column_exists(
        conn,
        "automations",
        "suggestion_id",
        "ALTER TABLE automations ADD COLUMN suggestion_id INTEGER",
    )
}

fn ensure_automations_summary_column(conn: &Connection) -> rusqlite::Result<()> {
    ensure_column_exists(
        conn,
        "automations",
        "summary",
        "ALTER TABLE automations ADD COLUMN summary TEXT",
    )
}

fn ensure_automations_state_column(conn: &Connection) -> rusqlite::Result<()> {
    ensure_column_exists(
        conn,
        "automations",
        "state",
        "ALTER TABLE automations ADD COLUMN state TEXT NOT NULL DEFAULT 'active'",
    )
}

fn ensure_patterns_last_seen_at_column(conn: &Connection) -> rusqlite::Result<()> {
    ensure_column_exists(
        conn,
        "patterns",
        "last_seen_at",
        "ALTER TABLE patterns ADD COLUMN last_seen_at TEXT",
    )
}

fn ensure_patterns_safety_score_column(conn: &Connection) -> rusqlite::Result<()> {
    ensure_column_exists(
        conn,
        "patterns",
        "safety_score",
        "ALTER TABLE patterns ADD COLUMN safety_score REAL NOT NULL DEFAULT 0.0",
    )
}

fn ensure_patterns_is_active_column(conn: &Connection) -> rusqlite::Result<()> {
    ensure_column_exists(
        conn,
        "patterns",
        "is_active",
        "ALTER TABLE patterns ADD COLUMN is_active INTEGER NOT NULL DEFAULT 1",
    )
}

fn ensure_suggestions_usefulness_score_column(conn: &Connection) -> rusqlite::Result<()> {
    ensure_column_exists(
        conn,
        "suggestions",
        "usefulness_score",
        "ALTER TABLE suggestions ADD COLUMN usefulness_score REAL NOT NULL DEFAULT 0.0",
    )
}

fn ensure_suggestions_freshness_column(conn: &Connection) -> rusqlite::Result<()> {
    ensure_column_exists(
        conn,
        "suggestions",
        "freshness",
        "ALTER TABLE suggestions ADD COLUMN freshness TEXT NOT NULL DEFAULT 'current'",
    )
}

fn normalize_automation_states(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE automations SET state = 'active' WHERE state IS NULL OR TRIM(state) = '' OR state = 'approved'",
        [],
    )?;
    Ok(())
}

fn ensure_column_exists(
    conn: &Connection,
    table: &str,
    column_name: &str,
    alter_sql: &str,
) -> rusqlite::Result<()> {
    let mut statement = conn.prepare(&format!("PRAGMA table_info({table})"))?;
    let columns = statement.query_map([], |row| row.get::<_, String>(1))?;

    for column in columns {
        if column? == column_name {
            return Ok(());
        }
    }

    conn.execute(alter_sql, [])?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrations_run() {
        let conn = Connection::open_in_memory().unwrap();
        run_migrations(&conn).unwrap();
    }
}
