//! Small standalone fixture: native Git only, never gh or guard configuration.
#![allow(dead_code)]
use std::{
    fs,
    io::Write,
    os::unix::fs::{PermissionsExt, symlink},
    path::{Path, PathBuf},
    process::{Command, Output, Stdio},
    sync::atomic::{AtomicU64, Ordering},
};
static NEXT: AtomicU64 = AtomicU64::new(0);
pub struct Fixture {
    pub root: PathBuf,
    pub bin: PathBuf,
    pub git: PathBuf,
}
impl Fixture {
    pub fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "commitlint fixture {} {}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let bin = root.join("only git");
        fs::create_dir_all(&bin).unwrap();
        let git =
            PathBuf::from(std::env::var("TEST_REAL_GIT").unwrap_or_else(|_| "/usr/bin/git".into()));
        assert!(git.is_file(), "set TEST_REAL_GIT to native Git");
        symlink(&git, bin.join("git")).unwrap();
        Self { root, bin, git }
    }
    pub fn run(
        &self,
        program: &Path,
        args: &[&str],
        cwd: &Path,
        extra: &[(&str, &str)],
        input: Option<&[u8]>,
        path: &str,
    ) -> Output {
        let mut cmd = Command::new(program);
        cmd.args(args)
            .current_dir(cwd)
            .env_clear()
            .env("HOME", &self.root)
            .env("PATH", path)
            .env("GIT_CONFIG_GLOBAL", self.root.join("gitconfig"))
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_AUTHOR_NAME", "tester")
            .env("GIT_AUTHOR_EMAIL", "tester@example.com")
            .env("GIT_COMMITTER_NAME", "tester")
            .env("GIT_COMMITTER_EMAIL", "tester@example.com")
            .envs(extra.iter().copied())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child = cmd.spawn().unwrap();
        let mut stdin = child.stdin.take().unwrap();
        if let Some(data) = input {
            stdin.write_all(data).unwrap();
        }
        drop(stdin);
        child.wait_with_output().unwrap()
    }
    pub fn git_path(&self) -> String {
        self.bin.display().to_string()
    }
    /// Run the standalone linter with Git (only) on PATH.
    pub fn lint(&self, args: &[&str], cwd: &Path, input: Option<&[u8]>) -> Output {
        self.run(
            Path::new(env!("CARGO_BIN_EXE_commitlint")),
            args,
            cwd,
            &[],
            input,
            &self.git_path(),
        )
    }
    pub fn raw(&self, args: &[&str], cwd: &Path) -> String {
        let out = self.run(&self.git, args, cwd, &[], None, &self.git_path());
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8(out.stdout).unwrap().trim().into()
    }
    pub fn repo(&self, name: &str) -> PathBuf {
        let repo = self.root.join(name);
        fs::create_dir_all(&repo).unwrap();
        self.raw(&["init", "-q", "-b", "main"], &repo);
        repo
    }
    pub fn commit(&self, repo: &Path, message: &str) -> String {
        self.raw(
            &[
                "-c",
                "core.hooksPath=/dev/null",
                "-c",
                "commit.gpgsign=false",
                "commit",
                "-q",
                "--allow-empty",
                "-m",
                message,
            ],
            repo,
        );
        self.raw(&["rev-parse", "HEAD"], repo)
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}
pub fn executable(path: &Path, body: &str) {
    fs::write(path, body).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
}
pub fn accepted(output: Output) {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}
pub fn refused(output: Output) {
    assert!(!output.status.success(), "unexpected success");
}
