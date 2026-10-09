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
        "Before creating, rewriting, signing, pushing, or delegating Git commits, read and apply gh-commit-identity at {}. Its native checker is mandatory: printable ASCII English Conventional Commits, 128 entire-message characters maximum, and raw Author/Committer matching the verified human github.com gh identity. Normal checks use the credential-bound cache without API calls; missing or changed cache requires --strict account. Push and native fix apply always verify online without cache fallback. Preserve recognized AI attribution. Use installed guarded Git for ordinary commits and V1 fix. Only explicitly owner-approved existing-history bulk-write may skip per-commit hooks/API; it requires frozen candidates and complete mapping verification before operator-controlled backup/CAS promotion. Never use a global ignore for new commits. Automatically correct only the skill's proven own unpublished safe subset, preserving trees and backup refs, and recheck before one authorized normal push retry. Missing tools/authentication or uncertain/public/foreign history stops. Verify actual delegated commits.",
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
