pub mod index;
pub mod migrations;
pub mod peers;
pub mod profiles;
pub mod quick_sends;
pub mod runs;

use rusqlite::{Connection, OpenFlags};
use std::path::Path;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum StoreError {
    #[error("SQLite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("Migration error: {0}")]
    Migration(String),
}

pub type Result<T> = std::result::Result<T, StoreError>;

pub struct Db {
    conn: Connection,
}

impl Db {
    /// Open or create a database at the given path with WAL mode enabled
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self> {
        let conn = Connection::open_with_flags(
            path,
            OpenFlags::SQLITE_OPEN_READ_WRITE
                | OpenFlags::SQLITE_OPEN_CREATE
                | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )?;

        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        conn.pragma_update(None, "foreign_keys", "ON")?;

        let mut db = Self { conn };
        db.run_migrations()?;
        Ok(db)
    }

    /// Open an in-memory database (for tests)
    pub fn in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        let mut db = Self { conn };
        db.run_migrations()?;
        Ok(db)
    }

    fn run_migrations(&mut self) -> Result<()> {
        let migrations = migrations::get_migrations();
        migrations
            .to_latest(&mut self.conn)
            .map_err(|e| StoreError::Migration(e.to_string()))?;
        Ok(())
    }

    /// Get current schema version
    pub fn schema_version(&self) -> Result<String> {
        let version: String = self.conn.query_row(
            "SELECT value FROM meta WHERE key = 'schema_version'",
            [],
            |row| row.get(0),
        )?;
        Ok(version)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn in_memory_db_initializes() {
        let db = Db::in_memory().expect("failed to create in-memory db");
        let version = db.schema_version().expect("failed to get schema version");
        assert_eq!(version, "6");
    }
}
