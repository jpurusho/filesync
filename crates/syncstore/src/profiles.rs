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
    pub version: u64,
    pub peer_id: String,
    pub origin_instance_id: String,
    pub pending_deletion: bool,
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
            "INSERT INTO profiles (id, name, mode, delete_propagation, conflict_policy, peer_name, version, peer_id, origin_instance_id, pending_deletion)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                profile.id.to_string(),
                profile.name,
                profile.mode,
                i32::from(profile.delete_propagation),
                profile.conflict_policy,
                profile.peer_name,
                profile.version as i64,
                profile.peer_id,
                profile.origin_instance_id,
                i32::from(profile.pending_deletion),
            ],
        )?;
        Ok(())
    }

    pub fn get_profile(&self, id: Uuid) -> Result<Option<ProfileRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, mode, delete_propagation, conflict_policy, peer_name, created_at, updated_at, version, peer_id, origin_instance_id, pending_deletion
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
                version: row.get::<_, i64>(8)? as u64,
                peer_id: row.get(9)?,
                origin_instance_id: row.get(10)?,
                pending_deletion: row.get::<_, i32>(11)? != 0,
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
            "SELECT id, name, mode, delete_propagation, conflict_policy, peer_name, created_at, updated_at, version, peer_id, origin_instance_id, pending_deletion
             FROM profiles WHERE pending_deletion = 0 ORDER BY name",
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
                version: row.get::<_, i64>(8)? as u64,
                peer_id: row.get(9)?,
                origin_instance_id: row.get(10)?,
                pending_deletion: row.get::<_, i32>(11)? != 0,
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
             conflict_policy = ?5, peer_name = ?6, version = ?7, peer_id = ?8,
             origin_instance_id = ?9, pending_deletion = ?10,
             updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
             WHERE id = ?1",
            params![
                profile.id.to_string(),
                profile.name,
                profile.mode,
                i32::from(profile.delete_propagation),
                profile.conflict_policy,
                profile.peer_name,
                profile.version as i64,
                profile.peer_id,
                profile.origin_instance_id,
                i32::from(profile.pending_deletion),
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

    /// List profiles that target a specific peer (for FR-PS-1).
    pub fn list_profiles_for_peer(&self, peer_id: Uuid) -> Result<Vec<ProfileRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, mode, delete_propagation, conflict_policy, peer_name, created_at, updated_at, version, peer_id, origin_instance_id, pending_deletion
             FROM profiles WHERE peer_id = ?1 AND pending_deletion = 0 ORDER BY name",
        )?;

        let rows = stmt.query_map(params![peer_id.to_string()], |row| {
            Ok(ProfileRow {
                id: Uuid::parse_str(&row.get::<_, String>(0)?).unwrap_or_default(),
                name: row.get(1)?,
                mode: row.get(2)?,
                delete_propagation: row.get::<_, i32>(3)? != 0,
                conflict_policy: row.get(4)?,
                peer_name: row.get(5)?,
                created_at: row.get(6)?,
                updated_at: row.get(7)?,
                version: row.get::<_, i64>(8)? as u64,
                peer_id: row.get(9)?,
                origin_instance_id: row.get(10)?,
                pending_deletion: row.get::<_, i32>(11)? != 0,
            })
        })?;

        let mut profiles = Vec::new();
        for row in rows {
            profiles.push(row?);
        }
        Ok(profiles)
    }

    /// Increment profile version and update timestamp.
    pub fn increment_profile_version(&self, profile_id: Uuid) -> Result<()> {
        self.conn.execute(
            "UPDATE profiles SET version = version + 1,
             updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
             WHERE id = ?1",
            params![profile_id.to_string()],
        )?;
        Ok(())
    }

    /// Insert a profile tombstone (for deletion notification).
    pub fn insert_profile_tombstone(&self, profile_id: Uuid, deleted_at: &str) -> Result<()> {
        self.conn.execute(
            "INSERT INTO profile_tombstones (profile_id, deleted_at, delivered)
             VALUES (?1, ?2, 0) ON CONFLICT(profile_id) DO UPDATE SET deleted_at = ?2, delivered = 0",
            params![profile_id.to_string(), deleted_at],
        )?;
        Ok(())
    }

    /// List undelivered tombstones.
    pub fn list_undelivered_tombstones(&self) -> Result<Vec<(Uuid, String)>> {
        let mut stmt = self.conn.prepare(
            "SELECT profile_id, deleted_at FROM profile_tombstones WHERE delivered = 0",
        )?;

        let rows = stmt.query_map([], |row| {
            let id_str: String = row.get(0)?;
            let deleted_at: String = row.get(1)?;
            Ok((
                Uuid::parse_str(&id_str).unwrap_or_default(),
                deleted_at,
            ))
        })?;

        let mut tombstones = Vec::new();
        for row in rows {
            tombstones.push(row?);
        }
        Ok(tombstones)
    }

    /// Mark a tombstone as delivered.
    pub fn mark_tombstone_delivered(&self, profile_id: Uuid) -> Result<()> {
        self.conn.execute(
            "UPDATE profile_tombstones SET delivered = 1 WHERE profile_id = ?1",
            params![profile_id.to_string()],
        )?;
        Ok(())
    }

    /// Delete all anchors for a profile (used when replacing profile during replication).
    pub fn delete_anchors_for_profile(&self, profile_id: Uuid) -> Result<()> {
        self.conn.execute(
            "DELETE FROM anchors WHERE profile_id = ?1",
            params![profile_id.to_string()],
        )?;
        Ok(())
    }

    /// List profiles pending deletion (for UI prompt).
    pub fn list_pending_deletions(&self) -> Result<Vec<ProfileRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, mode, delete_propagation, conflict_policy, peer_name, created_at, updated_at, version, peer_id, origin_instance_id, pending_deletion
             FROM profiles WHERE pending_deletion = 1 ORDER BY name",
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
                version: row.get::<_, i64>(8)? as u64,
                peer_id: row.get(9)?,
                origin_instance_id: row.get(10)?,
                pending_deletion: row.get::<_, i32>(11)? != 0,
            })
        })?;

        let mut profiles = Vec::new();
        for row in rows {
            profiles.push(row?);
        }
        Ok(profiles)
    }

    /// Clear pending_deletion flag to restore profile to active state.
    pub fn clear_pending_deletion(&self, profile_id: Uuid) -> Result<()> {
        self.conn.execute(
            "UPDATE profiles SET pending_deletion = 0 WHERE id = ?1",
            params![profile_id.to_string()],
        )?;
        Ok(())
    }
}
