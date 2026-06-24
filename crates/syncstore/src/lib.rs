pub mod index;
pub mod migrations;
pub mod peers;
pub mod profiles;
pub mod quick_sends;
pub mod runs;

use rusqlite::{Connection, OpenFlags};
use std::path::Path;
use std::sync::Arc;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum StoreError {
    #[error("SQLite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("Migration error: {0}")]
    Migration(String),
}

pub type Result<T> = std::result::Result<T, StoreError>;

/// A wrapper around rusqlite::Connection that is Send + Sync.
/// This is safe because we open the connection with SQLITE_OPEN_FULLMUTEX,
/// which enables SQLite's internal thread-safe serialization.
struct SendableConnection(Connection);

// SAFETY: rusqlite::Connection with FULLMUTEX is thread-safe.
// SQLite serializes all access internally when opened with FULLMUTEX.
unsafe impl Send for SendableConnection {}
unsafe impl Sync for SendableConnection {}

impl std::ops::Deref for SendableConnection {
    type Target = Connection;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

pub struct Db {
    conn: Arc<SendableConnection>,
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
                | OpenFlags::SQLITE_OPEN_FULL_MUTEX,
        )?;

        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        conn.pragma_update(None, "foreign_keys", "ON")?;

        // Run migrations before wrapping in Arc
        let migrations = migrations::get_migrations();
        migrations
            .to_latest(&mut conn)
            .map_err(|e| StoreError::Migration(e.to_string()))?;

        Ok(Self {
            conn: Arc::new(SendableConnection(conn)),
        })
    }

    /// Open an in-memory database (for tests)
    pub fn in_memory() -> Result<Self> {
        let mut conn = Connection::open_in_memory()?;
        conn.pragma_update(None, "foreign_keys", "ON")?;

        // Run migrations before wrapping in Arc
        let migrations = migrations::get_migrations();
        migrations
            .to_latest(&mut conn)
            .map_err(|e| StoreError::Migration(e.to_string()))?;

        Ok(Self {
            conn: Arc::new(SendableConnection(conn)),
        })
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

    /// Get a reference to the connection for use in impl methods
    pub(crate) fn conn(&self) -> &Connection {
        &self.conn
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
