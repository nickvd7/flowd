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
            confidence REAL NOT NULL DEFAULT 0.0
        );

        CREATE TABLE IF NOT EXISTS suggestions (
            id INTEGER PRIMARY KEY,
            pattern_id INTEGER NOT NULL,
            status TEXT NOT NULL,
            proposal_json TEXT NOT NULL,
            created_at TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS automations (
            id INTEGER PRIMARY KEY,
            spec_yaml TEXT NOT NULL,
            state TEXT NOT NULL,
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
    conn.execute(
        "CREATE UNIQUE INDEX IF NOT EXISTS idx_normalized_events_raw_event_id ON normalized_events(raw_event_id) WHERE raw_event_id IS NOT NULL",
        [],
    )?;

    Ok(())
}

fn ensure_normalized_events_raw_event_id_column(conn: &Connection) -> rusqlite::Result<()> {
    let mut statement = conn.prepare("PRAGMA table_info(normalized_events)")?;
    let columns = statement.query_map([], |row| row.get::<_, String>(1))?;

    for column in columns {
        if column? == "raw_event_id" {
            return Ok(());
        }
    }

    conn.execute(
        "ALTER TABLE normalized_events ADD COLUMN raw_event_id INTEGER",
        [],
    )?;
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
