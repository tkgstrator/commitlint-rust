use crate::{Config, Result};
use std::{
    ffi::OsStr,
    io::{Read, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};
pub struct Output {
    pub code: i32,
    pub stdout: Vec<u8>,
}
/// Capture without printing arguments, stderr, tokens, or remote URLs.
pub fn capture(
    program: &Path,
    args: &[String],
    input: Option<&[u8]>,
    timeout: Duration,
) -> Result<Output> {
    capture_env(program, args, input, timeout, &[])
}
pub fn capture_env(
    program: &Path,
    args: &[String],
    input: Option<&[u8]>,
    timeout: Duration,
    environment: &[(&str, Option<&str>)],
) -> Result<Output> {
    capture_env_bounded(program, args, input, timeout, environment, 64 * 1024 * 1024)
}
/// Keep output allocation bounded while a child is still running.
pub fn capture_env_bounded(
    program: &Path,
    args: &[String],
    input: Option<&[u8]>,
    timeout: Duration,
    environment: &[(&str, Option<&str>)],
    limit: usize,
) -> Result<Output> {
    let mut command = Command::new(program);
    command
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .env_remove("GH_DEBUG")
        .env_remove("DEBUG")
        .env("GH_PROMPT_DISABLED", "1")
        .env("GH_NO_UPDATE_NOTIFIER", "1")
        .env("GIT_NO_REPLACE_OBJECTS", "1");
    command
        .env("GH_NO_EXTENSION_UPDATE_NOTIFIER", "1")
        .env("DO_NOT_TRACK", "1");
    for (key, value) in environment {
        if let Some(value) = value {
            command.env(key, value);
        } else {
            command.env_remove(key);
        }
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    let mut child = command
        .spawn()
        .map_err(|_| "required command could not start".to_string())?;
    let mut stdout = child.stdout.take().ok_or("missing command stdout")?;
    let mut stderr = child.stderr.take().ok_or("missing command stderr")?;
    let (out_tx, out_rx) = std::sync::mpsc::channel();
    let exceeded = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let reader_exceeded = exceeded.clone();
    thread::spawn(move || {
        let mut bytes = Vec::new();
        let result = Read::by_ref(&mut stdout)
            .take(limit.saturating_add(1) as u64)
            .read_to_end(&mut bytes)
            .map_err(|_| "command output unreadable".to_string())
            .and_then(|_| {
                if bytes.len() > limit {
                    reader_exceeded.store(true, std::sync::atomic::Ordering::Release);
                    Err("command output exceeded safe limit".into())
                } else {
                    Ok(bytes)
                }
            });
        let _ = out_tx.send(result);
    });
    let (err_tx, err_rx) = std::sync::mpsc::channel();
    thread::spawn(move || {
        let mut buf = [0u8; 8192];
        loop {
            match stderr.read(&mut buf) {
                Ok(0) => break,
                Ok(_) => (),
                Err(_) => break,
            }
        }
        let _ = err_tx.send(());
    });
    let mut stdin = child.stdin.take().ok_or("missing command stdin")?;
    let bytes = input.unwrap_or_default().to_vec();
    let (write_tx, write_rx) = std::sync::mpsc::channel();
    thread::spawn(move || {
        let _ = stdin.write_all(&bytes);
        let _ = write_tx.send(());
    });
    let start = Instant::now();
    let status = loop {
        if exceeded.load(std::sync::atomic::Ordering::Acquire) {
            terminate(&mut child);
            break Err("command output exceeded safe limit".to_string());
        }
        match child.try_wait() {
            Ok(Some(status)) => break Ok(status),
            Ok(None) if start.elapsed() < timeout => thread::sleep(Duration::from_millis(10)),
            Ok(None) => {
                terminate(&mut child);
                break Err("required command timed out".to_string());
            }
            Err(_) => {
                terminate(&mut child);
                break Err("required command failed".to_string());
            }
        }
    };
    // A normally exited leader is already reaped: do not kill its potentially
    // reused process-group ID. Timeout termination kills before wait/reaping.
    // A descendant can escape the child's process group while retaining a pipe.
    // Never join an unbounded reader/writer: the full capture deadline also
    // bounds pipe draining. Detached readers finish when the OS closes the FD.
    let remaining = || timeout.saturating_sub(start.elapsed());
    let status = match status {
        Ok(status) => status,
        Err(error) => {
            // Termination closes owned pipes; wait within the original deadline
            // for bounded readers/writers rather than leaving live worker threads.
            let _ = out_rx.recv_timeout(remaining());
            let _ = err_rx.recv_timeout(remaining());
            let _ = write_rx.recv_timeout(remaining());
            return Err(error);
        }
    };
    let bytes = out_rx
        .recv_timeout(remaining())
        .map_err(|_| "command output pipes did not close before deadline")??;
    err_rx
        .recv_timeout(remaining())
        .map_err(|_| "command error pipe did not close before deadline")?;
    write_rx
        .recv_timeout(remaining())
        .map_err(|_| "command input pipe did not close before deadline")?;
    Ok(Output {
        code: status.code().unwrap_or(1),
        stdout: bytes,
    })
}
fn terminate(child: &mut std::process::Child) {
    #[cfg(unix)]
    unsafe {
        libc::kill(-(child.id() as i32), libc::SIGKILL);
    }
    let _ = child.kill();
    let _ = child.wait();
}
pub fn find_tool(name: &str) -> Result<PathBuf> {
    let paths = std::env::var_os("PATH").ok_or("PATH is unavailable")?;
    let mut candidates: Vec<PathBuf> = std::env::split_paths(&paths)
        .map(|p| p.join(name))
        .collect();
    #[cfg(windows)]
    {
        candidates = std::env::split_paths(&paths)
            .map(|p| p.join(format!("{name}.exe")))
            .collect();
    }
    for candidate in candidates.drain(..) {
        if !candidate.is_file() || !executable(&candidate) {
            continue;
        }
        let path = candidate
            .canonicalize()
            .map_err(|_| "cannot resolve required tool".to_string())?;
        if name == "git" {
            // Only unwrap an identifiable guard installation or the original known Mac guard.
            if let Some(root) = path.parent().and_then(Path::parent) {
                let config = root.join("config.json");
                if config.is_file()
                    && (root.join("bin/gh-commit-guard").is_file()
                        || root.file_name() == Some(OsStr::new("git-identity-guard")))
                {
                    let value: serde_json::Value = serde_json::from_slice(
                        &std::fs::read(config).map_err(|_| "guard configuration unreadable")?,
                    )
                    .map_err(|_| "guard configuration invalid")?;
                    let native = value
                        .get("git")
                        .and_then(|v| v.as_str())
                        .ok_or("guard native Git missing")?;
                    let native = PathBuf::from(native)
                        .canonicalize()
                        .map_err(|_| "guard native Git unavailable")?;
                    if native == path || !executable(&native) {
                        return Err("recursive guard Git path".into());
                    }
                    return Ok(native);
                }
            }
            if let Ok(self_path) = std::env::current_exe()
                && self_path.canonicalize().ok() == Some(path.clone())
            {
                continue;
            }
        }
        return Ok(path);
    }
    Err(format!("required {name} executable is unavailable"))
}
pub fn executable(path: &Path) -> bool {
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
pub fn auto_config() -> Result<Option<Config>> {
    let exe = std::env::current_exe().map_err(|_| "cannot find guard executable")?;
    let parent = exe.parent().ok_or("guard executable has no parent")?;
    let config = parent.parent().unwrap_or(parent).join("config.json");
    if config.is_file() {
        Ok(Some(Config::read(&config)?))
    } else {
        Ok(None)
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    #[test]
    fn concurrent_output_does_not_deadlock() {
        let result=capture(Path::new("/bin/sh"),&["-c".into(),"i=0; while [ $i -lt 20000 ]; do printf 'output\n'; printf 'private-error\n' >&2; i=$((i+1)); done".into()],None,Duration::from_secs(10)).unwrap();
        assert_eq!(result.code, 0);
        assert_eq!(result.stdout.len(), 140000);
        assert!(!String::from_utf8_lossy(&result.stdout).contains("private-error"));
    }
    #[test]
    fn output_limit_reaps_the_producer() {
        let path =
            std::env::temp_dir().join(format!("commitguard-limit-pid-{}", std::process::id()));
        let result=capture_env_bounded(Path::new("/bin/sh"),
            &["-c".into(), "printf '%s\\n' \"$$\" > \"$1\"; while :; do printf 'private-payload-xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx\\n'; done".into(), "fixture".into(),path.to_str().unwrap().into()],
            None,Duration::from_secs(5),&[],1024);
        assert!(result.err().unwrap().contains("output exceeded safe limit"));
        let pid: i32 = std::fs::read_to_string(&path)
            .unwrap()
            .trim()
            .parse()
            .unwrap();
        assert_eq!(
            unsafe { libc::kill(pid, 0) },
            -1,
            "capture must reap its owned producer"
        );
        std::fs::remove_file(path).unwrap();
    }
    #[test]
    fn timeout_reaps_process_group() {
        let start = Instant::now();
        assert!(
            capture(
                Path::new("/bin/sh"),
                &["-c".into(), "/bin/sleep 30 & wait".into()],
                None,
                Duration::from_millis(100)
            )
            .is_err()
        );
        assert!(start.elapsed() < Duration::from_secs(3));
    }
}

#[cfg(all(test, unix))]
mod escaped_pipe_tests {
    use super::*;
    #[test]
    #[ignore = "subprocess fixture used only by escaped_pipe_deadline"]
    fn inherited_pipe_child() {
        unsafe {
            let pid = libc::fork();
            assert!(pid >= 0);
            if pid == 0 {
                libc::setsid();
                libc::sleep(2);
                libc::_exit(0);
            }
        }
    }
    #[test]
    fn escaped_pipe_deadline() {
        let start = Instant::now();
        let exe = std::env::current_exe().unwrap();
        let args = [
            "--exact",
            "util::escaped_pipe_tests::inherited_pipe_child",
            "--ignored",
            "--nocapture",
            "--test-threads=1",
        ]
        .map(str::to_string);
        assert!(capture(&exe, &args, None, Duration::from_millis(150)).is_err());
        assert!(start.elapsed() < Duration::from_secs(1));
    }
}
