use rusqlite::params;
use uuid::Uuid;

use crate::{Db, Result};

#[derive(Debug, Clone)]
pub struct ProfileRow {
    pub id: Uuid,
    pub name: String,
    pub mode: String,
    pub delete_propagation: bool,
    pub conflict_policy: String,
    pub peer_name: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone)]
pub struct AnchorRow {
    pub id: i64,
    pub profile_id: Uuid,
    pub local_path: String,
    pub remote_path: String,
    pub max_depth: i32,
    pub include_hidden: bool,
    pub ignore_patterns: Vec<String>,
}

impl Db {
    pub fn insert_profile(&self, profile: &ProfileRow) -> Result<()> {
        self.conn.execute(
            "INSERT INTO profiles (id, name, mode, delete_propagation, conflict_policy, peer_name)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                profile.id.to_string(),
                profile.name,
                profile.mode,
                i32::from(profile.delete_propagation),
                profile.conflict_policy,
                profile.peer_name,
            ],
        )?;
        Ok(())
    }

    pub fn get_profile(&self, id: Uuid) -> Result<Option<ProfileRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, mode, delete_propagation, conflict_policy, peer_name, created_at, updated_at
             FROM profiles WHERE id = ?1",
        )?;

        let row = stmt.query_row(params![id.to_string()], |row| {
            Ok(ProfileRow {
                id: Uuid::parse_str(&row.get::<_, String>(0)?).unwrap_or_default(),
                name: row.get(1)?,
                mode: row.get(2)?,
                delete_propagation: row.get::<_, i32>(3)? != 0,
                conflict_policy: row.get(4)?,
                peer_name: row.get(5)?,
                created_at: row.get(6)?,
                updated_at: row.get(7)?,
            })
        });

        match row {
            Ok(p) => Ok(Some(p)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    pub fn list_profiles(&self) -> Result<Vec<ProfileRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, mode, delete_propagation, conflict_policy, peer_name, created_at, updated_at
             FROM profiles ORDER BY name",
        )?;

        let rows = stmt.query_map([], |row| {
            Ok(ProfileRow {
                id: Uuid::parse_str(&row.get::<_, String>(0)?).unwrap_or_default(),
                name: row.get(1)?,
                mode: row.get(2)?,
                delete_propagation: row.get::<_, i32>(3)? != 0,
                conflict_policy: row.get(4)?,
                peer_name: row.get(5)?,
                created_at: row.get(6)?,
                updated_at: row.get(7)?,
            })
        })?;

        let mut profiles = Vec::new();
        for row in rows {
            profiles.push(row?);
        }
        Ok(profiles)
    }

    pub fn update_profile(&self, profile: &ProfileRow) -> Result<()> {
        self.conn.execute(
            "UPDATE profiles SET name = ?2, mode = ?3, delete_propagation = ?4,
             conflict_policy = ?5, peer_name = ?6,
             updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
             WHERE id = ?1",
            params![
                profile.id.to_string(),
                profile.name,
                profile.mode,
                i32::from(profile.delete_propagation),
                profile.conflict_policy,
                profile.peer_name,
            ],
        )?;
        Ok(())
    }

    pub fn delete_profile(&self, id: Uuid) -> Result<()> {
        self.conn.execute(
            "DELETE FROM profiles WHERE id = ?1",
            params![id.to_string()],
        )?;
        Ok(())
    }

    pub fn insert_anchor(&self, anchor: &AnchorRow) -> Result<()> {
        let patterns_json = serde_json::to_string(&anchor.ignore_patterns).unwrap_or_default();
        self.conn.execute(
            "INSERT INTO anchors (profile_id, local_path, remote_path, max_depth, include_hidden, ignore_patterns)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                anchor.profile_id.to_string(),
                anchor.local_path,
                anchor.remote_path,
                anchor.max_depth,
                i32::from(anchor.include_hidden),
                patterns_json,
            ],
        )?;
        Ok(())
    }

    pub fn get_anchors(&self, profile_id: Uuid) -> Result<Vec<AnchorRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, profile_id, local_path, remote_path, max_depth, include_hidden, ignore_patterns
             FROM anchors WHERE profile_id = ?1 ORDER BY id",
        )?;

        let rows = stmt.query_map(params![profile_id.to_string()], |row| {
            let patterns_str: String = row.get(6)?;
            let patterns: Vec<String> = serde_json::from_str(&patterns_str).unwrap_or_default();
            Ok(AnchorRow {
                id: row.get(0)?,
                profile_id: Uuid::parse_str(&row.get::<_, String>(1)?).unwrap_or_default(),
                local_path: row.get(2)?,
                remote_path: row.get(3)?,
                max_depth: row.get(4)?,
                include_hidden: row.get::<_, i32>(5)? != 0,
                ignore_patterns: patterns,
            })
        })?;

        let mut anchors = Vec::new();
        for row in rows {
            anchors.push(row?);
        }
        Ok(anchors)
    }
}
