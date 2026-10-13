//! Managed agent-instruction block.
use super::{
    assets::NAME,
    paths::{io, mode},
    transaction::Transaction,
};
use crate::Result;
use std::{fs, path::Path};

pub(super) fn marked(original: &str, body: &str) -> Result<String> {
    let start = format!("<!-- {NAME} -->");
    let end = format!("<!-- /{NAME} -->");
    let block = format!("{start}\n{body}\n{end}");
    match (original.find(&start), original.find(&end)) {
        (Some(a), Some(b)) if b >= a => Ok(format!(
            "{}{}{}",
            &original[..a],
            block,
            &original[b + end.len()..]
        )),
        (None, None) => Ok(format!(
            "{}{}\n{}\n",
            original,
            if original.is_empty() || original.ends_with('\n') {
                ""
            } else {
                "\n"
            },
            block
        )),
        _ => Err("malformed managed instruction block".into()),
    }
}
pub(super) fn mandate(tx: &mut Transaction, path: &Path, skill: &Path) -> Result<()> {
    let original = if path.exists() {
        io(fs::read_to_string(path), "read agent instructions")?
    } else {
        String::new()
    };
    let body = format!(
        "Before creating, rewriting, signing, pushing, or delegating Git commits, read and apply gh-commit-identity at {}. Its native checker is mandatory for new or rewritten local commits: printable ASCII English Conventional Commits, 128 counted characters maximum excluding validated final Co-Authored-By lines and their otherwise empty separator; original ASCII and attribution checks cover the whole message and other trailers remain counted, and raw Author/Committer matching the verified human github.com gh identity. Normal checks use the credential-bound cache without API calls; missing or changed cache requires --strict account. Push and native fix apply always verify online without cache fallback. Preserve recognized AI attribution. Use installed guarded Git for ordinary commits and V1 fix. Only explicitly owner-approved existing-history bulk-write may skip per-commit hooks/API; it requires frozen candidates and complete mapping verification before operator-controlled backup/CAS promotion. Never use a global ignore for new commits. Automatically correct only the skill's proven own unpublished safe subset, preserving trees and backup refs, and recheck before one authorized normal push retry. Missing tools/authentication or uncertain refs stops; repairing public/foreign history requires separate authorization. Verify actual delegated new or rewritten local commits. GitHub-generated published merge/squash/rebase history has standing authorization for preservation: establish exact OIDs and origin from live github.com refs and merged-PR metadata, retain original identities/signatures/messages, and automatically skip agent correction and approval requests for those differences. Names, emails, subjects and parent counts alone do not prove origin. Do not use standalone commits checks on this inherited history to trigger repair; use the mandatory native push/pre-push checker on actual outgoing commits. This exception grants no rewrite, force-push, ownership or migration authority and never bypasses a failed native check.",
        skill.display()
    );
    let text = marked(&original, &body)?;
    tx.write(path, text.as_bytes(), mode(path))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn preserve_instructions() {
        let body = marked("Existing instructions.\n", "Policy").unwrap();
        assert_eq!(marked(&body, "Policy").unwrap(), body);
        assert!(body.starts_with("Existing instructions."));
        assert!(marked("<!-- gh-commit-identity -->", "Policy").is_err());
    }
}
