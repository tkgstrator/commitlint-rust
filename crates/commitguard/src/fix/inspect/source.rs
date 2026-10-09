//! Exact raw commit parsing and source identity validation.
use crate::fix::Source;
use crate::{Result, core};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

const MAX_SOURCE: usize = 16 * 1024 * 1024;

/// Parse complete raw bytes; do not normalize names, messages or source dates.
pub(in crate::fix) fn source(git: &Path, oid: &str) -> Result<Source> {
    if !core::oid_valid(oid) {
        return Err("invalid source OID".into());
    }
    if core::git_text(git, &["cat-file", "-t", oid])? != "commit" {
        return Err("source OID must identify a raw commit object, not a peeled tag".into());
    }
    let size = core::git_text(git, &["cat-file", "-s", oid])?
        .parse::<usize>()
        .map_err(|_| "invalid source object size")?;
    if size > MAX_SOURCE {
        return Err("source object exceeds inspection size limit".into());
    }
    let bytes = core::query(git, &["cat-file".into(), "commit".into(), oid.into()], None)?;
    let raw = core::text(&bytes)?.to_string();
    let (headers, message) = raw
        .split_once("\n\n")
        .ok_or("malformed raw source commit")?;
    if headers.contains(['\r', '\0']) || message.contains('\0') {
        return Err("malformed raw source commit bytes".into());
    }
    let mut values = BTreeMap::new();
    let mut signatures = BTreeSet::new();
    let lines: Vec<_> = headers.split('\n').collect();
    let mut i = 0;
    while i < lines.len() {
        let (key, value) = lines[i].split_once(' ').ok_or("malformed source header")?;
        if ["gpgsig", "gpgsig-sha256"].contains(&key) {
            if !signatures.insert(key) {
                return Err("duplicate source signature header".into());
            }
            let kind = match value {
                "-----BEGIN PGP SIGNATURE-----" => "PGP SIGNATURE",
                "-----BEGIN SSH SIGNATURE-----" => "SSH SIGNATURE",
                "-----BEGIN SIGNED MESSAGE-----" => "SIGNED MESSAGE",
                _ => return Err("unsupported source signature format".into()),
            };
            let end = format!("-----END {kind}-----");
            i += 1;
            let mut body = 0;
            let mut ended = false;
            while i < lines.len() && lines[i].starts_with(' ') {
                let line = &lines[i][1..];
                if ended || line.chars().any(|c| c.is_control()) {
                    return Err("malformed multiline source signature".into());
                }
                if line == end {
                    ended = true;
                } else {
                    body += 1;
                }
                i += 1;
            }
            if !ended || body == 0 {
                return Err("incomplete multiline source signature".into());
            }
            continue;
        }
        if !["tree", "parent", "author", "committer"].contains(&key) {
            return Err("unsupported source commit header".into());
        }
        if values.insert(key, value).is_some() {
            return Err("duplicate source header or nonlinear source history".into());
        }
        i += 1;
    }
    let tree = *values.get("tree").ok_or("missing source tree")?;
    let parent = *values
        .get("parent")
        .ok_or("root source history is unsupported")?;
    if !core::oid_valid(tree)
        || !core::oid_valid(parent)
        || tree.len() != oid.len()
        || parent.len() != oid.len()
    {
        return Err("malformed source tree/parent identifier".into());
    }
    let author = *values.get("author").ok_or("missing source Author")?;
    let committer = *values.get("committer").ok_or("missing source Committer")?;
    validate_raw_ident(author)?;
    validate_raw_ident(committer)?;
    if core::git_text(git, &["cat-file", "-t", tree])? != "tree" {
        return Err("source tree is unavailable".into());
    }
    Ok(Source {
        source_oid: oid.into(),
        parent: parent.into(),
        tree: tree.into(),
        author: author.into(),
        committer: committer.into(),
        message: message.into(),
        raw,
    })
}

fn validate_raw_ident(value: &str) -> Result<()> {
    let (name_email, date) = value
        .rsplit_once(' ')
        .ok_or("malformed source identity date")?;
    let (ident, timestamp) = name_email
        .rsplit_once(' ')
        .ok_or("malformed source identity timestamp")?;
    if timestamp.parse::<i64>().is_err()
        || timestamp.is_empty()
        || !timestamp
            .strip_prefix('-')
            .unwrap_or(timestamp)
            .bytes()
            .all(|b| b.is_ascii_digit())
    {
        return Err("malformed source identity timestamp".into());
    }
    let zone = date.as_bytes();
    if zone.len() != 5
        || !b"+-".contains(&zone[0])
        || !zone[1..].iter().all(u8::is_ascii_digit)
        || date[1..3].parse::<u8>().unwrap_or(255) > 23
        || date[3..5].parse::<u8>().unwrap_or(255) > 59
    {
        return Err("malformed source identity timezone".into());
    }
    let (name, email) = ident
        .strip_suffix('>')
        .and_then(|s| s.rsplit_once(" <"))
        .ok_or("malformed source identity")?;
    if name.is_empty()
        || email.is_empty()
        || name.contains(['<', '>'])
        || email.contains(['<', '>'])
        || ident.chars().any(char::is_control)
    {
        return Err("malformed source identity".into());
    }
    Ok(())
}
