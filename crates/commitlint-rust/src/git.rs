//! Independent Git helpers for implicit edit paths and commit ranges: the first
//! Git on PATH is run verbatim, with bounded deadlines and no guard knowledge.
use crate::Result;
use std::{
    io::Read,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};

const OUTPUT_LIMIT: u64 = 64 * 1024 * 1024;

pub struct Output {
    pub code: i32,
    pub stdout: Vec<u8>,
}

/// First executable regular `git` on PATH; PATH is the only input.
pub fn find_git() -> Result<PathBuf> {
    let paths = std::env::var_os("PATH").ok_or("PATH is unavailable")?;
    let name = if cfg!(windows) { "git.exe" } else { "git" };
    for dir in std::env::split_paths(&paths) {
        let candidate = dir.join(name);
        if candidate.is_file() && executable(&candidate) {
            return candidate
                .canonicalize()
                .map_err(|_| "cannot resolve Git executable".to_string());
        }
    }
    Err("required git executable is unavailable".into())
}

fn executable(path: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::metadata(path)
            .map(|m| m.permissions().mode() & 0o111 != 0)
            .unwrap_or(false)
    }
    #[cfg(not(unix))]
    {
        path.is_file()
    }
}

pub fn oid_valid(value: &str) -> bool {
    matches!(value.len(), 40 | 64)
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

fn drain<R: Read + Send + 'static>(
    mut reader: R,
    keep: bool,
) -> mpsc::Receiver<std::io::Result<Vec<u8>>> {
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let mut bytes = Vec::new();
        let result = (&mut reader)
            .take(OUTPUT_LIMIT + 1)
            .read_to_end(&mut bytes)
            .and_then(|_| {
                if bytes.len() as u64 > OUTPUT_LIMIT {
                    Err(std::io::Error::other("output too large"))
                } else {
                    Ok(if keep { bytes } else { Vec::new() })
                }
            });
        let _ = tx.send(result);
    });
    rx
}

/// Run Git in its own process group; the full deadline also bounds pipe
/// draining, so a descendant that escaped the group cannot hang the linter.
pub fn capture(git: &Path, args: &[&str], timeout: Duration) -> Result<Output> {
    let mut command = Command::new(git);
    command
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .env("GIT_NO_REPLACE_OBJECTS", "1");
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    let mut child = command
        .spawn()
        .map_err(|_| "required command could not start".to_string())?;
    let out = drain(child.stdout.take().ok_or("missing command stdout")?, true);
    let err = drain(child.stderr.take().ok_or("missing command stderr")?, false);
    let start = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if start.elapsed() < timeout => thread::sleep(Duration::from_millis(10)),
            Ok(None) => {
                terminate(&mut child);
                return Err("required command timed out".into());
            }
            Err(_) => {
                terminate(&mut child);
                return Err("required command failed".into());
            }
        }
    };
    let remaining = || timeout.saturating_sub(start.elapsed());
    let stdout = out
        .recv_timeout(remaining())
        .map_err(|_| "command output pipes did not close before deadline")?
        .map_err(|_| "command output unreadable")?;
    err.recv_timeout(remaining())
        .map_err(|_| "command error pipe did not close before deadline")?
        .map_err(|_| "command error output unreadable")?;
    Ok(Output {
        code: status.code().unwrap_or(1),
        stdout,
    })
}

fn terminate(child: &mut std::process::Child) {
    // Only reached while the leader is unreaped, so its group ID is not reused.
    #[cfg(unix)]
    unsafe {
        libc::kill(-(child.id() as i32), libc::SIGKILL);
    }
    let _ = child.kill();
    let _ = child.wait();
}

const SHORT: Duration = Duration::from_secs(15);
const LONG: Duration = Duration::from_secs(30);

pub fn commit_message_path(git: &Path) -> Result<PathBuf> {
    let result = capture(git, &["rev-parse", "--git-path", "COMMIT_EDITMSG"], SHORT)?;
    if result.code != 0 {
        return Err("cannot locate Git commit message".into());
    }
    Ok(PathBuf::from(
        std::str::from_utf8(&result.stdout)
            .map_err(|_| "invalid message path encoding")?
            .trim(),
    ))
}

fn resolve(git: &Path, reference: &str) -> Result<String> {
    let spec = format!("{reference}^{{commit}}");
    let output = capture(
        git,
        &["rev-parse", "--verify", "--end-of-options", &spec],
        SHORT,
    )?;
    if output.code != 0 {
        return Err("cannot resolve commit range".into());
    }
    let oid = std::str::from_utf8(&output.stdout)
        .map_err(|_| "invalid commit range")?
        .trim()
        .to_string();
    if !oid_valid(&oid) {
        return Err("invalid commit range".into());
    }
    Ok(oid)
}

/// Raw message bytes of every commit in `from..to`.
pub fn range_messages(git: &Path, from: &str, to: &str) -> Result<Vec<Vec<u8>>> {
    let (from, to) = (resolve(git, from)?, resolve(git, to)?);
    let list = capture(git, &["rev-list", &format!("{from}..{to}")], LONG)?;
    if list.code != 0 {
        return Err("cannot enumerate commit range".into());
    }
    let mut messages = Vec::new();
    for oid in String::from_utf8(list.stdout)
        .map_err(|_| "invalid revision response")?
        .lines()
    {
        if !oid_valid(oid) {
            return Err("invalid revision response".into());
        }
        let raw = capture(git, &["cat-file", "commit", oid], SHORT)?;
        if raw.code != 0 {
            return Err("cannot read commit object".into());
        }
        let split = raw
            .stdout
            .windows(2)
            .position(|p| p == b"\n\n")
            .ok_or("commit object has no message")?;
        messages.push(raw.stdout[split + 2..].to_vec())
    }
    Ok(messages)
}
