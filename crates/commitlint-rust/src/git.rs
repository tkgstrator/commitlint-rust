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

/// Parse trailers over original raw input without invoking hooks or gh.
/// Git configuration and PATH resolution follow the caller's explicit cwd.
pub fn interpret_trailers(
    raw: &str,
    git: Option<&Path>,
    cwd: Option<&Path>,
    timeout: Duration,
    output_limit: usize,
) -> Result<String> {
    let resolved;
    let git = match git {
        Some(path) => path,
        None => {
            resolved = find_git()?;
            &resolved
        }
    };
    let output = trailer_capture(git, cwd, raw.as_bytes(), timeout, output_limit)?;
    if output.code != 0 {
        return Err("Git trailer command failed".into());
    }
    String::from_utf8(output.stdout).map_err(|_| "Git trailer output is not UTF-8".into())
}

#[cfg(unix)]
fn trailer_capture(
    git: &Path,
    cwd: Option<&Path>,
    input: &[u8],
    timeout: Duration,
    limit: usize,
) -> Result<Output> {
    use std::{
        io::Write,
        os::{fd::AsRawFd, unix::process::CommandExt},
    };
    struct Guard {
        child: std::process::Child,
        reaped: bool,
    }
    impl Drop for Guard {
        fn drop(&mut self) {
            if !self.reaped {
                terminate(&mut self.child);
            }
        }
    }
    fn nonblocking(fd: &impl AsRawFd) -> Result<()> {
        let raw = fd.as_raw_fd();
        let flags = unsafe { libc::fcntl(raw, libc::F_GETFL) };
        if flags == -1 || unsafe { libc::fcntl(raw, libc::F_SETFL, flags | libc::O_NONBLOCK) } == -1
        {
            return Err("cannot configure trailer command pipes".into());
        }
        Ok(())
    }
    fn exited(pid: u32) -> Result<bool> {
        // Observe without reaping: the leader keeps the process-group ID
        // reserved until every pipe is closed and group cleanup is complete.
        let mut info: libc::siginfo_t = unsafe { std::mem::zeroed() };
        let result = unsafe {
            libc::waitid(
                libc::P_PID,
                pid as _,
                &mut info,
                libc::WEXITED | libc::WNOWAIT | libc::WNOHANG,
            )
        };
        if result != 0 {
            if std::io::Error::last_os_error().kind() == std::io::ErrorKind::Interrupted {
                return Ok(false);
            }
            return Err("cannot observe trailer command".into());
        }
        Ok(info.si_signo != 0)
    }
    fn read_pipe<R: Read>(
        pipe: &mut Option<R>,
        kept: &mut Vec<u8>,
        count: &mut usize,
        limit: usize,
        keep: bool,
    ) -> Result<()> {
        let Some(reader) = pipe.as_mut() else {
            return Ok(());
        };
        let mut bytes = [0u8; 8192];
        // A continuously-writing child cannot starve the deadline check.
        for _ in 0..16 {
            match reader.read(&mut bytes) {
                Ok(0) => {
                    *pipe = None;
                    break;
                }
                Ok(n) => {
                    *count = count
                        .checked_add(n)
                        .ok_or("Git trailer output exceeds limit")?;
                    if *count > limit {
                        return Err("Git trailer output exceeds limit".into());
                    }
                    if keep {
                        kept.extend_from_slice(&bytes[..n]);
                    }
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(_) => return Err("cannot read trailer command output".into()),
            }
        }
        Ok(())
    }
    let start = Instant::now();
    let mut command = Command::new(git);
    command
        .args(["interpret-trailers", "--parse"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .env("GIT_NO_REPLACE_OBJECTS", "1")
        .process_group(0);
    if let Some(cwd) = cwd {
        command.current_dir(cwd);
    }
    let child = command
        .spawn()
        .map_err(|_| "Git trailer command could not start")?;
    let mut guard = Guard {
        child,
        reaped: false,
    };
    let stdin = guard
        .child
        .stdin
        .take()
        .ok_or("missing trailer input pipe")?;
    let stdout = guard
        .child
        .stdout
        .take()
        .ok_or("missing trailer output pipe")?;
    let stderr = guard
        .child
        .stderr
        .take()
        .ok_or("missing trailer error pipe")?;
    nonblocking(&stdin)?;
    nonblocking(&stdout)?;
    nonblocking(&stderr)?;
    let mut stdin = Some(stdin);
    let mut stdout = Some(stdout);
    let mut stderr = Some(stderr);
    let mut sent = 0;
    let mut output = Vec::new();
    let mut discarded = Vec::new();
    let mut out_count = 0;
    let mut err_count = 0;
    loop {
        if start.elapsed() >= timeout {
            return Err("Git trailer command timed out".into());
        }
        if sent == input.len() {
            stdin = None;
        }
        if let Some(writer) = stdin.as_mut() {
            match writer.write(&input[sent..]) {
                Ok(0) => return Err("Git trailer input pipe closed".into()),
                Ok(n) => sent += n,
                Err(e)
                    if matches!(
                        e.kind(),
                        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::Interrupted
                    ) => {}
                Err(_) => return Err("cannot write Git trailer input".into()),
            }
        }
        read_pipe(&mut stdout, &mut output, &mut out_count, limit, true)?;
        read_pipe(&mut stderr, &mut discarded, &mut err_count, limit, false)?;
        if stdin.is_none() && stdout.is_none() && stderr.is_none() && exited(guard.child.id())? {
            // The unreaped leader guarantees this ID still belongs to our
            // group. Remove any descendants before releasing that identity.
            unsafe {
                libc::kill(-(guard.child.id() as i32), libc::SIGKILL);
            }
            let status = guard
                .child
                .wait()
                .map_err(|_| "cannot reap Git trailer command")?;
            guard.reaped = true;
            return Ok(Output {
                code: status.code().unwrap_or(1),
                stdout: output,
            });
        }
        thread::sleep(Duration::from_millis(2));
    }
}

#[cfg(not(unix))]
fn trailer_capture(
    _git: &Path,
    _cwd: Option<&Path>,
    _input: &[u8],
    _timeout: Duration,
    _limit: usize,
) -> Result<Output> {
    Err("bounded trailer execution requires a supported Unix release target".into())
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
