//! Editable proposals contain only the source receipt ID and ordered messages.
use super::{Candidate, Proposal, Receipt, inspect, storage};
use crate::{Config, Result, core};
use std::path::Path;

fn receipt_path(root: &Path, id: &str) -> std::path::PathBuf {
    root.join(format!("receipt-{id}.json"))
}
fn valid_id(id: &str) -> bool {
    id.len() == 64
        && id
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

pub(super) fn export(
    range: &str,
    path: &Path,
    ownership: Option<&Path>,
    cfg: Option<&Config>,
) -> Result<()> {
    let receipt = inspect::inspect(range, path, ownership, cfg)?;
    let plan_id = storage::hash(&receipt)?;
    let proposal = Proposal {
        plan_id: plan_id.clone(),
        candidates: receipt
            .sources
            .iter()
            .map(|source| Candidate {
                source_oid: source.source_oid.clone(),
                message: source.message.clone(),
            })
            .collect(),
    };
    // Refuse an existing proposal before persisting its immutable receipt.
    if std::fs::symlink_metadata(&receipt.proposal_path).is_ok() {
        return Err("proposal output already exists; choose a new path".into());
    }
    let root = storage::root(&receipt.common_dir)?;
    storage::create(&receipt_path(&root, &plan_id), &receipt)?;
    storage::create(&receipt.proposal_path, &proposal)?;
    println!("Plan ID: {plan_id}");
    println!("Proposal: {}", receipt.proposal_path.display());
    println!("Original messages exported unchanged; edit candidates only, then preview.");
    let tools = core::tools(cfg)?;
    for source in &receipt.sources {
        match core::validate_message_with_git(
            &tools.git,
            source.message.as_bytes(),
            &receipt.identity,
        ) {
            Ok(()) => println!("{}: original lint/attribution valid", source.source_oid),
            Err(error) => println!("{}: original findings: {error}", source.source_oid),
        }
    }
    if cfg.is_none() {
        println!(
            "Apply prerequisite missing: install guard explicitly, then export a new plan because installation changes the bound context; planning does not install it."
        );
    }
    println!(
        "Publication checks cover configured live destinations, not hidden refs or unknown forks."
    );
    Ok(())
}

fn bound(path: &Path, cfg: Option<&Config>) -> Result<(Proposal, Receipt)> {
    let tools = core::tools(cfg)?;
    let path = inspect::safe_path(path, &tools.git)?;
    let proposal: Proposal = storage::read(&path, false)?;
    if !valid_id(&proposal.plan_id) {
        return Err("invalid plan ID".into());
    }
    let common = core::git_text(&tools.git, &["rev-parse", "--git-common-dir"])?;
    let common = inspect::safe_path(Path::new(&common), &tools.git)?;
    // Preview/load do not create storage or silently regenerate missing receipts.
    let root = common.join("commitguard-fix");
    let receipt: Receipt = storage::read(&receipt_path(&root, &proposal.plan_id), true)?;
    if storage::hash(&receipt)? != proposal.plan_id {
        return Err("immutable source receipt SHA256 plan ID mismatch".into());
    }
    if receipt.schema_version != 2 || receipt.policy_version != 2 {
        return Err("unsupported repair receipt schema or policy version".into());
    }
    if receipt.common_dir != common || receipt.proposal_path != path {
        return Err("proposal path or repository differs from immutable receipt".into());
    }
    if proposal.candidates.len() != receipt.sources.len() || proposal.candidates.is_empty() {
        return Err("proposal must contain the exact complete ordered source list".into());
    }
    let mut seen = std::collections::BTreeSet::new();
    for (candidate, source) in proposal.candidates.iter().zip(&receipt.sources) {
        if candidate.source_oid != source.source_oid
            || !core::oid_valid(&candidate.source_oid)
            || !seen.insert(&candidate.source_oid)
        {
            return Err("proposal has duplicated, reordered, changed or extra source OIDs".into());
        }
    }
    inspect::recheck(&receipt, cfg, None)?;
    Ok((proposal, receipt))
}

/// Exact standardized credit blocks, including their folded physical lines.
/// Preserve all such source blocks, not just ones the linter currently accepts.
fn credits(message: &str) -> Vec<String> {
    let lines: Vec<_> = message.split('\n').collect();
    let mut credits = Vec::new();
    for (index, line) in lines.iter().enumerate() {
        let Some((key, _)) = line.trim_start().split_once(':') else {
            continue;
        };
        if !["co-authored-by", "signed-off-by"].contains(&key.trim().to_ascii_lowercase().as_str())
        {
            continue;
        }
        let mut block = (*line).to_string();
        for next in &lines[index + 1..] {
            if next.is_empty() || !next.starts_with([' ', '\t']) {
                break;
            }
            block.push('\n');
            block.push_str(next);
        }
        credits.push(block);
    }
    credits
}
fn preserve_credits(source: &str, candidate: &str) -> Result<()> {
    let mut remaining = credits(candidate);
    for credit in credits(source) {
        if let Some(index) = remaining.iter().position(|value| value == &credit) {
            remaining.remove(index);
        } else {
            return Err(
                "candidate deleted or changed exact source attribution (including folded lines)"
                    .into(),
            );
        }
    }
    Ok(())
}
fn normalize(git: &Path, message: &str) -> Result<String> {
    if message.contains('\r') {
        return Err("commit messages require LF line endings".into());
    }
    let mut before = String::new();
    for line in message.split_inclusive('\n') {
        if line.trim_end_matches('\n') == "# ------------------------ >8 ------------------------" {
            break;
        }
        before.push_str(line);
    }
    let bytes = core::query(
        git,
        &[
            "-c",
            "core.commentChar=#",
            "-c",
            "core.commentString=#",
            "stripspace",
            "--strip-comments",
        ]
        .map(str::to_string),
        Some(before.as_bytes()),
    )?;
    let message = core::text(&bytes)?;
    // Git stripspace supplies one final LF for nonempty messages; explicitly
    // normalize it so preview confirmation is independent of JSON final LFs.
    let trimmed = message.trim_end_matches('\n');
    Ok(if trimmed.is_empty() {
        String::new()
    } else {
        format!("{trimmed}\n")
    })
}
fn prepared(
    proposal: &Proposal,
    receipt: &Receipt,
    cfg: Option<&Config>,
) -> Result<(Vec<Candidate>, String, Vec<Vec<String>>)> {
    let tools = core::tools(cfg)?;
    let mut candidates = Vec::new();
    let mut all_findings = Vec::new();
    for (candidate, source) in proposal.candidates.iter().zip(&receipt.sources) {
        let mut findings = Vec::new();
        let message = match normalize(&tools.git, &candidate.message) {
            Ok(message) => message,
            Err(error) => {
                findings.push(error);
                // Invalid previews still show every source and the digest of
                // the displayed candidates, but cannot be applied.
                candidate.message.clone()
            }
        };
        if let Err(error) = preserve_credits(&source.message, &message) {
            findings.push(error);
        }
        if let Err(error) =
            core::validate_message_with_git(&tools.git, message.as_bytes(), &receipt.identity)
        {
            findings.push(error);
        }
        candidates.push(Candidate {
            source_oid: candidate.source_oid.clone(),
            message,
        });
        all_findings.push(findings);
    }
    let digest = storage::hash(&(proposal.plan_id.as_str(), &candidates))?;
    Ok((candidates, digest, all_findings))
}

pub(super) fn load(
    path: &Path,
    cfg: Option<&Config>,
) -> Result<(Proposal, Receipt, Vec<Candidate>, String)> {
    let (proposal, receipt) = bound(path, cfg)?;
    let (candidates, digest, findings) = prepared(&proposal, &receipt, cfg)?;
    if findings.iter().any(|items| !items.is_empty()) {
        let details = receipt
            .sources
            .iter()
            .zip(findings)
            .filter(|(_, items)| !items.is_empty())
            .map(|(source, items)| format!("{}: {}", source.source_oid, items.join("; ")))
            .collect::<Vec<_>>();
        return Err(format!(
            "invalid repair candidates: {}",
            details.join(" | ")
        ));
    }
    Ok((proposal, receipt, candidates, digest))
}

/// A completed operation can be inspected again without pretending its old
/// source tip is still current. Its requested content must remain identical.
pub(super) fn completed_candidates(
    proposal: &Proposal,
    receipt: &Receipt,
    cfg: &Config,
) -> Result<(Vec<Candidate>, String)> {
    if proposal.plan_id != storage::hash(receipt)?
        || proposal.candidates.len() != receipt.sources.len()
        || proposal
            .candidates
            .iter()
            .zip(&receipt.sources)
            .any(|(candidate, source)| candidate.source_oid != source.source_oid)
    {
        return Err("completed operation request has different source bindings".into());
    }
    let (candidates, digest, findings) = prepared(proposal, receipt, Some(cfg))?;
    if findings.iter().any(|items| !items.is_empty()) {
        return Err("completed operation request has invalid candidate content".into());
    }
    Ok((candidates, digest))
}
pub(super) fn preview(path: &Path, cfg: Option<&Config>) -> Result<()> {
    let (proposal, receipt) = bound(path, cfg)?;
    let (candidates, digest, findings) = prepared(&proposal, &receipt, cfg)?;
    println!("Operation: {}", receipt.operation);
    let destination = format!("{} <{}>", receipt.identity.login, receipt.identity.email);
    for ((source, candidate), items) in receipt.sources.iter().zip(&candidates).zip(&findings) {
        println!("Source OID: {}", source.source_oid);
        println!("Old Author: {}", source.author);
        println!("New Author: {destination} (original date/timezone retained)");
        println!("Old Committer: {}", source.committer);
        println!("New Committer: {destination} (operation date generated at apply)");
        println!("Normalized message: {:?}", candidate.message);
        if items.is_empty() {
            println!("Findings: valid");
        } else {
            for finding in items {
                println!("Finding: {finding}");
            }
        }
    }
    println!("Apply digest: {digest}");
    if cfg.is_none() {
        println!(
            "Apply prerequisite missing: install guard explicitly, then export a new plan because installation changes the bound context."
        );
    }
    if findings.iter().any(|items| !items.is_empty()) {
        return Err("invalid preview; candidates must pass every finding before apply".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn attribution_preserves_folded_bytes_and_multiplicity() {
        let source = "fix: source\n\nCo-Authored-By: Claude\n <noreply@anthropic.com>\n";
        assert!(preserve_credits(source, source).is_ok());
        assert!(
            preserve_credits(
                source,
                "fix: new\n\nCo-Authored-By: Claude <noreply@anthropic.com>\n"
            )
            .is_err()
        );
        assert!(preserve_credits(source, "fix: new\n").is_err());
        assert!(
            preserve_credits(
                "fix: a\nCo-authored-by: Codex\nCo-authored-by: Codex\n",
                "fix: a\nCo-authored-by: Codex\n"
            )
            .is_err()
        );
    }
    #[test]
    fn digest_binds_message_and_source_order() {
        let candidate = |oid: &str, message: &str| Candidate {
            source_oid: oid.into(),
            message: message.into(),
        };
        let first = vec![candidate("a", "fix: a\n"), candidate("b", "fix: b\n")];
        let second = vec![candidate("b", "fix: b\n"), candidate("a", "fix: a\n")];
        assert_ne!(
            storage::hash(&("id", first)).unwrap(),
            storage::hash(&("id", second)).unwrap()
        );
    }
}
