//! Hooked Git porcelain and narrowly bound native rebase editors.
use super::{Journal, Mapping, inspect, storage};
use crate::{Config, Result, core};
use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

pub(super) fn query(cfg: &Config, cwd: &Path, args: &[&str]) -> Result<String> {
    let mut all = vec!["-C".to_string(), cwd.to_string_lossy().into_owned()];
    all.extend(args.iter().map(|s| s.to_string()));
    Ok(core::text(&core::query(&cfg.git, &all, None)?)?
        .trim()
        .to_string())
}

pub(super) fn guarded(
    cfg: &Config,
    root: &Path,
    j: &Journal,
    cwd: &Path,
    args: &[String],
    author_date: Option<&str>,
    editors: bool,
) -> Result<()> {
    let exe = std::env::current_exe().map_err(|_| "cannot locate native repair helper")?;
    let mut cmd = Command::new(&exe);
    cmd.args(["--config"])
        .arg(root.join("guard-config.json"))
        .arg("git")
        .arg("-c")
        .arg(format!("core.hooksPath={}", j.receipt.hooks_dir.display()))
        .arg("-C")
        .arg(cwd);
    for setting in [
        "commit.gpgsign=false",
        "tag.gpgsign=false",
        "commit.cleanup=strip",
        "commit.template=",
        "core.fsync=committed",
        "core.fsyncMethod=fsync",
        "core.commentChar=#",
        "core.commentString=#",
        "rebase.autoSquash=false",
        "rebase.updateRefs=false",
        "rebase.autoStash=false",
        "rebase.abbreviateCommands=false",
        "rebase.instructionFormat=%s",
        "rebase.missingCommitsCheck=error",
        "rebase.rescheduleFailedExec=false",
        "rebase.stat=false",
        "rerere.enabled=false",
        "rerere.autoupdate=false",
    ] {
        cmd.arg("-c").arg(setting);
    }
    cmd.arg("-c")
        .arg(format!("core.abbrev={}", j.original_tip.len()));
    cmd.args(args)
        .current_dir(cwd)
        .env("GIT_AUTHOR_NAME", &j.receipt.identity.login)
        .env("GIT_AUTHOR_EMAIL", &j.receipt.identity.email)
        .env("GIT_COMMITTER_NAME", &j.receipt.identity.login)
        .env("GIT_COMMITTER_EMAIL", &j.receipt.identity.email)
        .env("GIT_COMMITTER_DATE", &j.committer_date)
        .env("GIT_NO_REPLACE_OBJECTS", "1")
        .env("GH_PROMPT_DISABLED", "1")
        .env("GH_NO_UPDATE_NOTIFIER", "1")
        .env_remove("GH_DEBUG")
        .env_remove("DEBUG")
        .env_remove("GIT_AUTHOR_DATE")
        .env_remove("GIT_DIR")
        .env_remove("GIT_COMMON_DIR")
        .env_remove("GIT_WORK_TREE")
        .env_remove("GIT_INDEX_FILE")
        .env_remove("GIT_CONFIG_COUNT")
        .env_remove("GIT_CONFIG_PARAMETERS")
        .env_remove("GIT_NAMESPACE")
        .env_remove("GIT_SEQUENCE_EDITOR")
        .env_remove("GIT_EDITOR")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    if let Some(date) = author_date {
        cmd.env("GIT_AUTHOR_DATE", format!("@{date}"));
    }
    if editors {
        cmd.env(
            "GIT_SEQUENCE_EDITOR",
            editor_command(&exe, root, j, "sequence"),
        )
        .env("GIT_EDITOR", editor_command(&exe, root, j, "message"));
    } else {
        // An unexpected editor must fail, never inherit an arbitrary shell command.
        cmd.env("GIT_EDITOR", editor_command(&exe, root, j, "unexpected"))
            .env(
                "GIT_SEQUENCE_EDITOR",
                editor_command(&exe, root, j, "unexpected"),
            );
    }
    let mut child = cmd.spawn().map_err(|_| "guarded Git could not start")?;
    let stderr = child
        .stderr
        .take()
        .ok_or("guarded Git diagnostic pipe missing")?;
    // Drain continuously: even an enormous hook diagnostic must not fill the
    // pipe and deadlock Git. Only a bounded prefix is retained in memory.
    let (send, receive) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = send.send(bounded_diagnostic(stderr));
    });
    let status = child.wait().map_err(|_| "guarded Git could not finish")?;
    // A hook may leave a descendant holding the inherited descriptor after
    // Git exits. Never wait indefinitely for that unrelated process's EOF.
    let diagnostic = receive_diagnostic(receive, std::time::Duration::from_secs(2))?;
    if !status.success() {
        let secrets: Vec<String> = [
            "GH_TOKEN",
            "GITHUB_TOKEN",
            "GH_ENTERPRISE_TOKEN",
            "GITHUB_ENTERPRISE_TOKEN",
        ]
        .into_iter()
        .filter_map(|key| std::env::var(key).ok())
        .filter(|value| !value.is_empty())
        .collect();
        let diagnostic = scrub_diagnostic(&diagnostic, &secrets);
        return Err(format!(
            "guarded Git refused or hooks changed the operation; retained staging evidence{}",
            if diagnostic.is_empty() {
                String::new()
            } else {
                format!("; Git diagnostic: {diagnostic}")
            }
        ));
    }
    // Check the source configuration separately in coordinator recheck. No raw
    // Git mutations are used here; the executable always enters the wrapper.
    let _ = cfg;
    Ok(())
}
const DIAGNOSTIC_LIMIT: usize = 16 * 1024;
fn receive_diagnostic(
    receive: std::sync::mpsc::Receiver<Result<String>>,
    timeout: std::time::Duration,
) -> Result<String> {
    match receive.recv_timeout(timeout) {
        Ok(result) => result,
        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
            Ok("[diagnostic pipe remains open after Git exited]".into())
        }
        Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
            Err("guarded Git diagnostic reader failed".into())
        }
    }
}
fn bounded_diagnostic(mut input: impl Read) -> Result<String> {
    let mut captured = Vec::new();
    let mut chunk = [0u8; 8192];
    let mut truncated = false;
    loop {
        let count = input
            .read(&mut chunk)
            .map_err(|_| "cannot read guarded Git diagnostic")?;
        if count == 0 {
            break;
        }
        let remaining = DIAGNOSTIC_LIMIT.saturating_sub(captured.len());
        captured.extend_from_slice(&chunk[..count.min(remaining)]);
        truncated |= count > remaining;
    }
    if truncated {
        // Never expose a credential fragment cut at the byte boundary.
        captured.truncate(
            captured
                .iter()
                .rposition(|byte| *byte == b'\n')
                .map_or(0, |index| index + 1),
        );
    }
    let mut text = String::from_utf8_lossy(&captured).into_owned();
    if truncated {
        text.push_str("[diagnostic truncated]");
    }
    Ok(text)
}
fn scrub_diagnostic(input: &str, secrets: &[String]) -> String {
    let mut text = input.to_string();
    for secret in secrets {
        text = text.replace(secret, "[redacted]");
    }
    // Token prefixes cover keychain credentials without another gh invocation.
    // Preserve ordinary words and Git's useful hook error details.
    let mut out = String::new();
    for segment in text.split_inclusive(char::is_whitespace) {
        let mut segment = segment.to_string();
        for prefix in ["ghp_", "gho_", "ghu_", "ghs_", "ghr_", "github_pat_"] {
            while let Some(start) = segment.find(prefix) {
                let end = start
                    + segment[start..]
                        .bytes()
                        .take_while(|b| b.is_ascii_alphanumeric() || *b == b'_')
                        .count();
                segment.replace_range(start..end, "[redacted]");
            }
        }
        let mut cursor = 0;
        while let Some(protocol) = segment[cursor..].find("://").map(|index| cursor + index) {
            let start = protocol + 3;
            let authority_end = segment[start..]
                .find(['/', '?', '#', ',', ';', ')', ']'])
                .map_or(segment.len(), |offset| start + offset);
            if let Some(at) = segment[start..authority_end].rfind('@') {
                segment.replace_range(start..start + at, "[redacted]");
            }
            cursor = start;
        }
        out.extend(
            segment
                .chars()
                .filter(|c| !c.is_control() || matches!(c, '\n' | '\t')),
        );
    }
    out.trim().to_string()
}
fn quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}
fn editor_command(exe: &Path, root: &Path, j: &Journal, kind: &str) -> String {
    [
        exe.to_string_lossy().as_ref(),
        "--config",
        root.join("guard-config.json").to_string_lossy().as_ref(),
        "fix",
        "--private-editor",
        root.to_string_lossy().as_ref(),
        &j.plan_id,
        kind,
    ]
    .iter()
    .map(|s| quote(s))
    .collect::<Vec<_>>()
    .join(" ")
}
fn args(values: &[&str]) -> Vec<String> {
    values.iter().map(|s| s.to_string()).collect()
}

pub(super) fn run(cfg: &Config, root: &Path, j: &mut Journal) -> Result<()> {
    if j.receipt.operation == "repair" {
        j.phase = "rebase".into();
        storage::save_journal(root, j)?;
        guarded(
            cfg,
            root,
            j,
            &j.staging,
            &args(&[
                "rebase",
                "--interactive",
                "--merge",
                "--force-rebase",
                "--keep-empty",
                "--empty=keep",
                "--no-gpg-sign",
                "--no-signoff",
                "--no-autosquash",
                "--no-update-refs",
                "--no-fork-point",
                "--no-rebase-merges",
                "--onto",
                &j.receipt.base,
                &j.receipt.base,
            ]),
            None,
            true,
        )?;
        *j = storage::read_journal(root)?;
        if j.editor_index != j.receipt.sources.len() {
            return Err("rebase did not consume every approved message".into());
        }
        let listed = query(
            cfg,
            &j.staging,
            &[
                "rev-list",
                "--reverse",
                &format!("{}..HEAD", j.receipt.base),
            ],
        )?;
        let oids: Vec<_> = listed.lines().collect();
        if oids.len() != j.receipt.sources.len() {
            return Err("rebase changed source count".into());
        }
        for (index, oid) in oids.iter().enumerate() {
            verify_one(cfg, j, index, oid)?;
            if let Some(mapped) = j.mapping.get(index) {
                if mapped.new_oid != *oid {
                    return Err("rebase changed an already verified replacement".into());
                }
            } else {
                j.mapping.push(Mapping {
                    source_oid: j.receipt.sources[index].source_oid.clone(),
                    new_oid: (*oid).into(),
                });
                j.verified.push(inspect::source(&cfg.git, oid)?);
                storage::save_journal(root, j)?;
            }
        }
    } else if j.receipt.operation == "author-migration" {
        j.phase = "migration-base-intent".into();
        storage::save_journal(root, j)?;
        guarded(
            cfg,
            root,
            j,
            &j.staging,
            &args(&["checkout", "--detach", &j.receipt.base]),
            None,
            false,
        )?;
        for index in 0..j.receipt.sources.len() {
            j.phase = format!("migration-restore-{index}");
            storage::save_journal(root, j)?;
            let oid = j.receipt.sources[index].source_oid.clone();
            guarded(
                cfg,
                root,
                j,
                &j.staging,
                &args(&[
                    "restore",
                    &format!("--source={oid}"),
                    "--staged",
                    "--worktree",
                    "--",
                    ":/",
                ]),
                None,
                false,
            )?;
            // Restore defaults to no-overlay: missing tracked paths are removed.
            query(
                cfg,
                &j.staging,
                &["diff", "--cached", "--exit-code", &oid, "--"],
            )?;
            query(cfg, &j.staging, &["diff", "--exit-code", "--"])?;
            let date = author_date(&j.receipt.sources[index].author)?.to_string();
            // Write approved bytes exclusively; never truncate existing evidence.
            let actual_path = root.join(format!("commit-message-{index}.txt"));
            write_new_message(&actual_path, j.candidates[index].message.as_bytes())?;
            j.phase = format!("migration-commit-{index}");
            storage::save_journal(root, j)?;
            guarded(
                cfg,
                root,
                j,
                &j.staging,
                &args(&[
                    "commit",
                    "--allow-empty",
                    "--cleanup=strip",
                    "-F",
                    actual_path.to_str().ok_or("non-UTF-8 message path")?,
                ]),
                Some(&date),
                false,
            )?;
            let new_oid = query(cfg, &j.staging, &["rev-parse", "HEAD"])?;
            verify_one(cfg, j, index, &new_oid)?;
            if !query(
                cfg,
                &j.staging,
                &["status", "--porcelain=v1", "--untracked-files=all"],
            )?
            .is_empty()
            {
                return Err("hook changed staging worktree; retained evidence".into());
            }
            j.verified.push(inspect::source(&cfg.git, &new_oid)?);
            j.mapping.push(Mapping {
                source_oid: oid,
                new_oid,
            });
            storage::save_journal(root, j)?;
        }
    } else {
        return Err("unsupported receipt operation".into());
    }
    j.phase = "verified".into();
    storage::save_journal(root, j)
}
fn write_new_message(path: &Path, bytes: &[u8]) -> Result<()> {
    #[cfg(unix)]
    {
        use std::{io::Write, os::unix::fs::OpenOptionsExt};
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW)
            .open(path)
            .map_err(|_| "cannot exclusively create message")?;
        file.write_all(bytes)
            .and_then(|_| file.sync_all())
            .map_err(|_| "cannot persist approved message")?;
        fs::File::open(path.parent().ok_or("message parent missing")?)
            .and_then(|f| f.sync_all())
            .map_err(|_| "cannot sync message directory")?;
        Ok(())
    }
    #[cfg(not(unix))]
    {
        let _ = (path, bytes);
        Err("Unix repair storage required".into())
    }
}
pub(super) fn author_date(author: &str) -> Result<&str> {
    let (_, remainder) = author.rsplit_once("> ").ok_or("malformed author date")?;
    Ok(remainder)
}
pub(super) fn verify_one(cfg: &Config, j: &Journal, index: usize, oid: &str) -> Result<()> {
    let old = &j.receipt.sources[index];
    let new = inspect::source(&cfg.git, oid)?;
    let parent = if index == 0 {
        &j.receipt.base
    } else {
        &j.mapping
            .get(index - 1)
            .ok_or("missing verified parent mapping")?
            .new_oid
    };
    let expected_author = format!(
        "{} <{}> {}",
        j.receipt.identity.login,
        j.receipt.identity.email,
        author_date(&old.author)?
    );
    let expected_committer = format!(
        "{} <{}> {}",
        j.receipt.identity.login,
        j.receipt.identity.email,
        j.committer_date.trim_start_matches('@')
    );
    if new.tree != old.tree
        || &new.parent != parent
        || new.author != expected_author
        || new.committer != expected_committer
        || new.message != j.candidates[index].message
        || oid == old.source_oid
    {
        return Err(format!(
            "replacement {index} differs from approved tree/parent/date/message/identity"
        ));
    }
    core::validate_commit(&cfg.git, oid, &j.receipt.identity)
}

pub(super) fn editor(args: &[String], cfg: &Config) -> Result<()> {
    if args.len() != 4 {
        return Err("private editor requires an active journal and intended file".into());
    }
    let root = PathBuf::from(&args[0]);
    let mut j = storage::read_journal(&root)?;
    if j.schema_version != 1
        || j.plan_id != args[1]
        || storage::hash(&j.receipt)? != j.plan_id
        || j.phase != "rebase"
        || j.receipt.operation != "repair"
    {
        return Err("private editor is not authorized in this journal phase".into());
    }
    let expected_root =
        storage::root(&j.receipt.common_dir)?.join(format!("operation-{}", j.plan_id));
    if root != expected_root
        || storage::hash(&(j.plan_id.as_str(), &j.candidates))? != j.apply_digest
    {
        return Err("private editor journal binding mismatch".into());
    }
    let lock: String = storage::read(&storage::root(&j.receipt.common_dir)?.join("lock"), true)?;
    if lock != j.plan_id {
        return Err("private editor requires its operation lock".into());
    }
    let cwd = std::env::current_dir()
        .map_err(|_| "cannot locate editor worktree")?
        .canonicalize()
        .map_err(|_| "editor worktree missing")?;
    if cwd
        != j.staging
            .canonicalize()
            .map_err(|_| "staging worktree missing")?
    {
        return Err("private editor invoked outside staging".into());
    }
    let git_dir = j
        .staging_git_dir
        .as_ref()
        .ok_or("journal has no staging Git directory")?;
    if PathBuf::from(query(
        cfg,
        &j.staging,
        &["rev-parse", "--absolute-git-dir"],
    )?)
    .canonicalize()
    .map_err(|_| "staging Git directory missing")?
        != git_dir
            .canonicalize()
            .map_err(|_| "journal staging Git directory missing")?
    {
        return Err("private editor staging directory mismatch".into());
    }
    let input_path = PathBuf::from(&args[3]);
    if fs::symlink_metadata(&input_path)
        .map_err(|_| "editor input missing")?
        .file_type()
        .is_symlink()
    {
        return Err("private editor refuses symlink inputs".into());
    }
    let supplied = input_path
        .canonicalize()
        .map_err(|_| "editor input missing")?;
    match args[2].as_str() {
        "sequence" => {
            let intended = git_dir
                .join("rebase-merge/git-rebase-todo")
                .canonicalize()
                .map_err(|_| "intended todo missing")?;
            if supplied != intended || j.editor_index != 0 {
                return Err("unexpected sequence editor path or phase".into());
            }
            let todo = fs::read_to_string(&supplied).map_err(|_| "cannot inspect rebase todo")?;
            if todo.len() > 4 * 1024 * 1024 {
                return Err("rebase todo too large".into());
            }
            let lines: Vec<_> = todo
                .lines()
                .filter(|line| !line.trim().is_empty() && !line.starts_with('#'))
                .collect();
            if lines.len() != j.receipt.sources.len() {
                return Err("todo source count differs from receipt".into());
            }
            let mut replacement = String::new();
            for (line, source) in lines.iter().zip(&j.receipt.sources) {
                let mut fields = line.split_whitespace();
                if fields.next() != Some("pick")
                    || fields.next() != Some(source.source_oid.as_str())
                {
                    return Err(
                        "todo must contain exactly the frozen full source OIDs in order".into(),
                    );
                }
                replacement.push_str(&format!("reword {}\n", source.source_oid));
            }
            fs::write(&supplied, replacement).map_err(|_| "cannot save verified todo")?;
        }
        "message" => {
            let intended = git_dir
                .join("COMMIT_EDITMSG")
                .canonicalize()
                .map_err(|_| "intended message missing")?;
            if supplied != intended || j.editor_index >= j.candidates.len() {
                return Err("unexpected message editor path or count".into());
            }
            let index = j.editor_index;
            // The sequencer records a command in done before executing it.
            // Bind the editor to that exact ordered source, not to an assumption
            // that Git has already committed the staged replacement at HEAD.
            let done_path = git_dir.join("rebase-merge/done");
            if fs::symlink_metadata(&done_path)
                .map_err(|_| "sequencer done file missing")?
                .file_type()
                .is_symlink()
            {
                return Err("symlink sequencer state is unsupported".into());
            }
            let done = fs::read_to_string(&done_path)
                .map_err(|_| "cannot inspect sequencer source position")?;
            if done.len() > 4 * 1024 * 1024 {
                return Err("sequencer state too large".into());
            }
            let commands: Vec<_> = done
                .lines()
                .filter(|line| !line.trim().is_empty() && !line.starts_with('#'))
                .collect();
            if commands.len() != index + 1 {
                return Err("unexpected sequencer source position".into());
            }
            for (command, source) in commands.iter().zip(&j.receipt.sources) {
                let fields: Vec<_> = command.split_whitespace().collect();
                if fields.as_slice() != ["reword", source.source_oid.as_str()] {
                    return Err("sequencer executed an unapproved command/source".into());
                }
            }
            let head = query(cfg, &j.staging, &["rev-parse", "HEAD"])?;
            let count: usize = query(
                cfg,
                &j.staging,
                &["rev-list", "--count", &format!("{}..HEAD", j.receipt.base)],
            )?
            .parse()
            .map_err(|_| "invalid editor history count")?;
            // Non-fast-forward reword invokes commit's editor before creating
            // the replacement. Git's reuse/amend path may invoke it after.
            let previous = if count == index {
                head.clone()
            } else if count == index + 1 {
                query(cfg, &j.staging, &["rev-parse", "HEAD^"])?
            } else {
                return Err("editor history does not match the approved source position".into());
            };
            if index == 0 {
                if previous != j.receipt.base {
                    return Err("editor changed the excluded base".into());
                }
            } else {
                verify_one(cfg, &j, index - 1, &previous)?;
                if j.mapping.len() != index - 1 {
                    return Err("editor mapping count mismatch".into());
                }
                j.verified.push(inspect::source(&cfg.git, &previous)?);
                j.mapping.push(Mapping {
                    source_oid: j.receipt.sources[index - 1].source_oid.clone(),
                    new_oid: previous.clone(),
                });
            }
            query(
                cfg,
                &j.staging,
                &[
                    "diff",
                    "--cached",
                    "--exit-code",
                    &j.receipt.sources[index].source_oid,
                    "--",
                ],
            )?;
            if query(cfg, &j.staging, &["var", "GIT_AUTHOR_IDENT"])?
                != j.receipt.sources[index].author
            {
                return Err("editor effective source Author/date differs from receipt".into());
            }
            if count == index + 1 {
                let current = inspect::source(&cfg.git, &head)?;
                if current.tree != j.receipt.sources[index].tree
                    || current.author != j.receipt.sources[index].author
                    || current.parent != previous
                {
                    return Err("amend editor source tree/author/parent mismatch".into());
                }
            }
            fs::write(&supplied, &j.candidates[index].message)
                .map_err(|_| "cannot save approved message")?;
            j.editor_index += 1;
            storage::save_journal(&root, &j)?;
        }
        _ => return Err("unexpected private editor invocation".into()),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn diagnostic_reader_drains_large_output_with_bounded_retention() {
        let mut input = std::io::Cursor::new(
            format!(
                "hook rejected: useful reason\n{}",
                "x".repeat(3 * 1024 * 1024)
            )
            .into_bytes(),
        );
        let diagnostic = bounded_diagnostic(&mut input).unwrap();
        assert_eq!(input.position(), input.get_ref().len() as u64);
        assert_eq!(
            diagnostic,
            "hook rejected: useful reason\n[diagnostic truncated]"
        );
        assert!(diagnostic.len() < DIAGNOSTIC_LIMIT);
    }
    #[test]
    fn diagnostic_redacts_credentials_and_terminal_controls() {
        let output = scrub_diagnostic(
            "hook: do not use old-secret; ghp_123456 github_pat_test https://user:password@github.com/repo\n\x1b[31mrejected",
            &["old-secret".into()],
        );
        assert!(output.contains("hook: do not use [redacted]"));
        assert!(output.contains("https://[redacted]@github.com/repo"));
        for secret in [
            "old-secret",
            "ghp_123456",
            "github_pat_test",
            "password",
            "\x1b",
        ] {
            assert!(!output.contains(secret));
        }
        assert!(output.contains("rejected"));
    }
    #[test]
    fn diagnostic_redacts_each_adjacent_url_without_whitespace() {
        let output = scrub_diagnostic(
            "https://one:secret-one@host/repo,ssh://two:secret-two@host/repo;https://three:secret-three@host",
            &[],
        );
        for secret in ["secret-one", "secret-two", "secret-three"] {
            assert!(!output.contains(secret), "{output}");
        }
        assert_eq!(output.matches("[redacted]").count(), 3);
    }
    #[cfg(unix)]
    #[test]
    fn inherited_open_diagnostic_pipe_does_not_wait_indefinitely_after_git_exits() {
        use std::{
            os::unix::net::UnixStream,
            time::{Duration, Instant},
        };
        let (reader, inherited_writer) = UnixStream::pair().unwrap();
        let (send, receive) = std::sync::mpsc::channel();
        let worker = std::thread::spawn(move || {
            let _ = send.send(bounded_diagnostic(reader));
        });
        let start = Instant::now();
        let result = receive_diagnostic(receive, Duration::from_millis(50)).unwrap();
        assert!(result.contains("pipe remains open"));
        assert!(start.elapsed() < Duration::from_secs(2));
        // Close only our fixture descriptor and wait for its owned reader.
        drop(inherited_writer);
        // Timeout drops the receiver, so the worker's send may fail normally.
        let _ = worker.join();
    }
}
