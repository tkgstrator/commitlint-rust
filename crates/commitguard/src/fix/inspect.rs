//! Read-only, fail-closed inspection of the source repository.
use super::{OwnedSource, Receipt, Source};
use crate::{Config, Result, core, util};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    env, fs,
    io::Read,
    path::{Component, Path, PathBuf},
    time::Duration,
};

const MAX_OWNERSHIP: u64 = 1024 * 1024;
const MAX_SOURCE: usize = 16 * 1024 * 1024;

fn unix() -> Result<()> {
    if !cfg!(unix) {
        return Err("commitguard fix requires Unix".into());
    }
    Ok(())
}

fn absolute(path: &Path) -> Result<PathBuf> {
    path.canonicalize()
        .map_err(|_| "cannot resolve repository path".into())
}

fn dirs(git: &Path) -> Result<(PathBuf, PathBuf, PathBuf)> {
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
struct Worktree {
    path: PathBuf,
    head: String,
    branch: Option<String>,
    record: String,
}

fn worktrees(git: &Path) -> Result<Vec<Worktree>> {
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

/// Resolve the existing parent after rejecting user-controlled symlinks, then
/// bind the real absolute destination without following a final file symlink.
/// All worktree checks use this resolved destination.
pub(super) fn safe_path(path: &Path, git: &Path) -> Result<PathBuf> {
    unix()?;
    let path = if path.is_absolute() {
        path.to_path_buf()
    } else {
        env::current_dir()
            .map_err(|_| "cannot locate current directory")?
            .join(path)
    };
    reject_symlinks(&path)?;
    let name = path.file_name().ok_or("file path required")?;
    let parent = absolute(path.parent().ok_or("file parent required")?)?;
    let resolved = parent.join(name);
    reject_symlinks(&resolved)?;
    let (common, _, _) = dirs(git)?;
    if !resolved.starts_with(&common) {
        for worktree in worktrees(git)? {
            if resolved.starts_with(worktree.path) {
                return Err("proposal/ownership files must be outside every worktree or under the common Git directory".into());
            }
        }
    }
    Ok(resolved)
}

/// macOS exposes these root-owned aliases on its protected root filesystem.
/// Accept only their exact system targets; aliases elsewhere are never trusted.
fn trusted_system_alias(path: &Path, metadata: &fs::Metadata) -> bool {
    #[cfg(target_os = "macos")]
    {
        use std::os::unix::fs::MetadataExt;
        let target = match path.to_str() {
            Some("/var") => Path::new("/private/var"),
            Some("/tmp") => Path::new("/private/tmp"),
            Some("/etc") => Path::new("/private/etc"),
            _ => return false,
        };
        if metadata.uid() != 0 {
            return false;
        }
        // These directories control both the alias entry and its destination.
        // /private/tmp itself is intentionally writable and is not an owner
        // of either root-level directory entry.
        for parent in [Path::new("/"), Path::new("/private")] {
            let Ok(m) = fs::symlink_metadata(parent) else {
                return false;
            };
            if !m.is_dir() || m.uid() != 0 || m.mode() & 0o022 != 0 {
                return false;
            }
        }
        let Ok(link) = fs::read_link(path) else {
            return false;
        };
        let link = if link.is_absolute() {
            link
        } else {
            Path::new("/").join(link)
        };
        link == target && fs::symlink_metadata(target).is_ok_and(|m| m.is_dir() && m.uid() == 0)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (path, metadata);
        false
    }
}

fn reject_symlinks(path: &Path) -> Result<()> {
    let mut current = PathBuf::new();
    for part in path.components() {
        match part {
            Component::ParentDir => {
                return Err("parent traversal is unsupported in repair paths".into());
            }
            Component::CurDir => continue,
            _ => current.push(part.as_os_str()),
        }
        match fs::symlink_metadata(&current) {
            Ok(metadata)
                if metadata.file_type().is_symlink()
                    && !trusted_system_alias(&current, &metadata) =>
            {
                return Err("symlink repair path is forbidden".into());
            }
            Ok(_) => (),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
            Err(_) => return Err("cannot inspect repair path".into()),
        }
    }
    Ok(())
}

fn ownership_file(path: &Path, git: &Path) -> Result<Vec<OwnedSource>> {
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

/// Parse complete raw bytes; do not normalize names, messages or source dates.
pub(super) fn source(git: &Path, oid: &str) -> Result<Source> {
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

fn ancestry(git: &Path, source: &str, tip: &str) -> Result<bool> {
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

fn repository_safety(git: &Path, common: &Path, git_dir: &Path) -> Result<()> {
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

fn status(git: &Path, root: &Path) -> Result<Vec<u8>> {
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

fn destinations(git: &Path) -> Result<Vec<String>> {
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

fn ref_commit(git: &Path, oid: &str) -> Result<Option<String>> {
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

fn unpublished(git: &Path, urls: &[String], sources: &[Source]) -> Result<()> {
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

fn original_hooks(
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

fn feed(hash: &mut Sha256, bytes: &[u8]) {
    hash.update((bytes.len() as u64).to_be_bytes());
    hash.update(bytes);
}

fn file_fingerprint(hash: &mut Sha256, path: &Path, required: bool) -> Result<()> {
    feed(
        hash,
        path.to_str()
            .ok_or("non-UTF-8 fingerprint path")?
            .as_bytes(),
    );
    match fs::metadata(path) {
        Ok(metadata) => {
            if !metadata.is_file() {
                return Err("fingerprinted executable/config is not a regular file".into());
            }
            let canonical = absolute(path)?;
            feed(
                hash,
                canonical
                    .to_str()
                    .ok_or("non-UTF-8 executable path")?
                    .as_bytes(),
            );
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                feed(hash, &metadata.permissions().mode().to_be_bytes());
            }
            let mut file =
                fs::File::open(path).map_err(|_| "cannot fingerprint executable/config")?;
            let mut digest = Sha256::new();
            let mut buffer = [0u8; 65536];
            loop {
                let n = file
                    .read(&mut buffer)
                    .map_err(|_| "cannot fingerprint executable/config")?;
                if n == 0 {
                    break;
                }
                digest.update(&buffer[..n]);
            }
            feed(hash, &digest.finalize());
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound && !required => feed(hash, b"absent"),
        Err(_) => return Err("required executable/config fingerprint is unavailable".into()),
    }
    Ok(())
}

fn verifier_fingerprint(hash: &mut Sha256, program: &str) -> Result<()> {
    // Unsigned repair never invokes these optional signature verifiers. Bind
    // their configured value without requiring PATH resolution or installation.
    feed(hash, program.as_bytes());
    let path = Path::new(program);
    if path.is_absolute() && path.is_file() && util::executable(path) {
        file_fingerprint(hash, path, true)
    } else {
        feed(hash, b"optional unused signature verifier unavailable");
        Ok(())
    }
}

fn hooks_fingerprint(hash: &mut Sha256, directory: &Path) -> Result<()> {
    feed(
        hash,
        directory
            .to_str()
            .ok_or("non-UTF-8 hook directory")?
            .as_bytes(),
    );
    if !directory.exists() {
        feed(hash, b"absent hooks");
        return Ok(());
    }
    let mut files = Vec::new();
    for item in fs::read_dir(directory).map_err(|_| "cannot inspect hook chain")? {
        let item = item.map_err(|_| "cannot inspect hook chain")?;
        let path = item.path();
        // Git hook entry points are regular executable files. Hash helper files
        // too when regular, since hook scripts may source them.
        if path.is_file() {
            files.push(path);
        } else if fs::symlink_metadata(&path)
            .map_err(|_| "cannot inspect hook entry")?
            .file_type()
            .is_symlink()
        {
            return Err("unresolvable hook-chain symlink".into());
        } else if path.is_dir() {
            files.push(path);
        } else {
            return Err("unsupported special file in hook chain".into());
        }
    }
    files.sort();
    for file in files {
        if file.is_dir() {
            hooks_fingerprint(hash, &file)?;
        } else {
            file_fingerprint(hash, &file, true)?;
        }
    }
    Ok(())
}

fn shared_state(
    git: &Path,
    r: &Receipt,
    staging: Option<&Path>,
    backup: Option<&str>,
    promoted_tip: Option<&str>,
) -> Result<Vec<u8>> {
    let mut state = Vec::new();
    let expected_tip = promoted_tip.unwrap_or(&r.tip);
    let mut branch_found = false;
    let refs = core::query(
        git,
        &[
            "for-each-ref".into(),
            "--format=%(refname) %(objectname)".into(),
        ],
        None,
    )?;
    for line in core::text(&refs)?.lines() {
        let (reference, oid) = line.split_once(' ').ok_or("malformed shared ref")?;
        if !core::oid_valid(oid) {
            return Err("malformed shared ref OID".into());
        }
        if Some(reference) == backup {
            if oid != r.tip {
                return Err("operation backup no longer matches source tip".into());
            }
            continue;
        }
        if reference == r.branch {
            if oid != expected_tip {
                return Err("target branch tip changed".into());
            }
            branch_found = true;
            // Only the approved branch movement is excluded from the frozen
            // shared-state digest. All other refs retain their actual bytes.
            state.extend_from_slice(format!("{} {}", r.branch, r.tip).as_bytes());
            state.push(0);
            continue;
        } else {
            if let Some(tip) = ref_commit(git, oid)? {
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
                for source in &r.sources {
                    if ancestry(git, &source.source_oid, &tip)? {
                        return Err("selected source is reachable from another shared ref (including stash/backup)".into());
                    }
                }
            }
        }
        state.extend_from_slice(line.as_bytes());
        state.push(0);
    }
    if !branch_found {
        return Err("target branch disappeared".into());
    }
    let mut trees = worktrees(git)?;
    trees.sort_by(|a, b| a.path.cmp(&b.path));
    let mut found = false;
    for tree in trees {
        if staging == Some(tree.path.as_path()) {
            if tree.branch.is_some() {
                return Err("operation staging worktree must be detached".into());
            }
            continue;
        }
        if tree.path == r.source_root {
            if tree.head != expected_tip || tree.branch.as_deref() != Some(r.branch.as_str()) {
                return Err("source worktree branch/tip changed".into());
            }
            found = true;
        } else {
            if tree.branch.as_deref() == Some(r.branch.as_str()) {
                return Err("another worktree is attached to the source branch".into());
            }
            for source in &r.sources {
                if ancestry(git, &source.source_oid, &tree.head)? {
                    return Err("selected source is reachable from another worktree HEAD".into());
                }
            }
        }
        if tree.path == r.source_root && promoted_tip.is_some() {
            // Normalize the single parsed HEAD field, never arbitrary OID text
            // in paths, other worktrees, or any additional record fields.
            let normalized = tree
                .record
                .split('\0')
                .map(|field| {
                    if field.strip_prefix("HEAD ") == Some(expected_tip) {
                        format!("HEAD {}", r.tip)
                    } else {
                        field.to_owned()
                    }
                })
                .collect::<Vec<_>>()
                .join("\0");
            state.extend_from_slice(normalized.as_bytes());
        } else {
            state.extend_from_slice(tree.record.as_bytes());
        }
        state.push(0);
        let cleanliness = status(git, &tree.path)?;
        if !cleanliness.is_empty() {
            return Err("source or linked worktree index/files are dirty".into());
        }
        state.extend_from_slice(&cleanliness);
        state.push(0);
    }
    if !found {
        return Err("original source worktree disappeared".into());
    }
    Ok(state)
}

fn fingerprint(
    git: &Path,
    cfg: Option<&Config>,
    r: &Receipt,
    staging: Option<&Path>,
    backup: Option<&str>,
    promoted_tip: Option<&str>,
) -> Result<String> {
    let mut hash = Sha256::new();
    feed(&mut hash, b"commitguard fix inspection v1");
    let effective = core::query(
        git,
        &[
            "config".into(),
            "--null".into(),
            "--show-origin".into(),
            "--list".into(),
        ],
        None,
    )?;
    feed(&mut hash, &effective);
    // Effective values alone do not detect changes to comments/includes that
    // are currently shadowed; freeze every file Git reports as an origin too.
    let fields: Vec<_> = core::text(&effective)?.split('\0').collect();
    let mut files = BTreeSet::new();
    for pair in fields.chunks(2) {
        if let Some(origin) = pair.first().and_then(|s| s.strip_prefix("file:")) {
            files.insert(PathBuf::from(origin));
        }
    }
    for path in files {
        file_fingerprint(&mut hash, &path, true)?;
    }
    file_fingerprint(&mut hash, git, true)?;
    file_fingerprint(&mut hash, &core::tools(cfg)?.gh, true)?;
    file_fingerprint(
        &mut hash,
        &env::current_exe().map_err(|_| "cannot locate active guard executable")?,
        true,
    )?;
    if let Some(cfg) = cfg {
        feed(
            &mut hash,
            &serde_json::to_vec(cfg)
                .map_err(|_| "cannot serialize effective guard configuration")?,
        );
        file_fingerprint(&mut hash, &cfg.root.join("config.json"), false)?;
        let mut arguments = env::args_os().skip(1);
        if arguments.next().as_deref() == Some(std::ffi::OsStr::new("--config")) {
            let explicit = PathBuf::from(
                arguments
                    .next()
                    .ok_or("missing explicit guard config path")?,
            );
            file_fingerprint(&mut hash, &absolute(&explicit)?, true)?;
        }
        file_fingerprint(&mut hash, &cfg.cli(), true)?;
        file_fingerprint(&mut hash, &cfg.canonical_cli(), true)?;
        file_fingerprint(&mut hash, &cfg.root.join("bin/git"), true)?;
        for kind in ["openpgp", "ssh", "x509"] {
            file_fingerprint(&mut hash, &cfg.root.join(format!("sign-{kind}")), false)?;
        }
        for program in cfg.verify_programs.values() {
            verifier_fingerprint(&mut hash, program)?;
        }
        hooks_fingerprint(&mut hash, &cfg.hooks())?;
    } else {
        feed(
            &mut hash,
            b"portable: apply requires an explicitly installed guard",
        );
    }
    hooks_fingerprint(&mut hash, &r.hooks_dir)?;
    feed(
        &mut hash,
        &shared_state(git, r, staging, backup, promoted_tip)?,
    );
    // Relevant environment is part of effective policy and checkout behavior;
    // never store/log values (which can include credentials), only the digest.
    let mut environment: Vec<_> = env::vars_os()
        .filter(|(k, _)| {
            let k = k.to_string_lossy();
            k.starts_with("GIT_") || k == "HOME" || k == "XDG_CONFIG_HOME" || k == "PATH"
        })
        .collect();
    environment.sort_by(|a, b| a.0.cmp(&b.0));
    for (key, value) in environment {
        feed(
            &mut hash,
            key.to_str().ok_or("non-UTF-8 Git environment")?.as_bytes(),
        );
        feed(
            &mut hash,
            value
                .to_str()
                .ok_or("non-UTF-8 Git environment")?
                .as_bytes(),
        );
    }
    Ok(format!("{:x}", hash.finalize()))
}

fn ownership_gate(r: &Receipt, git: &Path) -> Result<()> {
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

pub(super) fn inspect(
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

pub(super) fn recheck(r: &Receipt, cfg: Option<&Config>, staging: Option<&Path>) -> Result<()> {
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
pub(super) fn postcheck(
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
        .join(format!("operation-{}", super::storage::hash(r)?));
    let journal = super::storage::read_journal(&journal_root)?;
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

fn operation_exemptions(
    r: &Receipt,
    staging: Option<&Path>,
    git: &Path,
) -> Result<(Option<PathBuf>, Option<String>)> {
    let base = r.common_dir.join("commitguard-fix");
    let id = super::storage::hash(r)?;
    let root = base.join(format!("operation-{id}"));
    let journal_path = root.join("journal.json");
    if fs::symlink_metadata(&journal_path).is_err() {
        if staging.is_some() {
            return Err("staging exemption requires an authenticated operation journal".into());
        }
        return Ok((None, None));
    }
    let journal = super::storage::read_journal(&root)?;
    let lock: String = super::storage::read(&base.join("lock"), true)?;
    if journal.plan_id != id
        || super::storage::hash(&journal.receipt)? != id
        || lock != id
        || journal.backup_ref != format!("refs/commitguard/backups/{id}")
        || journal.staging != root.join(format!("staging-{id}"))
        || journal.original_tip != r.tip
    {
        return Err("operation journal does not authorize these backup/staging exemptions".into());
    }
    // Prepared is the only phase before a backup has been created. Once an
    // operation mutates history, absence of its recovery ref is never exempt.
    if journal.phase == "prepared" {
        if staging.is_some() {
            return Err("prepared operation cannot authorize staging".into());
        }
        return Ok((None, None));
    }
    if core::git_text(
        git,
        &["show-ref", "--verify", "--hash", &journal.backup_ref],
    )
    .map_err(|_| "operation backup is missing".to_string())?
        != r.tip
    {
        return Err("operation backup no longer matches source tip".into());
    }
    let staging = if let Some(path) = staging {
        reject_symlinks(path)?;
        let path = absolute(path)?;
        if path != journal.staging
            || journal.staging_git_dir.is_none()
            || ![
                "staged",
                "verified",
                "promotion-intent",
                "promoted",
                "cleanup-intent",
            ]
            .contains(&journal.phase.as_str())
        {
            return Err("staging exemption does not match the recorded active operation".into());
        }
        let recorded = journal.staging_git_dir.as_ref().unwrap();
        let bytes = core::query(
            git,
            &[
                "-C".into(),
                path.to_str().ok_or("non-UTF-8 staging path")?.into(),
                "rev-parse".into(),
                "--absolute-git-dir".into(),
            ],
            None,
        )?;
        if absolute(Path::new(core::text(&bytes)?.trim()))? != *recorded {
            return Err("staging Git directory changed since journal registration".into());
        }
        Some(path)
    } else {
        None
    };
    Ok((staging, Some(journal.backup_ref)))
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::fs::{PermissionsExt, symlink};

    struct TestDirectory(PathBuf);
    impl TestDirectory {
        fn new() -> Self {
            static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
            let nonce = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let unique = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let path = env::temp_dir().join(format!(
                "commitguard-inspect-{}-{nonce}-{unique}",
                std::process::id()
            ));
            fs::create_dir(&path).unwrap();
            Self(path)
        }
    }
    impl Drop for TestDirectory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn repair_paths_reject_user_symlink_parents_and_leaves() {
        let root = TestDirectory::new();
        fs::create_dir(root.0.join("real")).unwrap();
        symlink(root.0.join("real"), root.0.join("alias")).unwrap();
        assert!(reject_symlinks(&root.0.join("alias/new.json")).is_err());
        fs::write(root.0.join("real/source.json"), b"{}").unwrap();
        symlink(root.0.join("real/source.json"), root.0.join("leaf.json")).unwrap();
        assert!(reject_symlinks(&root.0.join("leaf.json")).is_err());
        assert!(reject_symlinks(&root.0.join("real/new.json")).is_ok());
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn repair_paths_accept_only_protected_macos_system_aliases() {
        for alias in ["/var", "/tmp", "/etc"] {
            assert!(reject_symlinks(Path::new(alias)).is_ok(), "{alias}");
        }
        let root = TestDirectory::new();
        assert!(reject_symlinks(&root.0.join("new.json")).is_ok());
        // A user-owned alias to a trusted target still must not gain trust.
        symlink("/private/tmp", root.0.join("tmp")).unwrap();
        assert!(reject_symlinks(&root.0.join("tmp/new.json")).is_err());
    }

    #[test]
    fn optional_verifiers_do_not_add_unsigned_runtime_dependencies() {
        let root = TestDirectory::new();
        let digest = |program: &str| {
            let mut hash = Sha256::new();
            verifier_fingerprint(&mut hash, program).unwrap();
            hash.finalize()
        };
        for name in ["gpg", "ssh-keygen", "gpgsm"] {
            let _ = digest(name);
        }
        assert_ne!(digest("gpg"), digest("gpgsm"));
        let absent = root.0.join("absent-verifier");
        let _ = digest(absent.to_str().unwrap());
        let program = root.0.join("verifier");
        fs::write(&program, b"#!/bin/sh\nexit 0\n").unwrap();
        fs::set_permissions(&program, fs::Permissions::from_mode(0o700)).unwrap();
        let before = digest(program.to_str().unwrap());
        fs::write(&program, b"#!/bin/sh\nexit 1\n").unwrap();
        assert_ne!(before, digest(program.to_str().unwrap()));
    }
}
