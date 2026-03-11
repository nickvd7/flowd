pub mod migrations;
pub mod repo;

use anyhow::{Context, Result};
use rusqlite::Connection;
use std::path::Path;

pub fn open_database(path: impl AsRef<Path>) -> Result<Connection> {
    let path = path.as_ref();
    let conn = Connection::open(path)
        .with_context(|| format!("failed to open database at {}", path.display()))?;
    migrations::run_migrations(&conn).context("failed to run database migrations")?;
    Ok(conn)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn opens_sqlite_database_and_runs_migrations() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("flowd.db");

        let conn = open_database(&db_path).unwrap();
        let table_exists: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'raw_events'",
                [],
                |row| row.get(0),
            )
            .unwrap();

        assert_eq!(table_exists, 1);
    }
}
