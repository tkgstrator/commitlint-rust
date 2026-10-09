//! Frozen shared refs and worktree state enforcement.
use super::{
    publication::ref_commit,
    repository::{ancestry, status, worktrees},
};
use crate::fix::Receipt;
use crate::{Result, core};
use std::path::Path;

pub(super) fn shared_state(
    git: &Path,
    r: &Receipt,
    staging: Option<&Path>,
    backup: Option<&str>,
    promoted_tip: Option<&str>,
) -> Result<Vec<u8>> {
    let mut state = Vec::new();
    let expected_tip = promoted_tip.unwrap_or(&r.tip);
    let mut branch_found = false;
    let refs = core::query(
        git,
        &[
            "for-each-ref".into(),
            "--format=%(refname) %(objectname)".into(),
        ],
        None,
    )?;
    for line in core::text(&refs)?.lines() {
        let (reference, oid) = line.split_once(' ').ok_or("malformed shared ref")?;
        if !core::oid_valid(oid) {
            return Err("malformed shared ref OID".into());
        }
        if Some(reference) == backup {
            if oid != r.tip {
                return Err("operation backup no longer matches source tip".into());
            }
            continue;
        }
        if reference == r.branch {
            if oid != expected_tip {
                return Err("target branch tip changed".into());
            }
            branch_found = true;
            // Only the approved branch movement is excluded from the frozen
            // shared-state digest. All other refs retain their actual bytes.
            state.extend_from_slice(format!("{} {}", r.branch, r.tip).as_bytes());
            state.push(0);
            continue;
        } else {
            if let Some(tip) = ref_commit(git, oid)? {
                core::query(
                    git,
                    &[
                        "rev-list".into(),
                        "--parents".into(),
                        tip.clone(),
                        "--".into(),
                    ],
                    None,
                )?;
                for source in &r.sources {
                    if ancestry(git, &source.source_oid, &tip)? {
                        return Err("selected source is reachable from another shared ref (including stash/backup)".into());
                    }
                }
            }
        }
        state.extend_from_slice(line.as_bytes());
        state.push(0);
    }
    if !branch_found {
        return Err("target branch disappeared".into());
    }
    let mut trees = worktrees(git)?;
    trees.sort_by(|a, b| a.path.cmp(&b.path));
    let mut found = false;
    for tree in trees {
        if staging == Some(tree.path.as_path()) {
            if tree.branch.is_some() {
                return Err("operation staging worktree must be detached".into());
            }
            continue;
        }
        if tree.path == r.source_root {
            if tree.head != expected_tip || tree.branch.as_deref() != Some(r.branch.as_str()) {
                return Err("source worktree branch/tip changed".into());
            }
            found = true;
        } else {
            if tree.branch.as_deref() == Some(r.branch.as_str()) {
                return Err("another worktree is attached to the source branch".into());
            }
            for source in &r.sources {
                if ancestry(git, &source.source_oid, &tree.head)? {
                    return Err("selected source is reachable from another worktree HEAD".into());
                }
            }
        }
        if tree.path == r.source_root && promoted_tip.is_some() {
            // Normalize the single parsed HEAD field, never arbitrary OID text
            // in paths, other worktrees, or any additional record fields.
            let normalized = tree
                .record
                .split('\0')
                .map(|field| {
                    if field.strip_prefix("HEAD ") == Some(expected_tip) {
                        format!("HEAD {}", r.tip)
                    } else {
                        field.to_owned()
                    }
                })
                .collect::<Vec<_>>()
                .join("\0");
            state.extend_from_slice(normalized.as_bytes());
        } else {
            state.extend_from_slice(tree.record.as_bytes());
        }
        state.push(0);
        let cleanliness = status(git, &tree.path)?;
        if !cleanliness.is_empty() {
            return Err("source or linked worktree index/files are dirty".into());
        }
        state.extend_from_slice(&cleanliness);
        state.push(0);
    }
    if !found {
        return Err("original source worktree disappeared".into());
    }
    Ok(state)
}
