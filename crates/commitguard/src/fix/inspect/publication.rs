//! Live destination enumeration and unpublished ancestry checks.
use super::repository::ancestry;
use crate::fix::Source;
use crate::{Result, core, util};
use std::{collections::BTreeSet, path::Path, time::Duration};

pub(super) fn destinations(git: &Path) -> Result<Vec<String>> {
    let remotes = core::git_text(git, &["remote"])?;
    let mut urls = BTreeSet::new();
    for remote in remotes.lines() {
        if remote.is_empty() || remote.starts_with('-') {
            return Err("invalid remote name".into());
        }
        for push in [false, true] {
            let mut args = vec!["remote".into(), "get-url".into()];
            if push {
                args.push("--push".into());
            }
            args.extend(["--all".into(), remote.into()]);
            let bytes = core::query(git, &args, None)?;
            let values = core::text(&bytes)?;
            if values.lines().count() == 0 {
                return Err("remote destination is unresolved".into());
            }
            for url in values.lines() {
                if url.is_empty() || url.contains(['\r', '\0']) || url.starts_with('-') {
                    return Err("remote destination is unresolved".into());
                }
                safe_destination(url)?;
                urls.insert(url.into());
            }
        }
    }
    if urls.is_empty() {
        return Err("zero configured remotes cannot establish unpublished history".into());
    }
    Ok(urls.into_iter().collect())
}

fn safe_destination(url: &str) -> Result<()> {
    // Query parameters commonly carry tokens. Reject rather than copy them
    // into a receipt, even when a transport would otherwise accept them.
    if url.contains('?') {
        return Err("remote URL query parameters are unsupported; use gh authentication".into());
    }
    if let Some((scheme, rest)) = url.split_once("://") {
        let authority = rest.split(['/', '#']).next().unwrap_or_default();
        if let Some((userinfo, _)) = authority.rsplit_once('@')
            && (matches!(scheme.to_ascii_lowercase().as_str(), "http" | "https")
                || userinfo.contains(':')
                || userinfo.to_ascii_lowercase().contains("%3a"))
        {
            return Err(
                "credential-bearing remote URLs are unsupported; use gh authentication".into(),
            );
        }
    }
    Ok(())
}

#[cfg(test)]
#[test]
fn remote_destination_validation_never_accepts_passwords_or_query_tokens() {
    for url in [
        "https://token@github.com/repo",
        "ssh://git:secret@host/repo",
        "custom://git:secret@host/repo",
        "ssh://git%3Asecret@host/repo",
        "https://github.com/repo?token=secret",
        "ssh://git@host/repo?token=secret",
    ] {
        let error = safe_destination(url).unwrap_err();
        assert!(!error.contains("secret"));
        assert!(!error.contains(url));
    }
    for url in [
        "ssh://git@host/repo",
        "git@host:repo",
        "https://github.com/repo",
        "/private/local remote.git",
    ] {
        assert!(safe_destination(url).is_ok(), "{url}");
    }
}

pub(super) fn ref_commit(git: &Path, oid: &str) -> Result<Option<String>> {
    let mut kind = core::git_text(git, &["cat-file", "-t", oid])
        .map_err(|_| "unknown ref object; fetch complete history before repair")?;
    if kind == "tag" {
        kind = core::git_text(git, &["cat-file", "-t", &format!("{oid}^{{}}")])
            .map_err(|_| "unknown annotated tag target; fetch complete history before repair")?;
    }
    match kind.as_str() {
        "commit" => Ok(Some(
            core::resolve_commit(git, oid)?.ok_or("ref commit ancestry is unavailable")?,
        )),
        "tree" | "blob" => Ok(None),
        _ => Err("unsupported advertised/shared ref object".into()),
    }
}

pub(super) fn unpublished(git: &Path, urls: &[String], sources: &[Source]) -> Result<()> {
    for url in urls {
        // Include HEAD and peeled advertisements, not just refs/heads. We
        // independently peel locally, so absent unknown tag objects fail closed.
        let output = util::capture_env(
            git,
            &["ls-remote".into(), "--".into(), url.clone()],
            None,
            Duration::from_secs(60),
            &[
                ("GIT_TERMINAL_PROMPT", Some("0")),
                ("GCM_INTERACTIVE", Some("Never")),
            ],
        )?;
        if output.code != 0 {
            return Err(
                "cannot inspect live remote refs without interactive authentication".into(),
            );
        }
        let bytes = output.stdout;
        let mut tips = BTreeSet::new();
        for line in core::text(&bytes)?.lines() {
            let (oid, reference) = line
                .split_once('\t')
                .ok_or("malformed live ref advertisement")?;
            if !core::oid_valid(oid)
                || reference.is_empty()
                || reference.chars().any(char::is_whitespace)
            {
                return Err("malformed live ref advertisement".into());
            }
            if reference != "HEAD" {
                let reference = reference.strip_suffix("^{}").unwrap_or(reference);
                if !reference.starts_with("refs/")
                    || util::capture(
                        git,
                        &["check-ref-format".into(), reference.into()],
                        None,
                        Duration::from_secs(15),
                    )?
                    .code
                        != 0
                {
                    return Err("malformed advertised ref name".into());
                }
            }
            if let Some(tip) = ref_commit(git, oid)? {
                tips.insert(tip);
            }
        }
        for tip in tips {
            // Traverse the entire advertised ancestry even if merge-base could
            // answer early, to refuse corrupt/incomplete ancestry consistently.
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
            for source in sources {
                if ancestry(git, &source.source_oid, &tip)? {
                    return Err("selected source is published at a live destination".into());
                }
            }
        }
    }
    Ok(())
}
