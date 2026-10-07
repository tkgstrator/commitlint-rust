//! Durable isolated execution and expected-old-OID promotion; never pushes.
use super::{Journal, Proposal, inspect, plan, replay, storage};
use crate::{Config, Result, core};
use std::{
    fs,
    path::Path,
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

pub(super) fn run(path: &Path, confirmation: Option<&str>, cfg: &Config) -> Result<()> {
    let _strict = crate::auth::strict_scope();
    let common = core::git_text(
        &cfg.git,
        &["rev-parse", "--path-format=absolute", "--git-common-dir"],
    )?;
    let base_root = storage::root(Path::new(&common))?;
    let proposal_path = inspect::safe_path(path, &cfg.git)?;
    let requested: Proposal = storage::read(&proposal_path, false)?;
    if requested.plan_id.len() != 64
        || !requested
            .plan_id
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err("invalid plan ID".into());
    }
    let root = base_root.join(format!("operation-{}", requested.plan_id));
    // A retry is inspection, not an invitation to break a stale lock or rerun
    // any step whose effects may already exist after a crash.
    if root.join("journal.json").exists() {
        let old = storage::read_journal(&root)?;
        let proposal = &requested;
        let actual = replay::query(
            cfg,
            &old.receipt.source_root,
            &["rev-parse", &old.receipt.branch],
        )?;
        let expected = old.mapping.last().map(|m| m.new_oid.as_str());
        if old.plan_id == proposal.plan_id
            && old.phase == "complete"
            && expected == Some(actual.as_str())
        {
            let (candidates, digest) = plan::completed_candidates(proposal, &old.receipt, cfg)?;
            if proposal_path != old.receipt.proposal_path
                || digest != old.apply_digest
                || storage::hash(&candidates)? != storage::hash(&old.candidates)?
            {
                return Err(
                    "completed operation request differs from the applied content/path".into(),
                );
            }
            if (old.receipt.operation == "author-migration"
                && confirmation != Some(digest.as_str()))
                || (old.receipt.operation != "author-migration" && confirmation.is_some())
            {
                return Err(
                    "completed operation requires its original migration confirmation semantics"
                        .into(),
                );
            }
            if core::account(&core::tools(Some(cfg))?)? != old.receipt.identity
                || crate::auth::context_fingerprint(&core::tools(Some(cfg))?)?
                    != old.receipt.auth_context
                || replay::query(cfg, &old.receipt.source_root, &["symbolic-ref", "HEAD"])?
                    != old.receipt.branch
                || Path::new(&common)
                    .canonicalize()
                    .map_err(|_| "repository unavailable")?
                    != old.receipt.common_dir
                || Path::new(&core::git_text(
                    &cfg.git,
                    &["rev-parse", "--absolute-git-dir"],
                )?)
                .canonicalize()
                .map_err(|_| "worktree unavailable")?
                    != old.receipt.git_dir
            {
                return Err(
                    "completed operation account or repository/worktree context changed".into(),
                );
            }
            if old.mapping.len() != old.verified.len()
                || old.mapping.len() != old.receipt.sources.len()
            {
                return Err("completed operation has incomplete verification records".into());
            }
            for (index, mapped) in old.mapping.iter().enumerate() {
                replay::verify_one(cfg, &old, index, &mapped.new_oid)?;
                if inspect::source(&cfg.git, &mapped.new_oid)? != old.verified[index] {
                    return Err(
                        "completed replacement no longer matches its verified record".into(),
                    );
                }
            }
            if replay::query(
                cfg,
                &old.receipt.source_root,
                &["rev-parse", &old.backup_ref],
            )? != old.original_tip
            {
                return Err("completed backup is missing or changed".into());
            }
            println!(
                "no-op: operation {} already complete; backup {}",
                old.plan_id, old.backup_ref
            );
            return Ok(());
        }
        return Err(format!(
            "existing operation {} phase {}; original ref now {}; staging {}; backup {}; inspect journal.json and recover explicitly (no automatic reset or lock removal)",
            old.plan_id,
            old.phase,
            actual,
            old.staging.display(),
            old.backup_ref
        ));
    }
    if fs::symlink_metadata(base_root.join("lock")).is_ok() {
        let operation: String = storage::read(&base_root.join("lock"), true)?;
        if operation.len() != 64
            || !operation
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err(
                "existing repair lock contains an invalid operation ID; inspect manually".into(),
            );
        }
        let active = base_root.join(format!("operation-{operation}"));
        let tip = core::git_text(&cfg.git, &["rev-parse", "HEAD"])?;
        let phase = storage::read_journal(&active)
            .map(|j| j.phase)
            .unwrap_or_else(|_| "unreadable or pre-journal interruption".into());
        return Err(format!(
            "repository repair lock exists for {operation}, phase {phase}, current HEAD {tip}; inspect {} and actual refs; no automatic lock removal",
            active.display()
        ));
    }
    let (proposal, receipt, candidates, digest) = plan::load(path, Some(cfg))?;
    if receipt.operation == "author-migration" {
        if confirmation != Some(digest.as_str()) {
            return Err("author migration requires --confirm-author-migration with the current preview apply digest".into());
        }
    } else if confirmation.is_some() {
        return Err("migration confirmation is only valid for author migration".into());
    }
    prerequisites(cfg)?;
    if receipt
        .sources
        .iter()
        .zip(&candidates)
        .all(|(source, candidate)| {
            source.message == candidate.message
                && core::check_ident(&source.author, &receipt.identity, "author").is_ok()
                && core::check_ident(&source.committer, &receipt.identity, "committer").is_ok()
                && core::validate_commit(&cfg.git, &source.source_oid, &receipt.identity).is_ok()
        })
    {
        println!("no-op: approved history already satisfies policy");
        return Ok(());
    }
    let root = base_root.join(format!("operation-{}", proposal.plan_id));
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| "system clock predates epoch")?
        .as_secs();
    let mut journal = Journal {
        schema_version: 1,
        plan_id: proposal.plan_id.clone(),
        apply_digest: digest,
        original_tip: receipt.tip.clone(),
        backup_ref: format!("refs/commitguard/backups/{}", proposal.plan_id),
        staging: root.join(format!("staging-{}", proposal.plan_id)),
        staging_git_dir: None,
        phase: "prepared".into(),
        editor_index: 0,
        committer_date: format!("{timestamp} +0000"),
        receipt,
        candidates,
        mapping: Vec::new(),
        verified: Vec::new(),
        failure: None,
    };
    storage::journal_capacity(&journal)?;
    storage::lock(&base_root, &proposal.plan_id)?;
    let root = storage::operation_root(Path::new(&common), &proposal.plan_id)?;
    storage::save_journal(&root, &journal)?;
    let result = execute(cfg, &root, &mut journal);
    if let Err(error) = result {
        // Editors may have durably advanced the journal even when Git refused.
        if let Ok(latest) = storage::read_journal(&root) {
            journal = latest;
        }
        journal.failure = Some(error.clone());
        let prior_phase = journal.phase.clone();
        journal.phase = format!("failed:{prior_phase}");
        let save = storage::save_journal(&root, &journal);
        let actual = replay::query(
            cfg,
            &journal.receipt.source_root,
            &["rev-parse", &journal.receipt.branch],
        )
        .unwrap_or_else(|_| "unreadable".into());
        return Err(format!(
            "{error}; operation {} phase {}; original ref now {}; backup {}; staging {}; verified replacements {}; journal retained{}; inspect state and recover explicitly, never automatically reset",
            journal.plan_id,
            prior_phase,
            actual,
            journal.backup_ref,
            journal.staging.display(),
            journal.mapping.len(),
            if save.is_err() {
                " (failure journal update also failed)"
            } else {
                ""
            }
        ));
    }
    println!(
        "repair complete; backup {}; journal {}; no push performed",
        journal.backup_ref,
        root.join("journal.json").display()
    );
    Ok(())
}
fn execute(cfg: &Config, root: &Path, j: &mut Journal) -> Result<()> {
    storage::create(&root.join("guard-config.json"), cfg)?;
    inspect::recheck(&j.receipt, Some(cfg), None)?;
    j.phase = "backup-intent".into();
    storage::save_journal(root, j)?;
    replay::guarded(
        cfg,
        root,
        j,
        &j.receipt.source_root,
        &[
            "update-ref".into(),
            j.backup_ref.clone(),
            j.original_tip.clone(),
            "0".repeat(j.original_tip.len()),
        ],
        None,
        false,
    )?;
    if replay::query(cfg, &j.receipt.source_root, &["rev-parse", &j.backup_ref])? != j.original_tip
    {
        return Err("backup does not preserve original tip".into());
    }
    j.phase = "worktree-intent".into();
    storage::save_journal(root, j)?;
    replay::guarded(
        cfg,
        root,
        j,
        &j.receipt.source_root,
        &[
            "worktree".into(),
            "add".into(),
            "--detach".into(),
            j.staging.to_string_lossy().into_owned(),
            j.original_tip.clone(),
        ],
        None,
        false,
    )?;
    j.staging_git_dir =
        Some(replay::query(cfg, &j.staging, &["rev-parse", "--absolute-git-dir"])?.into());
    j.phase = "staged".into();
    storage::save_journal(root, j)?;
    if replay::query(
        cfg,
        &j.staging,
        &["status", "--porcelain=v1", "--untracked-files=all"],
    )? != ""
    {
        return Err("checkout hooks dirtied staging; evidence retained".into());
    }
    inspect::recheck(&j.receipt, Some(cfg), Some(&j.staging))?;
    replay::run(cfg, root, j)?;
    if j.mapping.len() != j.receipt.sources.len() {
        return Err("incomplete verified mapping".into());
    }
    let final_oid = j
        .mapping
        .last()
        .ok_or("empty result mapping")?
        .new_oid
        .clone();
    let count = replay::query(
        cfg,
        &j.staging,
        &[
            "rev-list",
            "--count",
            &format!("{}..{final_oid}", j.receipt.base),
        ],
    )?;
    if count != j.receipt.sources.len().to_string()
        || replay::query(cfg, &j.staging, &["rev-parse", "HEAD"])? != final_oid
    {
        return Err("replacement count or staging tip changed".into());
    }
    for (index, mapped) in j.mapping.iter().enumerate() {
        replay::verify_one(cfg, j, index, &mapped.new_oid)?;
        if inspect::source(&cfg.git, &mapped.new_oid)? != j.verified[index] {
            return Err("actual replacement differs from durable verified bytes".into());
        }
    }
    if replay::query(
        cfg,
        &j.staging,
        &["status", "--porcelain=v1", "--untracked-files=all"],
    )? != ""
    {
        return Err("hooks modified staging worktree; retained evidence".into());
    }
    j.phase = "promotion-intent".into();
    storage::save_journal(root, j)?;
    // Fresh account, live ALL remote destinations, original branch/index/files,
    // effective hook/configuration, and every other worktree/ref are rechecked.
    inspect::recheck(&j.receipt, Some(cfg), Some(&j.staging))?;
    replay::guarded(
        cfg,
        root,
        j,
        &j.receipt.source_root,
        &[
            "update-ref".into(),
            j.receipt.branch.clone(),
            final_oid.clone(),
            j.original_tip.clone(),
        ],
        None,
        false,
    )?;
    j.phase = "promoted".into();
    storage::save_journal(root, j)?;
    if replay::query(
        cfg,
        &j.receipt.source_root,
        &["rev-parse", &j.receipt.branch],
    )? != final_oid
        || replay::query(
            cfg,
            &j.receipt.source_root,
            &["status", "--porcelain=v1", "--untracked-files=all"],
        )? != ""
    {
        return Err("post-promotion hook/concurrent change detected; no automatic rollback".into());
    }
    inspect::postcheck(&j.receipt, cfg, Some(&j.staging), &final_oid)?;
    // Ordinary worktree remove intentionally refuses hook-created dirty evidence.
    j.phase = "cleanup-intent".into();
    storage::save_journal(root, j)?;
    if replay::guarded(
        cfg,
        root,
        j,
        &j.receipt.source_root,
        &[
            "worktree".into(),
            "remove".into(),
            j.staging.to_string_lossy().into_owned(),
        ],
        None,
        false,
    )
    .is_err()
    {
        j.phase = "cleanup-incomplete".into();
        storage::save_journal(root, j)?;
        return Err("promotion succeeded but ordinary staging cleanup refused; retained worktree and journal".into());
    }
    if j.staging.exists() {
        return Err("staging directory remains after cleanup".into());
    }
    inspect::postcheck(&j.receipt, cfg, None, &final_oid)?;
    j.phase = "complete".into();
    storage::save_journal(root, j)?;
    storage::release_lock(&storage::root(&j.receipt.common_dir)?)
}
fn prerequisites(cfg: &Config) -> Result<()> {
    if !cfg.canonical_cli().is_file()
        || !cfg.hooks().is_dir()
        || !fs::metadata(&cfg.git).map(|m| m.is_file()).unwrap_or(false)
    {
        return Err("guarded Git/config/hooks must be explicitly installed before apply".into());
    }
    let version = core::git_text(&cfg.git, &["--version"])?;
    let number = version
        .strip_prefix("git version ")
        .ok_or("unrecognized Git version")?;
    let mut fields = number.split('.');
    let major: u32 = fields
        .next()
        .ok_or("missing Git version")?
        .parse()
        .map_err(|_| "invalid Git version")?;
    let minor: u32 = fields
        .next()
        .ok_or("missing Git version")?
        .parse()
        .map_err(|_| "invalid Git version")?;
    if major < 2 || (major == 2 && minor < 38) {
        return Err("fix requires Git >= 2.38".into());
    }
    let help = Command::new(&cfg.git)
        .args(["rebase", "-h"])
        .output()
        .map_err(|_| "cannot probe Git rebase features")?;
    let output = format!(
        "{}{}",
        String::from_utf8_lossy(&help.stdout),
        String::from_utf8_lossy(&help.stderr)
    )
    .replace("--[no-]", "--");
    for flag in [
        "--update-refs",
        "--empty",
        "--interactive",
        "--force-rebase",
    ] {
        if !output.contains(flag) {
            return Err("required Git rebase feature probe failed".into());
        }
    }
    Ok(())
}
