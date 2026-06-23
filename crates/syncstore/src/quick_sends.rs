use rusqlite::params;
use uuid::Uuid;

use crate::{Db, Result};

#[derive(Debug, Clone)]
pub struct QuickSendRecordRow {
    pub id: Uuid,
    pub peer_id: Uuid,
    pub direction: String,
    pub destination_dir: String,
    pub started_at: String,
    pub finished_at: String,
    pub status: String,
    pub files_transferred: u32,
    pub bytes_transferred: u64,
    pub error_summary: Option<String>,
}

impl Db {
    pub fn insert_quick_send(&self, record: &QuickSendRecordRow) -> Result<()> {
        let conn = self.conn();
        conn.execute(
            "INSERT INTO quick_send_records (id, peer_id, direction, destination_dir,
             started_at, finished_at, status, files_transferred, bytes_transferred, error_summary)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                record.id.to_string(),
                record.peer_id.to_string(),
                record.direction,
                record.destination_dir,
                record.started_at,
                record.finished_at,
                record.status,
                record.files_transferred,
                record.bytes_transferred,
                record.error_summary,
            ],
        )?;
        Ok(())
    }

    pub fn get_quick_sends(&self, limit: u32) -> Result<Vec<QuickSendRecordRow>> {
        let conn = self.conn();
        let mut stmt = conn.prepare(
            "SELECT id, peer_id, direction, destination_dir, started_at, finished_at,
                    status, files_transferred, bytes_transferred, error_summary
             FROM quick_send_records
             ORDER BY started_at DESC LIMIT ?1",
        )?;

        let rows = stmt.query_map(params![limit], |row| {
            Ok(QuickSendRecordRow {
                id: Uuid::parse_str(&row.get::<_, String>(0)?).unwrap_or_default(),
                peer_id: Uuid::parse_str(&row.get::<_, String>(1)?).unwrap_or_default(),
                direction: row.get(2)?,
                destination_dir: row.get(3)?,
                started_at: row.get(4)?,
                finished_at: row.get(5)?,
                status: row.get(6)?,
                files_transferred: row.get(7)?,
                bytes_transferred: row.get(8)?,
                error_summary: row.get(9)?,
            })
        })?;

        let mut records = Vec::new();
        for row in rows {
            records.push(row?);
        }
        Ok(records)
    }

    pub fn get_quick_sends_for_peer(
        &self,
        peer_id: Uuid,
        limit: u32,
    ) -> Result<Vec<QuickSendRecordRow>> {
        let conn = self.conn();
        let mut stmt = conn.prepare(
            "SELECT id, peer_id, direction, destination_dir, started_at, finished_at,
                    status, files_transferred, bytes_transferred, error_summary
             FROM quick_send_records WHERE peer_id = ?1
             ORDER BY started_at DESC LIMIT ?2",
        )?;

        let rows = stmt.query_map(params![peer_id.to_string(), limit], |row| {
            Ok(QuickSendRecordRow {
                id: Uuid::parse_str(&row.get::<_, String>(0)?).unwrap_or_default(),
                peer_id: Uuid::parse_str(&row.get::<_, String>(1)?).unwrap_or_default(),
                direction: row.get(2)?,
                destination_dir: row.get(3)?,
                started_at: row.get(4)?,
                finished_at: row.get(5)?,
                status: row.get(6)?,
                files_transferred: row.get(7)?,
                bytes_transferred: row.get(8)?,
                error_summary: row.get(9)?,
            })
        })?;

        let mut records = Vec::new();
        for row in rows {
            records.push(row?);
        }
        Ok(records)
    }
}
