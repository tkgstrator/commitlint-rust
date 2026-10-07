//! Explicit, receipt-bound repair of an unpublished linear suffix.
mod apply;
mod inspect;
mod plan;
mod replay;
mod storage;

use crate::{Config, Identity, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(super) struct Source {
    pub source_oid: String,
    pub parent: String,
    pub tree: String,
    pub author: String,
    pub committer: String,
    pub message: String,
    pub raw: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(super) struct OwnedSource {
    pub source_oid: String,
    pub old_author: String,
    pub owned: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Receipt {
    pub schema_version: u32,
    pub policy_version: u32,
    pub operation: String,
    pub proposal_path: PathBuf,
    pub common_dir: PathBuf,
    pub git_dir: PathBuf,
    pub source_root: PathBuf,
    pub branch: String,
    pub object_format: String,
    pub tip: String,
    pub base: String,
    pub sources: Vec<Source>,
    pub identity: Identity,
    pub auth_context: String,
    pub ownership: Vec<OwnedSource>,
    pub hooks_dir: PathBuf,
    pub fingerprint: String,
    pub destinations: Vec<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Candidate {
    pub source_oid: String,
    pub message: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Proposal {
    pub plan_id: String,
    pub candidates: Vec<Candidate>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Mapping {
    pub source_oid: String,
    pub new_oid: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Journal {
    pub schema_version: u32,
    pub plan_id: String,
    pub apply_digest: String,
    pub receipt: Receipt,
    pub candidates: Vec<Candidate>,
    pub original_tip: String,
    pub backup_ref: String,
    pub staging: PathBuf,
    pub staging_git_dir: Option<PathBuf>,
    pub phase: String,
    pub editor_index: usize,
    pub committer_date: String,
    pub mapping: Vec<Mapping>,
    pub verified: Vec<Source>,
    pub failure: Option<String>,
}

pub fn run(args: &[String], cfg: Option<&Config>) -> Result<()> {
    if !cfg!(unix) {
        return Err("commitguard fix is unsupported on Windows; Unix is required".into());
    }
    // Source binaries may coordinate an already installed guard too. Keep this
    // discovery local to fix; all existing public checker/config behavior stays.
    let discovered = if cfg.is_none() {
        match std::env::var_os("HOME") {
            Some(home) => {
                let root = PathBuf::from(home).join(".local/share/gh-commit-identity/guard");
                let path = root.join("config.json");
                if std::fs::symlink_metadata(&path).is_ok() {
                    let installed = Config::read(&path)?;
                    if installed.root != root {
                        return Err("installed fix configuration root mismatch".into());
                    }
                    Some(installed)
                } else {
                    None
                }
            }
            None => None,
        }
    } else {
        None
    };
    let cfg = cfg.or(discovered.as_ref());
    if args.first().map(String::as_str) == Some("--private-editor") {
        return replay::editor(
            &args[1..],
            cfg.ok_or("private editor requires guard configuration")?,
        );
    }
    let mut options = std::collections::BTreeMap::new();
    let allowed = [
        "--range",
        "--plan",
        "--preview",
        "--apply",
        "--author",
        "--ownership",
        "--confirm-author-migration",
    ];
    let mut index = 0;
    while index < args.len() {
        let key = args[index].as_str();
        if !allowed.contains(&key) || options.contains_key(key) {
            return Err("unknown or duplicate fix option".into());
        }
        let value = args
            .get(index + 1)
            .filter(|v| !v.starts_with("--"))
            .ok_or("fix option requires a value")?;
        options.insert(key, value.as_str());
        index += 2;
    }
    if let Some(path) = options.get("--plan") {
        if options.contains_key("--preview")
            || options.contains_key("--apply")
            || options.contains_key("--confirm-author-migration")
        {
            return Err("conflicting fix operations".into());
        }
        let range = options.get("--range").ok_or("plan requires --range")?;
        let ownership = match (options.get("--author"), options.get("--ownership")) {
            (None, None) => None,
            (Some(&"gh"), Some(path)) => Some(Path::new(path)),
            _ => return Err("migration requires --author gh and --ownership FILE".into()),
        };
        return plan::export(range, Path::new(path), ownership, cfg);
    }
    if let Some(path) = options.get("--preview") {
        if options.len() != 1 {
            return Err("conflicting preview options".into());
        }
        return plan::preview(Path::new(path), cfg);
    }
    if let Some(path) = options.get("--apply") {
        if options
            .keys()
            .any(|k| !["--apply", "--confirm-author-migration"].contains(k))
        {
            return Err("conflicting apply options".into());
        }
        return apply::run(
            Path::new(path),
            options.get("--confirm-author-migration").copied(),
            cfg.ok_or("install guard explicitly before apply")?,
        );
    }
    Err("fix requires --range BASE..HEAD --plan FILE, --preview FILE, or --apply FILE".into())
}
