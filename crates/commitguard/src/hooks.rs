//! Enforce native hooks while preserving the original hooks and their stdin.
use crate::{Config, Identity, Result, core, util};
use std::{
    env, fs,
    io::{self, Read, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    thread,
    time::Duration,
};

fn absolute(path: &Path) -> Result<PathBuf> {
    let path = if path.is_absolute() {
        path.to_path_buf()
    } else {
        env::current_dir()
            .map_err(|_| "cannot resolve repository directory")?
            .join(path)
    };
    Ok(path.canonicalize().unwrap_or(path))
}
fn repository_dirs(cfg: &Config, name: &str) -> Result<Option<(PathBuf, PathBuf)>> {
    let common = core::git_text(&cfg.git, &["rev-parse", "--git-common-dir"]);
    let worktree = core::git_text(&cfg.git, &["rev-parse", "--git-dir"]);
    if let (Ok(common), Ok(worktree)) = (common, worktree) {
        return Ok(Some((
            absolute(Path::new(&common))?,
            absolute(Path::new(&worktree))?,
        )));
    }
    if name != "reference-transaction" {
        return Err("cannot locate repository hooks".into());
    }
    // git init fires reference-transaction before HEAD exists. Recover only
    // explicit/local incomplete Git directories; never suppress an actual hook.
    let cwd = env::current_dir().map_err(|_| "cannot find incomplete Git directory")?;
    let mut candidates = Vec::new();
    if let Some(explicit) = env::var_os("GIT_DIR") {
        candidates.push(PathBuf::from(explicit));
    }
    candidates.push(cwd.join(".git"));
    candidates.push(cwd);
    for candidate in candidates {
        if candidate.is_dir()
            && (candidate.join("hooks").is_dir() || candidate.join("refs").is_dir())
        {
            let common = absolute(&candidate)?;
            return Ok(Some((common.clone(), common)));
        }
    }
    Ok(None)
}
fn expand_home(path: &Path) -> PathBuf {
    if let Ok(rest) = path.strip_prefix("~") {
        if let Some(home) = env::var_os("HOME") {
            return PathBuf::from(home).join(rest);
        }
    }
    path.to_path_buf()
}
fn original(cfg: &Config, name: &str, args: &[String], payload: Option<&[u8]>) -> Result<i32> {
    let Some((common, worktree)) = repository_dirs(cfg, name)? else {
        return Ok(0);
    };
    let previous = env::var_os("GIT_IDENTITY_PREVIOUS_HOOKS_PATH")
        .map(PathBuf::from)
        .or_else(|| {
            cfg.repo_hooks
                .get(&worktree.to_string_lossy().to_string())
                .map(PathBuf::from)
        })
        .or_else(|| {
            cfg.repo_hooks
                .get(&common.to_string_lossy().to_string())
                .map(PathBuf::from)
        })
        .or_else(|| cfg.previous_hooks.clone());
    let folder = previous
        .map(|path| expand_home(&path))
        .filter(|path| path.is_dir())
        .unwrap_or_else(|| common.join("hooks"));
    let target = folder.join(name);
    if !target.is_file() || !util::executable(&target) {
        return Ok(0);
    }
    let target = absolute(&target)?;
    if target == absolute(&cfg.hooks().join(name))?
        || target == absolute(&cfg.cli())?
        || target == absolute(&cfg.canonical_cli())?
    {
        return Ok(0);
    }
    let mut command = Command::new(&target);
    command
        .args(args)
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());
    command.stdin(if payload.is_some() {
        Stdio::piped()
    } else {
        Stdio::inherit()
    });
    let mut child = command
        .spawn()
        .map_err(|_| "original hook could not start")?;
    let writer = if let Some(bytes) = payload {
        let mut stdin = child
            .stdin
            .take()
            .ok_or("original hook stdin unavailable")?;
        let bytes = bytes.to_vec();
        Some(thread::spawn(move || {
            let _ = stdin.write_all(&bytes);
        }))
    } else {
        None
    };
    let status = child.wait().map_err(|_| "original hook failed")?;
    if let Some(writer) = writer {
        let _ = writer.join();
    }
    Ok(status.code().unwrap_or(1))
}
fn payload() -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    io::stdin()
        .read_to_end(&mut bytes)
        .map_err(|_| "cannot read hook input")?;
    Ok(bytes)
}
fn check_cherry_author(cfg: &Config, identity: &Identity) -> Result<()> {
    let path = core::git_text(&cfg.git, &["rev-parse", "--git-path", "CHERRY_PICK_HEAD"])?;
    if !Path::new(&path).exists() {
        return Ok(());
    }
    let value = fs::read_to_string(path).map_err(|_| "cannot inspect cherry-pick source")?;
    let oid = value.trim();
    if !core::oid_valid(oid) {
        return Err("invalid CHERRY_PICK_HEAD".into());
    }
    let raw = core::query(
        &cfg.git,
        &["cat-file".into(), "commit".into(), oid.into()],
        None,
    )?;
    let (headers, _) = core::text(&raw)?
        .split_once("\n\n")
        .ok_or("malformed cherry-pick source")?;
    let authors: Vec<_> = headers
        .lines()
        .filter_map(|line| line.strip_prefix("author "))
        .collect();
    if authors.len() != 1 {
        return Err("missing or duplicate cherry-pick source author".into());
    }
    core::check_ident(authors[0], identity, "cherry-pick author")
}
fn check_rebase_source(cfg: &Config, oid: &str, identity: &Identity) -> Result<()> {
    if !core::oid_valid(oid) {
        return Err("invalid rebase source identifier".into());
    }
    let raw = core::query(
        &cfg.git,
        &["cat-file".into(), "commit".into(), oid.into()],
        None,
    )?;
    let (headers, message) = core::text(&raw)?
        .split_once("\n\n")
        .ok_or("malformed rebase source")?;
    if headers.contains(['\r', '\0']) {
        return Err("invalid rebase source headers".into());
    }
    let authors = headers
        .lines()
        .filter(|line| line.split(' ').next() == Some("author"))
        .collect::<Vec<_>>();
    if authors.len() != 1 {
        return Err("missing or duplicate rebase source author".into());
    }
    let author = authors[0]
        .strip_prefix("author ")
        .ok_or("malformed rebase source author")?;
    core::check_ident(author, identity, "rebase source author")?;
    // Input wording may be the violation this explicit reword is correcting.
    // Commit-msg validates reworded messages; actual resulting objects and the
    // entire outgoing range must pass before the agent reports success/pushes.
    core::validate_trailers_with_git(&cfg.git, message.as_bytes(), identity)
}
pub fn clean_message(git: &Path, path: &Path, identity: &Identity) -> Result<()> {
    let bytes = fs::read(path).map_err(|_| "cannot read commit message")?;
    let message = core::text(&bytes)?;
    if message.contains('\r') {
        return Err("commit messages require LF line endings".into());
    }
    let comment = util::capture(
        git,
        &["config".into(), "--get".into(), "core.commentChar".into()],
        None,
        Duration::from_secs(15),
    )?;
    let char = if comment.code == 0 {
        core::text(&comment.stdout)?.trim()
    } else {
        "#"
    };
    if char.len() != 1 || !(b' '..=b'~').contains(&char.as_bytes()[0]) {
        return Err("unsupported Git comment character".into());
    }
    let scissors = format!("{char} ------------------------ >8 ------------------------");
    let mut before = String::new();
    for line in message.split_inclusive('\n') {
        if line.trim_end_matches('\n') == scissors {
            break;
        }
        before.push_str(line);
    }
    let cleaned = core::query(
        git,
        &["stripspace".into(), "--strip-comments".into()],
        Some(before.as_bytes()),
    )?;
    core::validate_message_with_git(git, &cleaned, identity)?;
    // Always write exactly the checked bytes, closing --cleanup=verbatim gaps.
    fs::write(path, cleaned).map_err(|_| "cannot save validated commit message")?;
    Ok(())
}

/// A signing program entry point never queries a repository or fresh gh.
/// It either delegates a verification action to the saved native verifier or
/// refuses the action. Multiple/attached ssh -Y modes are rejected deliberately.
fn verification_allowed(kind: &str, args: &[String]) -> bool {
    if kind == "ssh" {
        let mut mode = None;
        let mut index = 0;
        while index < args.len() {
            let arg = &args[index];
            if arg == "-Y" {
                if mode.is_some() {
                    return false;
                }
                index += 1;
                let Some(value) = args.get(index) else {
                    return false;
                };
                if !["verify", "find-principals", "check-novalidate"].contains(&value.as_str()) {
                    return false;
                }
                mode = Some(value);
            } else if arg.starts_with("-Y") {
                return false;
            }
            index += 1;
        }
        return mode.is_some();
    }
    if kind != "openpgp" && kind != "x509" {
        return false;
    }
    let signing = args.iter().any(|arg| {
        if arg.starts_with("--") {
            let option = arg.split('=').next().unwrap_or(arg);
            return ["--sign", "--clearsign", "--clear-sign", "--detach-sign"]
                .iter()
                .any(|dangerous| {
                    option.len() > 2
                        && (dangerous.starts_with(option) || option.starts_with(dangerous))
                });
        }
        arg.starts_with('-') && arg[1..].contains(['s', 'b'])
    });
    args.iter().any(|arg| arg == "--verify") && !signing
}
fn signing_entry(cfg: &Config, kind: &str, args: &[String]) -> Result<i32> {
    if !verification_allowed(kind, args) {
        return Err(
            "local cryptographic signing is disabled; gh verifies identity, not signing-key UIDs"
                .into(),
        );
    }
    let program = cfg
        .verify_programs
        .get(kind)
        .ok_or("saved native verification backend unavailable")?;
    let path = Path::new(program);
    if !path.is_absolute() || !util::executable(path) {
        return Err("saved native verification backend unavailable".into());
    }
    if absolute(path)? == absolute(&cfg.cli())?
        || absolute(path)? == absolute(&cfg.canonical_cli())?
        || env::current_exe().ok().and_then(|p| p.canonicalize().ok()) == path.canonicalize().ok()
    {
        return Err("recursive verification backend is forbidden".into());
    }
    let mut command = Command::new(path);
    command
        .args(args)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        let _ = command.exec();
        Err("saved verification backend could not start".into())
    }
    #[cfg(not(unix))]
    {
        Ok(command
            .status()
            .map_err(|_| "saved verification backend could not start")?
            .code()
            .unwrap_or(1))
    }
}

pub fn run(cfg: &Config, name: &str, args: &[String]) -> Result<i32> {
    if let Some(kind) = name.strip_prefix("sign-") {
        return signing_entry(cfg, kind, args);
    }
    if name == "pre-push" {
        let bytes = payload()?;
        core::validate_pre_push(cfg, args, &bytes)?;
        return original(cfg, name, args, Some(&bytes));
    }
    // stdin-bearing non-policy hooks must retain byte-for-byte content too.
    let bytes = if matches!(
        name,
        "reference-transaction" | "post-rewrite" | "pre-receive" | "post-receive" | "proc-receive"
    ) {
        Some(payload()?)
    } else {
        None
    };
    let status = original(cfg, name, args, bytes.as_deref())?;
    if status != 0 {
        return Ok(status);
    }
    if name == "pre-applypatch" {
        return Err("git am is blocked; apply changes and create a checked commit".into());
    }
    if matches!(
        name,
        "pre-commit" | "pre-merge-commit" | "prepare-commit-msg" | "commit-msg" | "pre-rebase"
    ) {
        let tools = core::tools(Some(cfg))?;
        let identity = core::account(&tools)?;
        core::check_effective(&cfg.git, &identity, name != "pre-rebase")?;
        if matches!(name, "prepare-commit-msg" | "commit-msg") {
            check_cherry_author(cfg, &identity)?;
            let path = args
                .first()
                .ok_or("message hook requires its message file")?;
            if name == "commit-msg" {
                clean_message(&cfg.git, Path::new(path), &identity)?;
            } else {
                core::validate_trailers_with_git(
                    &cfg.git,
                    &fs::read(path).map_err(|_| "cannot read prepared message")?,
                    &identity,
                )?;
            }
        }
        if name == "pre-rebase" {
            core::history_safety(&cfg.git)?;
            let upstream = args
                .first()
                .filter(|value| !value.is_empty() && value.as_str() != "--root")
                .ok_or("root rebase is blocked; inspect author ownership before rewriting")?;
            let branch = args.get(1).map(String::as_str).unwrap_or("HEAD");
            let upstream = core::resolve_commit(&cfg.git, upstream)?
                .ok_or("cannot resolve rebase upstream")?;
            let branch =
                core::resolve_commit(&cfg.git, branch)?.ok_or("cannot resolve rebase source")?;
            let commits = core::query(
                &cfg.git,
                &["rev-list".into(), format!("{upstream}..{branch}")],
                None,
            )?;
            for oid in core::text(&commits)?.lines() {
                check_rebase_source(cfg, oid, &identity)?;
            }
        }
    }
    Ok(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn args(values: &[&str]) -> Vec<String> {
        values.iter().map(|v| v.to_string()).collect()
    }
    #[test]
    fn signing_never_passes_as_verification() {
        for kind in ["openpgp", "x509"] {
            assert!(verification_allowed(
                kind,
                &args(&["--status-fd=1", "--verify", "fixture"])
            ));
            for flag in [
                "--sign",
                "--sig",
                "--clearsign",
                "--cle",
                "--detach-sign",
                "-s",
                "-b",
                "-bs",
            ] {
                assert!(!verification_allowed(
                    kind,
                    &args(&["--verify", flag, "fixture"])
                ));
                assert!(!verification_allowed(kind, &args(&[flag, "fixture"])));
            }
        }
        assert!(!verification_allowed("unknown", &args(&["--verify"])));
    }
    #[test]
    fn ssh_verification_requires_one_explicit_safe_action() {
        for mode in ["verify", "find-principals", "check-novalidate"] {
            assert!(verification_allowed(
                "ssh",
                &args(&["-Y", mode, "-n", "git"])
            ));
        }
        for values in [
            &["-Y", "sign"][..],
            &["-Ysign"][..],
            &["-Yverify"][..],
            &["-Y", "verify", "-Y", "sign"][..],
            &["-Y"][..],
            &["-t", "ed25519"][..],
        ] {
            assert!(!verification_allowed("ssh", &args(values)));
        }
    }
}
