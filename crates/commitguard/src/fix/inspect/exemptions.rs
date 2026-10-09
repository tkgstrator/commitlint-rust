//! Durable-journal authorization of exact backup and staging exemptions.
use super::paths::{absolute, reject_symlinks};
use crate::fix::{Receipt, storage};
use crate::{Result, core};
use std::{
    fs,
    path::{Path, PathBuf},
};

pub(super) fn operation_exemptions(
    r: &Receipt,
    staging: Option<&Path>,
    git: &Path,
) -> Result<(Option<PathBuf>, Option<String>)> {
    let base = r.common_dir.join("commitguard-fix");
    let id = storage::hash(r)?;
    let root = base.join(format!("operation-{id}"));
    let journal_path = root.join("journal.json");
    if fs::symlink_metadata(&journal_path).is_err() {
        if staging.is_some() {
            return Err("staging exemption requires an authenticated operation journal".into());
        }
        return Ok((None, None));
    }
    let journal = storage::read_journal(&root)?;
    let lock: String = storage::read(&base.join("lock"), true)?;
    if journal.plan_id != id
        || storage::hash(&journal.receipt)? != id
        || lock != id
        || journal.backup_ref != format!("refs/commitguard/backups/{id}")
        || journal.staging != root.join(format!("staging-{id}"))
        || journal.original_tip != r.tip
    {
        return Err("operation journal does not authorize these backup/staging exemptions".into());
    }
    // Prepared is the only phase before a backup has been created. Once an
    // operation mutates history, absence of its recovery ref is never exempt.
    if journal.phase == "prepared" {
        if staging.is_some() {
            return Err("prepared operation cannot authorize staging".into());
        }
        return Ok((None, None));
    }
    if core::git_text(
        git,
        &["show-ref", "--verify", "--hash", &journal.backup_ref],
    )
    .map_err(|_| "operation backup is missing".to_string())?
        != r.tip
    {
        return Err("operation backup no longer matches source tip".into());
    }
    let staging = if let Some(path) = staging {
        reject_symlinks(path)?;
        let path = absolute(path)?;
        if path != journal.staging
            || journal.staging_git_dir.is_none()
            || ![
                "staged",
                "verified",
                "promotion-intent",
                "promoted",
                "cleanup-intent",
            ]
            .contains(&journal.phase.as_str())
        {
            return Err("staging exemption does not match the recorded active operation".into());
        }
        let recorded = journal.staging_git_dir.as_ref().unwrap();
        let bytes = core::query(
            git,
            &[
                "-C".into(),
                path.to_str().ok_or("non-UTF-8 staging path")?.into(),
                "rev-parse".into(),
                "--absolute-git-dir".into(),
            ],
            None,
        )?;
        if absolute(Path::new(core::text(&bytes)?.trim()))? != *recorded {
            return Err("staging Git directory changed since journal registration".into());
        }
        Some(path)
    } else {
        None
    };
    Ok((staging, Some(journal.backup_ref)))
}
