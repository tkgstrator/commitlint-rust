//! Repository layout, cleanliness, safety, and original hook resolution.
use super::paths::absolute;
use crate::{Config, Result, core, util};
use std::{
    env, fs,
    path::{Path, PathBuf},
    time::Duration,
};

pub(super) fn dirs(git: &Path) -> Result<(PathBuf, PathBuf, PathBuf)> {
    if core::git_text(git, &["rev-parse", "--is-bare-repository"])? != "false" {
        return Err("fix requires a non-bare source worktree".into());
    }
    Ok((
        absolute(Path::new(&core::git_text(
            git,
            &["rev-parse", "--git-common-dir"],
        )?))?,
        absolute(Path::new(&core::git_text(
            git,
            &["rev-parse", "--absolute-git-dir"],
        )?))?,
        absolute(Path::new(&core::git_text(
            git,
            &["rev-parse", "--show-toplevel"],
        )?))?,
    ))
}

#[derive(Debug)]
pub(super) struct Worktree {
    pub(super) path: PathBuf,
    pub(super) head: String,
    pub(super) branch: Option<String>,
    pub(super) record: String,
}

pub(super) fn worktrees(git: &Path) -> Result<Vec<Worktree>> {
    let bytes = core::query(
        git,
        &[
            "worktree".into(),
            "list".into(),
            "--porcelain".into(),
            "-z".into(),
        ],
        None,
    )?;
    let raw = core::text(&bytes)?;
    let mut result = Vec::new();
    for record in raw.split("\0\0").filter(|s| !s.is_empty()) {
        let fields: Vec<_> = record.trim_end_matches('\0').split('\0').collect();
        let path = fields
            .first()
            .and_then(|s| s.strip_prefix("worktree "))
            .ok_or("malformed worktree enumeration")?;
        let path = absolute(Path::new(path))?;
        let mut head = None;
        let mut branch = None;
        for field in &fields[1..] {
            if let Some(value) = field.strip_prefix("HEAD ") {
                if head.replace(value.to_string()).is_some() || !core::oid_valid(value) {
                    return Err("invalid worktree HEAD".into());
                }
            } else if let Some(value) = field.strip_prefix("branch ") {
                if branch.replace(value.to_string()).is_some() {
                    return Err("duplicate worktree branch".into());
                }
            } else if *field != "detached"
                && *field != "bare"
                && !field.starts_with("locked")
                && !field.starts_with("prunable")
            {
                return Err("unsupported worktree record".into());
            }
        }
        result.push(Worktree {
            path,
            head: head.ok_or("unborn or bare worktree is unsupported")?,
            branch,
            record: record.into(),
        });
    }
    if result.is_empty() {
        return Err("cannot enumerate source worktrees".into());
    }
    Ok(result)
}

pub(super) fn ancestry(git: &Path, source: &str, tip: &str) -> Result<bool> {
    let output = util::capture(
        git,
        &[
            "merge-base".into(),
            "--is-ancestor".into(),
            source.into(),
            tip.into(),
        ],
        None,
        Duration::from_secs(60),
    )?;
    match output.code {
        0 => Ok(true),
        1 => Ok(false),
        _ => Err("cannot establish complete ancestry".into()),
    }
}

pub(super) fn repository_safety(git: &Path, common: &Path, git_dir: &Path) -> Result<()> {
    core::history_safety(git)?;
    if !core::git_text(
        git,
        &["for-each-ref", "--format=%(refname)", "refs/replace/"],
    )?
    .is_empty()
    {
        return Err("replace objects are unsupported for repair".into());
    }
    for directory in [common, git_dir] {
        for name in [
            "info/grafts",
            "shallow",
            "sequencer",
            "rebase-merge",
            "rebase-apply",
            "MERGE_HEAD",
            "CHERRY_PICK_HEAD",
            "REVERT_HEAD",
            "BISECT_START",
            "index.lock",
            "HEAD.lock",
        ] {
            if fs::symlink_metadata(directory.join(name)).is_ok() {
                return Err(
                    "incomplete history or concurrent repository operation prevents repair".into(),
                );
            }
        }
    }
    if [
        "GIT_REPLACE_REF_BASE",
        "GIT_SHALLOW_FILE",
        "GIT_GRAFT_FILE",
        "GIT_INDEX_FILE",
        "GIT_OBJECT_DIRECTORY",
        "GIT_ALTERNATE_OBJECT_DIRECTORIES",
        "GIT_DIR",
        "GIT_COMMON_DIR",
        "GIT_WORK_TREE",
        "GIT_NAMESPACE",
        "GIT_CONFIG_COUNT",
        "GIT_CONFIG_PARAMETERS",
        "GIT_CONFIG",
    ]
    .iter()
    .any(|key| env::var_os(key).is_some())
    {
        return Err("alternate history/index environment is unsupported for repair".into());
    }
    let config = core::query(
        git,
        &["config".into(), "--null".into(), "--list".into()],
        None,
    )?;
    for entry in core::text(&config)?.split('\0').filter(|s| !s.is_empty()) {
        let (key, value) = entry.split_once('\n').unwrap_or((entry, ""));
        let key = key.to_ascii_lowercase();
        if key == "extensions.partialclone"
            || key.ends_with(".partialclonefilter")
            || (key.ends_with(".promisor") && value != "false" && value != "no" && value != "0")
        {
            return Err("partial/promisor repositories are unsupported for repair".into());
        }
    }
    let pack = common.join("objects/pack");
    if pack.exists() {
        for entry in fs::read_dir(pack).map_err(|_| "cannot inspect object storage")? {
            let entry = entry.map_err(|_| "cannot inspect object storage")?;
            if entry.path().extension().is_some_and(|v| v == "promisor") {
                return Err("promisor object storage is unsupported for repair".into());
            }
        }
    }
    Ok(())
}

pub(super) fn status(git: &Path, root: &Path) -> Result<Vec<u8>> {
    let root = root.to_str().ok_or("non-UTF-8 worktree path")?;
    let entries = core::query(
        git,
        &[
            "--no-optional-locks".into(),
            "-c".into(),
            "core.fsmonitor=false".into(),
            "-c".into(),
            "core.untrackedCache=false".into(),
            "-C".into(),
            root.into(),
            "ls-files".into(),
            "-v".into(),
            "-z".into(),
        ],
        None,
    )?;
    for entry in entries.split(|b| *b == 0).filter(|entry| !entry.is_empty()) {
        if entry[0].is_ascii_lowercase() || entry[0] == b'S' {
            return Err("assume-unchanged/skip-worktree index entries cannot establish a clean repair worktree".into());
        }
    }
    core::query(
        git,
        &[
            "--no-optional-locks".into(),
            "-c".into(),
            "core.fsmonitor=false".into(),
            "-c".into(),
            "core.untrackedCache=false".into(),
            "-C".into(),
            root.into(),
            "status".into(),
            "--porcelain=v1".into(),
            "-z".into(),
            "--untracked-files=all".into(),
            "--ignore-submodules=none".into(),
        ],
        None,
    )
}

pub(super) fn original_hooks(
    git: &Path,
    cfg: Option<&Config>,
    common: &Path,
    git_dir: &Path,
    root: &Path,
) -> Result<PathBuf> {
    let output = util::capture(
        git,
        &[
            "config".into(),
            "--path".into(),
            "--get".into(),
            "core.hooksPath".into(),
        ],
        None,
        Duration::from_secs(15),
    )?;
    let configured = match output.code {
        0 => {
            let value = core::text(&output.stdout)?.trim_end_matches('\n');
            if value.is_empty() || value.contains(['\n', '\r', '\0']) {
                return Err("invalid effective hooksPath".into());
            }
            Some(PathBuf::from(value))
        }
        1 => None,
        _ => return Err("cannot resolve effective hooksPath".into()),
    };
    let guard = cfg.map(Config::hooks);
    let is_guard = configured
        .as_ref()
        .zip(guard.as_ref())
        .is_some_and(|(a, b)| {
            let a = if a.is_absolute() {
                a.clone()
            } else {
                root.join(a)
            };
            a.canonicalize().unwrap_or(a) == b.canonicalize().unwrap_or_else(|_| b.clone())
        });
    let mapping = cfg.and_then(|cfg| {
        cfg.repo_hooks
            .get(&git_dir.to_string_lossy().to_string())
            .or_else(|| cfg.repo_hooks.get(&common.to_string_lossy().to_string()))
            .map(PathBuf::from)
            .or_else(|| cfg.previous_hooks.clone())
    });
    let chosen = match configured {
        Some(configured) if !is_guard => configured,
        _ => mapping.unwrap_or_else(|| common.join("hooks")),
    };
    let chosen = if let Ok(rest) = chosen.strip_prefix("~") {
        PathBuf::from(env::var_os("HOME").ok_or("cannot expand original hooksPath")?).join(rest)
    } else {
        chosen
    };
    let chosen = if chosen.is_absolute() {
        chosen
    } else {
        root.join(chosen)
    };
    if chosen.exists() {
        if !chosen.is_dir() {
            return Err("original hooksPath is not a directory".into());
        }
        let chosen = absolute(&chosen)?;
        if let Some(guard) = guard
            && chosen == guard.canonicalize().unwrap_or(guard)
        {
            return Err("recursive original guard hook chain is unsupported".into());
        }
        Ok(chosen)
    } else {
        // The default hooks directory may legitimately be absent. Explicit
        // mappings must be resolvable; never silently fall back to another chain.
        if chosen != common.join("hooks") {
            return Err("unresolvable original hook chain".into());
        }
        Ok(chosen)
    }
}
