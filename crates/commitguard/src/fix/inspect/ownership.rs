//! Bounded ownership declarations and exact mistaken-Author authorization.
use super::paths::safe_path;
use crate::fix::{OwnedSource, Receipt};
use crate::{Result, core};
use serde::Deserialize;
use std::{collections::BTreeMap, fs, io::Read, path::Path};

const MAX_OWNERSHIP: u64 = 1024 * 1024;

pub(super) fn ownership_file(path: &Path, git: &Path) -> Result<Vec<OwnedSource>> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Ownership {
        sources: Vec<OwnedSource>,
    }
    let path = safe_path(path, git)?;
    let mut options = fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    let file = options
        .open(path)
        .map_err(|_| "cannot safely read ownership declaration")?;
    let metadata = file
        .metadata()
        .map_err(|_| "cannot inspect ownership declaration")?;
    if !metadata.is_file() || metadata.len() > MAX_OWNERSHIP {
        return Err("ownership declaration is not a bounded regular file".into());
    }
    let mut bytes = Vec::new();
    file.take(MAX_OWNERSHIP + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "cannot read ownership declaration")?;
    if bytes.len() as u64 > MAX_OWNERSHIP {
        return Err("ownership declaration is too large".into());
    }
    let parsed: Ownership =
        serde_json::from_slice(&bytes).map_err(|_| "invalid ownership declaration")?;
    Ok(parsed.sources)
}

pub(super) fn ownership_gate(r: &Receipt, git: &Path) -> Result<()> {
    let mut declarations = BTreeMap::new();
    for owned in &r.ownership {
        if !owned.owned
            || !core::oid_valid(&owned.source_oid)
            || declarations
                .insert(owned.source_oid.as_str(), owned)
                .is_some()
        {
            return Err("invalid or duplicate ownership declaration".into());
        }
        let source = r
            .sources
            .iter()
            .find(|s| s.source_oid == owned.source_oid)
            .ok_or("ownership declaration names an unselected source")?;
        if source.author != owned.old_author {
            return Err("ownership declaration old Author differs from exact source bytes".into());
        }
        if core::check_ident(&source.author, &r.identity, "Author").is_ok() {
            return Err(
                "ownership declaration must identify a mistaken noncanonical Author".into(),
            );
        }
    }
    for source in &r.sources {
        if core::check_ident(&source.author, &r.identity, "Author").is_err()
            && (r.operation != "author-migration"
                || !declarations.contains_key(source.source_oid.as_str()))
        {
            return Err("source Author must match verified gh identity; migration requires an exact per-OID ownership declaration".into());
        }
        core::validate_trailers_with_git(git, source.message.as_bytes(), &r.identity)?;
    }
    if r.operation == "repair" && !r.ownership.is_empty() {
        return Err("repair receipt cannot authorize Author migration".into());
    }
    if r.operation == "author-migration" && r.ownership.is_empty() {
        return Err("migration requires mistaken-Author ownership declarations".into());
    }
    Ok(())
}
