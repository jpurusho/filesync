use crate::reconcile::{Action, SyncPlan};

/// Order actions for safe execution:
/// 1. CreateDir (parents before children — sorted by path depth ascending)
/// 2. CopyFile / RenameConflict (any order)
/// 3. Delete (children before parents — sorted by path depth descending)
///
/// This ensures directories exist before files are copied into them,
/// and that files are removed before their parent directories.
pub fn order_actions(plan: &mut SyncPlan) {
    plan.actions.sort_by(|a, b| {
        let phase_a = action_phase(a);
        let phase_b = action_phase(b);
        phase_a.cmp(&phase_b).then_with(|| {
            // Within the same phase, sort by depth
            let depth_a = action_depth(a);
            let depth_b = action_depth(b);
            match phase_a {
                // CreateDir: shallow first
                0 => depth_a.cmp(&depth_b),
                // Delete: deep first
                2 => depth_b.cmp(&depth_a),
                // CopyFile/RenameConflict: stable (any order)
                _ => std::cmp::Ordering::Equal,
            }
        })
    });
}

/// Deduplicate CreateDir actions (same path + side).
pub fn dedup_dirs(plan: &mut SyncPlan) {
    let mut seen = std::collections::HashSet::new();
    plan.actions.retain(|action| {
        if let Action::CreateDir { on, path } = action {
            let key = (std::mem::discriminant(on), path.normalized().to_owned());
            seen.insert(key)
        } else {
            true
        }
    });
}

fn action_phase(action: &Action) -> u8 {
    match action {
        Action::CreateDir { .. } => 0,
        Action::CopyFile { .. } | Action::RenameConflict { .. } => 1,
        Action::Delete { .. } => 2,
    }
}

fn action_depth(action: &Action) -> usize {
    let path = match action {
        Action::CreateDir { path, .. }
        | Action::CopyFile { path, .. }
        | Action::Delete { path, .. }
        | Action::RenameConflict { path, .. } => path,
    };
    path.display().matches('/').count()
}

#[cfg(test)]
mod tests {
    use crate::path::RelPath;
    use crate::reconcile::{Action, Side, SyncPlan};

    use super::*;

    #[test]
    fn dirs_before_copies_before_deletes() {
        let mut plan = SyncPlan {
            actions: vec![
                Action::Delete {
                    on: Side::Remote,
                    path: RelPath::new("old.txt"),
                },
                Action::CopyFile {
                    from: Side::Local,
                    path: RelPath::new("new.txt"),
                },
                Action::CreateDir {
                    on: Side::Remote,
                    path: RelPath::new("subdir"),
                },
            ],
            conflicts: vec![],
        };

        order_actions(&mut plan);

        assert!(matches!(&plan.actions[0], Action::CreateDir { .. }));
        assert!(matches!(&plan.actions[1], Action::CopyFile { .. }));
        assert!(matches!(&plan.actions[2], Action::Delete { .. }));
    }

    #[test]
    fn parent_dirs_created_before_children() {
        let mut plan = SyncPlan {
            actions: vec![
                Action::CreateDir {
                    on: Side::Remote,
                    path: RelPath::new("a/b/c"),
                },
                Action::CreateDir {
                    on: Side::Remote,
                    path: RelPath::new("a"),
                },
                Action::CreateDir {
                    on: Side::Remote,
                    path: RelPath::new("a/b"),
                },
            ],
            conflicts: vec![],
        };

        order_actions(&mut plan);

        let paths: Vec<_> = plan
            .actions
            .iter()
            .map(|a| match a {
                Action::CreateDir { path, .. } => path.display().to_string(),
                _ => String::new(),
            })
            .collect();
        assert_eq!(paths, vec!["a", "a/b", "a/b/c"]);
    }

    #[test]
    fn children_deleted_before_parents() {
        let mut plan = SyncPlan {
            actions: vec![
                Action::Delete {
                    on: Side::Remote,
                    path: RelPath::new("dir"),
                },
                Action::Delete {
                    on: Side::Remote,
                    path: RelPath::new("dir/sub/file.txt"),
                },
                Action::Delete {
                    on: Side::Remote,
                    path: RelPath::new("dir/sub"),
                },
            ],
            conflicts: vec![],
        };

        order_actions(&mut plan);

        let paths: Vec<_> = plan
            .actions
            .iter()
            .map(|a| match a {
                Action::Delete { path, .. } => path.display().to_string(),
                _ => String::new(),
            })
            .collect();
        assert_eq!(paths, vec!["dir/sub/file.txt", "dir/sub", "dir"]);
    }

    #[test]
    fn dedup_removes_duplicate_dirs() {
        let mut plan = SyncPlan {
            actions: vec![
                Action::CreateDir {
                    on: Side::Remote,
                    path: RelPath::new("sub"),
                },
                Action::CreateDir {
                    on: Side::Remote,
                    path: RelPath::new("sub"),
                },
                Action::CopyFile {
                    from: Side::Local,
                    path: RelPath::new("sub/file.txt"),
                },
            ],
            conflicts: vec![],
        };

        dedup_dirs(&mut plan);

        let dir_count = plan
            .actions
            .iter()
            .filter(|a| matches!(a, Action::CreateDir { .. }))
            .count();
        assert_eq!(dir_count, 1);
    }
}
