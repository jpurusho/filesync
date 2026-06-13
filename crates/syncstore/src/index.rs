use rusqlite::params;
use uuid::Uuid;

use crate::{Db, Result};

#[derive(Debug, Clone)]
pub struct IndexEntryRow {
    pub profile_id: Uuid,
    pub anchor_idx: usize,
    pub rel_path: String,
    pub kind: String,
    pub size: u64,
    pub mtime_secs: i64,
    pub hash: String,
}

impl Db {
    pub fn load_index(&self, profile_id: Uuid, anchor_idx: usize) -> Result<Vec<IndexEntryRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT profile_id, anchor_idx, rel_path, kind, size, mtime_secs, hash
             FROM sync_index WHERE profile_id = ?1 AND anchor_idx = ?2",
        )?;

        let rows = stmt.query_map(params![profile_id.to_string(), anchor_idx as i64], |row| {
            Ok(IndexEntryRow {
                profile_id: Uuid::parse_str(&row.get::<_, String>(0)?).unwrap_or_default(),
                anchor_idx: row.get::<_, i64>(1)? as usize,
                rel_path: row.get(2)?,
                kind: row.get(3)?,
                size: row.get::<_, i64>(4)? as u64,
                mtime_secs: row.get(5)?,
                hash: row.get(6)?,
            })
        })?;

        let mut entries = Vec::new();
        for row in rows {
            entries.push(row?);
        }
        Ok(entries)
    }

    pub fn save_index(
        &self,
        profile_id: Uuid,
        anchor_idx: usize,
        entries: &[IndexEntryRow],
    ) -> Result<()> {
        // Clear old index for this anchor
        self.conn.execute(
            "DELETE FROM sync_index WHERE profile_id = ?1 AND anchor_idx = ?2",
            params![profile_id.to_string(), anchor_idx as i64],
        )?;

        let mut stmt = self.conn.prepare(
            "INSERT INTO sync_index (profile_id, anchor_idx, rel_path, kind, size, mtime_secs, hash)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        )?;

        for entry in entries {
            stmt.execute(params![
                profile_id.to_string(),
                anchor_idx as i64,
                entry.rel_path,
                entry.kind,
                entry.size as i64,
                entry.mtime_secs,
                entry.hash,
            ])?;
        }

        Ok(())
    }

    pub fn clear_index(&self, profile_id: Uuid) -> Result<()> {
        self.conn.execute(
            "DELETE FROM sync_index WHERE profile_id = ?1",
            params![profile_id.to_string()],
        )?;
        Ok(())
    }
}
