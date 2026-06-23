pub mod index;
pub mod migrations;
pub mod peers;
pub mod profiles;
pub mod quick_sends;
pub mod runs;

use rusqlite::{Connection, OpenFlags};
use std::path::Path;
use std::sync::{Arc, Mutex};
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
    conn: Arc<Mutex<Connection>>,
}

impl Clone for Db {
    fn clone(&self) -> Self {
        Self {
            conn: Arc::clone(&self.conn),
        }
    }
}

impl Db {
    /// Open or create a database at the given path with WAL mode enabled
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self> {
        let mut conn = Connection::open_with_flags(
            path,
            OpenFlags::SQLITE_OPEN_READ_WRITE
                | OpenFlags::SQLITE_OPEN_CREATE
                | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )?;

        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        conn.pragma_update(None, "foreign_keys", "ON")?;

        // Run migrations before wrapping in Arc<Mutex>
        let migrations = migrations::get_migrations();
        migrations
            .to_latest(&mut conn)
            .map_err(|e| StoreError::Migration(e.to_string()))?;

        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    /// Open an in-memory database (for tests)
    pub fn in_memory() -> Result<Self> {
        let mut conn = Connection::open_in_memory()?;
        conn.pragma_update(None, "foreign_keys", "ON")?;

        // Run migrations before wrapping in Arc<Mutex>
        let migrations = migrations::get_migrations();
        migrations
            .to_latest(&mut conn)
            .map_err(|e| StoreError::Migration(e.to_string()))?;

        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    /// Get current schema version
    pub fn schema_version(&self) -> Result<String> {
        let conn = self.conn.lock().unwrap();
        let version: String = conn.query_row(
            "SELECT value FROM meta WHERE key = 'schema_version'",
            [],
            |row| row.get(0),
        )?;
        Ok(version)
    }

    /// Get a locked reference to the connection for use in impl methods
    pub(crate) fn conn(&self) -> std::sync::MutexGuard<'_, Connection> {
        self.conn.lock().unwrap()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn in_memory_db_initializes() {
        let db = Db::in_memory().expect("failed to create in-memory db");
        let version = db.schema_version().expect("failed to get schema version");
        assert_eq!(version, "7");
    }
}
