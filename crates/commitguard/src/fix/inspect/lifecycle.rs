//! Inspection and distinct pre- and post-promotion receipt checks.
use super::{
    exemptions::operation_exemptions,
    fingerprint::fingerprint,
    ownership::{ownership_file, ownership_gate},
    paths::{safe_path, unix},
    publication::{destinations, unpublished},
    repository::{ancestry, dirs, original_hooks, repository_safety, status},
    source::source,
};
use crate::fix::{Receipt, storage};
use crate::{Config, Result, core};
use std::path::Path;

pub(in crate::fix) fn inspect(
    range: &str,
    proposal_path: &Path,
    ownership: Option<&Path>,
    cfg: Option<&Config>,
) -> Result<Receipt> {
    unix()?;
    let tools = core::tools(cfg)?;
    let git = &tools.git;
    let (common_dir, git_dir, source_root) = dirs(git)?;
    let proposal_path = safe_path(proposal_path, git)?;
    if proposal_path.starts_with(&common_dir) {
        return Err(
            "proposal output must be outside the common Git directory and every worktree".into(),
        );
    }
    repository_safety(git, &common_dir, &git_dir)?;
    if !status(git, &source_root)?.is_empty() {
        return Err("repair requires a clean index/worktree including untracked files".into());
    }
    let (base_ref, end) = range.split_once("..").ok_or("range must be BASE..HEAD")?;
    if base_ref.is_empty()
        || base_ref.starts_with('-')
        || base_ref.contains(['\n', '\r', '\0'])
        || end != "HEAD"
        || base_ref.contains("..")
    {
        return Err("range must be an existing excluded BASE..HEAD".into());
    }
    let branch = core::git_text(git, &["symbolic-ref", "--quiet", "HEAD"])
        .map_err(|_| "repair requires the current named branch")?;
    if !branch.starts_with("refs/heads/") {
        return Err("repair requires a named local branch".into());
    }
    let tip = core::resolve_commit(git, "HEAD")?.ok_or("source HEAD is not a commit")?;
    let base =
        core::resolve_commit(git, base_ref)?.ok_or("range base is not an existing commit")?;
    if base == tip || !ancestry(git, &base, &tip)? {
        return Err("base must be a strict ancestor of HEAD".into());
    }
    let object_format = core::git_text(git, &["rev-parse", "--show-object-format"])?;
    if !["sha1", "sha256"].contains(&object_format.as_str()) {
        return Err("unsupported object format".into());
    }
    let listed = core::git_text(
        git,
        &["rev-list", "--reverse", &format!("{base}..{tip}"), "--"],
    )?;
    let mut sources = Vec::new();
    let mut parent = base.clone();
    for oid in listed.lines() {
        let source = source(git, oid)?;
        if source.parent != parent {
            return Err("range must be the complete linear suffix of HEAD".into());
        }
        parent = source.source_oid.clone();
        sources.push(source);
    }
    if sources.is_empty() || parent != tip {
        return Err("range is not a complete nonempty suffix".into());
    }
    let identity = core::account(&tools)?;
    let ownership_values = match ownership {
        Some(path) => ownership_file(path, git)?,
        None => Vec::new(),
    };
    let hooks_dir = original_hooks(git, cfg, &common_dir, &git_dir, &source_root)?;
    let mut receipt = Receipt {
        schema_version: 2,
        policy_version: 2,
        operation: if ownership.is_some() {
            "author-migration"
        } else {
            "repair"
        }
        .into(),
        proposal_path,
        common_dir,
        git_dir,
        source_root,
        branch,
        object_format,
        tip,
        base,
        sources,
        auth_context: crate::auth::context_fingerprint(&tools)?,
        identity,
        ownership: ownership_values,
        hooks_dir,
        fingerprint: String::new(),
        destinations: destinations(git)?,
    };
    ownership_gate(&receipt, git)?;
    unpublished(git, &receipt.destinations, &receipt.sources)?;
    repository_safety(git, &receipt.common_dir, &receipt.git_dir)?;
    if original_hooks(
        git,
        cfg,
        &receipt.common_dir,
        &receipt.git_dir,
        &receipt.source_root,
    )? != receipt.hooks_dir
        || destinations(git)? != receipt.destinations
        || core::account(&tools)? != receipt.identity
        || crate::auth::context_fingerprint(&tools)? != receipt.auth_context
    {
        return Err("account, destinations or hook context changed during inspection".into());
    }
    receipt.fingerprint = fingerprint(git, cfg, &receipt, None, None, None)?;
    Ok(receipt)
}

pub(in crate::fix) fn recheck(
    r: &Receipt,
    cfg: Option<&Config>,
    staging: Option<&Path>,
) -> Result<()> {
    unix()?;
    if r.schema_version != 2
        || r.policy_version != 2
        || !["repair", "author-migration"].contains(&r.operation.as_str())
    {
        return Err("unsupported repair receipt schema/policy/operation".into());
    }
    let tools = core::tools(cfg)?;
    let git = &tools.git;
    let (common, git_dir, root) = dirs(git)?;
    if (common.clone(), git_dir.clone(), root.clone())
        != (
            r.common_dir.clone(),
            r.git_dir.clone(),
            r.source_root.clone(),
        )
    {
        return Err("repair receipt belongs to another repository/worktree".into());
    }
    repository_safety(git, &common, &git_dir)?;
    if !status(git, &root)?.is_empty() {
        return Err("original index/worktree changed or is dirty".into());
    }
    if core::git_text(git, &["symbolic-ref", "--quiet", "HEAD"])? != r.branch
        || core::resolve_commit(git, "HEAD")?.as_deref() != Some(r.tip.as_str())
        || core::git_text(git, &["rev-parse", "--show-object-format"])? != r.object_format
    {
        return Err("original branch/tip/object format changed".into());
    }
    if core::account(&tools)? != r.identity
        || crate::auth::context_fingerprint(&tools)? != r.auth_context
    {
        return Err("fresh gh account changed since planning".into());
    }
    if safe_path(&r.proposal_path, git)? != r.proposal_path {
        return Err("proposal path changed".into());
    }
    if r.base == r.tip || !ancestry(git, &r.base, &r.tip)? {
        return Err("frozen base is not a strict ancestor".into());
    }
    let listed = core::git_text(
        git,
        &[
            "rev-list",
            "--reverse",
            &format!("{}..{}", r.base, r.tip),
            "--",
        ],
    )?;
    let frozen: Vec<_> = r.sources.iter().map(|s| s.source_oid.as_str()).collect();
    if listed.lines().collect::<Vec<_>>() != frozen {
        return Err("frozen source range changed".into());
    }
    let mut parent = r.base.as_str();
    for frozen in &r.sources {
        if source(git, &frozen.source_oid)? != *frozen || frozen.parent != parent {
            return Err("raw source bytes/linear ancestry changed".into());
        }
        parent = &frozen.source_oid;
    }
    ownership_gate(r, git)?;
    if original_hooks(git, cfg, &common, &git_dir, &root)? != r.hooks_dir {
        return Err("effective original hook chain changed".into());
    }
    let urls = destinations(git)?;
    if urls != r.destinations {
        return Err("fetch/push destinations changed".into());
    }
    // Exemptions are authorized below by the durable operation journal, never
    // by a namespace-wide backup rule or an arbitrary caller-supplied path.
    let (staging, backup) = operation_exemptions(r, staging, git)?;
    unpublished(git, &urls, &r.sources)?;
    // Network helpers can run arbitrary code too. Check local shared state
    // after live publication inspection, immediately before returning to CAS.
    repository_safety(git, &common, &git_dir)?;
    if original_hooks(git, cfg, &common, &git_dir, &root)? != r.hooks_dir
        || destinations(git)? != r.destinations
        || core::account(&tools)? != r.identity
        || crate::auth::context_fingerprint(&tools)? != r.auth_context
    {
        return Err(
            "account, destinations or original hook context changed during publication checks"
                .into(),
        );
    }
    if fingerprint(git, cfg, r, staging.as_deref(), backup.as_deref(), None)? != r.fingerprint {
        return Err(
            "effective configuration, executable, hook chain or shared state changed".into(),
        );
    }
    Ok(())
}

/// Validate post-promotion state without weakening the pre-promotion receipt.
/// Only the recorded target branch/HEAD movement is normalized in the digest.
pub(in crate::fix) fn postcheck(
    r: &Receipt,
    cfg: &Config,
    staging: Option<&Path>,
    final_oid: &str,
) -> Result<()> {
    unix()?;
    if !core::oid_valid(final_oid)
        || r.schema_version != 2
        || r.policy_version != 2
        || !["repair", "author-migration"].contains(&r.operation.as_str())
    {
        return Err("invalid post-promotion receipt/result".into());
    }
    let tools = core::tools(Some(cfg))?;
    let git = &tools.git;
    let (common, git_dir, root) = dirs(git)?;
    if (common.clone(), git_dir.clone(), root.clone())
        != (
            r.common_dir.clone(),
            r.git_dir.clone(),
            r.source_root.clone(),
        )
    {
        return Err("post-promotion receipt belongs to another repository/worktree".into());
    }
    repository_safety(git, &common, &git_dir)?;
    if core::git_text(git, &["symbolic-ref", "--quiet", "HEAD"])? != r.branch
        || core::resolve_commit(git, "HEAD")?.as_deref() != Some(final_oid)
        || core::git_text(git, &["show-ref", "--verify", "--hash", &r.branch])? != final_oid
        || core::git_text(git, &["rev-parse", "--show-object-format"])? != r.object_format
    {
        return Err("post-promotion branch/tip/object format changed".into());
    }
    let journal_root = common
        .join("commitguard-fix")
        .join(format!("operation-{}", storage::hash(r)?));
    let journal = storage::read_journal(&journal_root)?;
    if !["promoted", "cleanup-intent"].contains(&journal.phase.as_str())
        || journal.mapping.len() != r.sources.len()
        || journal.mapping.last().map(|m| m.new_oid.as_str()) != Some(final_oid)
    {
        return Err("post-promotion result does not match the durable operation".into());
    }
    let (staging, backup) = operation_exemptions(r, staging, git)?;
    if backup.is_none() {
        return Err("post-promotion backup is unavailable".into());
    }
    if safe_path(&r.proposal_path, git)? != r.proposal_path {
        return Err("proposal path changed".into());
    }
    if r.base == r.tip || !ancestry(git, &r.base, &r.tip)? {
        return Err("frozen base is not a strict ancestor".into());
    }
    let listed = core::git_text(
        git,
        &[
            "rev-list",
            "--reverse",
            &format!("{}..{}", r.base, r.tip),
            "--",
        ],
    )?;
    if listed.lines().collect::<Vec<_>>()
        != r.sources
            .iter()
            .map(|s| s.source_oid.as_str())
            .collect::<Vec<_>>()
    {
        return Err("frozen source range changed after promotion".into());
    }
    let mut parent = r.base.as_str();
    for frozen in &r.sources {
        if source(git, &frozen.source_oid)? != *frozen || frozen.parent != parent {
            return Err("raw source bytes/linear ancestry changed after promotion".into());
        }
        parent = &frozen.source_oid;
    }
    ownership_gate(r, git)?;
    if core::account(&tools)? != r.identity
        || crate::auth::context_fingerprint(&tools)? != r.auth_context
        || original_hooks(git, Some(cfg), &common, &git_dir, &root)? != r.hooks_dir
        || destinations(git)? != r.destinations
    {
        return Err(
            "account, destinations or original hook context changed after promotion".into(),
        );
    }
    unpublished(git, &r.destinations, &r.sources)?;
    // Publication helpers and hooks can alter state too; run the complete
    // frozen-state check after those processes, including backup existence.
    repository_safety(git, &common, &git_dir)?;
    if core::account(&tools)? != r.identity
        || crate::auth::context_fingerprint(&tools)? != r.auth_context
        || original_hooks(git, Some(cfg), &common, &git_dir, &root)? != r.hooks_dir
        || destinations(git)? != r.destinations
    {
        return Err("account, destinations or hook context changed during post-promotion publication checks".into());
    }
    let (staging, backup) = operation_exemptions(r, staging.as_deref(), git)?;
    if fingerprint(
        git,
        Some(cfg),
        r,
        staging.as_deref(),
        backup.as_deref(),
        Some(final_oid),
    )? != r.fingerprint
    {
        return Err(
            "post-promotion configuration, executable, hook chain or shared state changed".into(),
        );
    }
    Ok(())
}
