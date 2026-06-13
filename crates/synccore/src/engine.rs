use std::collections::BTreeMap;
use std::io;
use std::path::Path;
use std::time::SystemTime;

use tracing::info;
use uuid::Uuid;

use crate::apply::{self, ApplyContext, ApplyResult};
use crate::diff::{self, DiffResult, IndexEntry, SyncIndex};
use crate::path::RelPath;
use crate::plan;
use crate::reconcile::{self, ConflictPolicy, ReconcileContext, SyncMode, SyncPlan};
use crate::scan::{self, EntryKind, ScanConfig, Snapshot};

/// Profile configuration for a sync run.
#[derive(Debug, Clone)]
pub struct Profile {
    pub id: Uuid,
    pub name: String,
    pub mode: SyncMode,
    pub delete_propagation: bool,
    pub conflict_policy: ConflictPolicy,
    pub anchors: Vec<Anchor>,
    pub peer_name: String,
}

/// A single folder pair in a profile.
#[derive(Debug, Clone)]
pub struct Anchor {
    pub local_path: std::path::PathBuf,
    pub remote_path: std::path::PathBuf,
    pub config: ScanConfig,
}

/// The complete result of a sync run.
#[derive(Debug)]
pub struct RunResult {
    pub run_id: Uuid,
    pub profile_id: Uuid,
    pub started_at: SystemTime,
    pub finished_at: SystemTime,
    pub anchor_results: Vec<AnchorRunResult>,
}

/// Result for a single anchor within a run.
#[derive(Debug)]
pub struct AnchorRunResult {
    pub anchor_index: usize,
    pub diff: DiffResult,
    pub plan: SyncPlan,
    pub apply: ApplyResult,
    pub new_index: SyncIndex,
}

/// Run a sync for a profile, given an existing sync index (empty for first run).
///
/// This is the engine's top-level entry point. It orchestrates:
/// scan → diff → reconcile → plan → apply → new_index
pub fn run_sync(profile: &Profile, index: &SyncIndex) -> io::Result<RunResult> {
    let run_id = Uuid::new_v4();
    let started_at = SystemTime::now();

    info!(
        run_id = %run_id,
        profile = %profile.name,
        "starting sync run"
    );

    let mut anchor_results = Vec::new();

    for (anchor_idx, anchor) in profile.anchors.iter().enumerate() {
        let result = run_anchor(profile, anchor, anchor_idx, index)?;
        anchor_results.push(result);
    }

    let finished_at = SystemTime::now();

    Ok(RunResult {
        run_id,
        profile_id: profile.id,
        started_at,
        finished_at,
        anchor_results,
    })
}

fn run_anchor(
    profile: &Profile,
    anchor: &Anchor,
    anchor_index: usize,
    index: &SyncIndex,
) -> io::Result<AnchorRunResult> {
    // 1. Scan both sides
    let local_snap = scan::scan_tree(&anchor.local_path, &anchor.config)?;
    let remote_snap = scan::scan_tree(&anchor.remote_path, &anchor.config)?;

    // 2. Compute diff against index
    let diff = diff::compute_diff(
        &local_snap,
        &remote_snap,
        index,
        &anchor.local_path,
        &anchor.remote_path,
    )?;

    // 3. Reconcile
    let ctx = ReconcileContext {
        local_entries: &local_snap.entries,
        remote_entries: &remote_snap.entries,
        delete_propagation: profile.delete_propagation,
        peer_name: profile.peer_name.clone(),
    };
    let mut sync_plan =
        reconcile::reconcile(&diff, index, profile.mode, profile.conflict_policy, &ctx);

    // 4. Order and dedup actions
    plan::dedup_dirs(&mut sync_plan);
    plan::order_actions(&mut sync_plan);

    // 5. Apply
    let apply_ctx = ApplyContext {
        local_root: &anchor.local_path,
        remote_root: &anchor.remote_path,
    };
    let apply_result = apply::apply_plan(&sync_plan, &apply_ctx);

    // 6. Build new index from final state of both sides
    let new_index = build_post_sync_index(&anchor.local_path, &local_snap, &remote_snap);

    Ok(AnchorRunResult {
        anchor_index,
        diff,
        plan: sync_plan,
        apply: apply_result,
        new_index,
    })
}

/// Build the new sync index after a successful apply.
/// Re-scans paths that were modified to get current hashes.
fn build_post_sync_index(
    local_root: &Path,
    local_snap: &Snapshot,
    remote_snap: &Snapshot,
) -> SyncIndex {
    let mut entries = BTreeMap::new();

    // Merge both snapshots to get the expected converged state
    let all_paths: std::collections::BTreeSet<&RelPath> = local_snap
        .entries
        .keys()
        .chain(remote_snap.entries.keys())
        .collect();

    for path in all_paths {
        let entry = local_snap
            .entries
            .get(path)
            .or_else(|| remote_snap.entries.get(path));

        if let Some(fe) = entry {
            if fe.kind != EntryKind::File {
                continue;
            }

            let full_path = local_root.join(fe.path.to_path_buf());
            let hash = if full_path.exists() {
                scan::hash_file(&full_path).unwrap_or_default()
            } else {
                String::new()
            };

            let mtime_secs = fe
                .mtime
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_secs() as i64);

            entries.insert(
                path.clone(),
                IndexEntry {
                    path: path.clone(),
                    kind: fe.kind,
                    size: fe.size,
                    mtime_secs,
                    hash,
                },
            );
        }
    }

    SyncIndex { entries }
}
