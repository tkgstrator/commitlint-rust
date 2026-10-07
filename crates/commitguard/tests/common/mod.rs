#![allow(dead_code)]
#[cfg(unix)]
use std::os::unix::fs::{PermissionsExt, symlink};
use std::{
    collections::BTreeMap,
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Output, Stdio},
    sync::atomic::{AtomicU64, Ordering},
};
static NEXT: AtomicU64 = AtomicU64::new(0);
pub struct Fixture {
    pub root: PathBuf,
    pub home: PathBuf,
    pub bin: PathBuf,
    pub codex: PathBuf,
    pub global: PathBuf,
    pub git: PathBuf,
    pub env: BTreeMap<String, String>,
}
impl Fixture {
    pub fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "native guard spaces {} {}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&root).unwrap();
        let home = root.join("home with spaces");
        let bin = root.join("runtime only git gh");
        let codex = home.join("custom codex");
        let global = home.join(".gitconfig");
        for path in [&home, &bin, &codex] {
            fs::create_dir_all(path).unwrap();
        }
        let git =
            PathBuf::from(std::env::var("TEST_REAL_GIT").unwrap_or_else(|_| "/usr/bin/git".into()));
        assert!(git.is_file(), "set TEST_REAL_GIT to native Git");
        #[cfg(unix)]
        symlink(&git, bin.join("git")).unwrap();
        executable(
            &bin.join("gh"),
            r#"#!/bin/sh
case "$*" in
 '--version') echo 'gh fixture'; exit 0;;
 'auth token --hostname github.com')
   if [ "${FIXTURE_GH_FAIL:-0}" = 1 ]; then exit 1; fi
   printf '%s\n' "${GH_TOKEN:-${GITHUB_TOKEN:-fixture-token-${FIXTURE_GH_ACCOUNT:-tester}}}"; exit 0;;
 'config get user --host github.com') printf '%s\n' "${FIXTURE_GH_ACCOUNT:-tester}"; exit 0;;
 'api --hostname github.com user') ;;
 *) echo 'unexpected gh command' >&2; exit 78;;
esac
if [ "${FIXTURE_GH_FAIL:-0}" = 1 ]; then exit 1; fi
case "${FIXTURE_GH_ACCOUNT:-tester}" in
 bot) echo '{"login":"tester","id":44,"type":"Bot"}';;
 missing-type) echo '{"login":"tester","id":44}';;
 switched) echo '{"login":"changed","id":55,"type":"User"}';;
 *) echo '{"login":"tester","id":44,"type":"User"}';;
esac
"#,
        );
        fs::write(&global, "[alias]\n  preserved = status\n").unwrap();
        let mut env = BTreeMap::new();
        for (key, value) in [
            ("HOME", home.display().to_string()),
            ("CODEX_HOME", codex.display().to_string()),
            ("PATH", bin.display().to_string()),
            ("GIT_CONFIG_GLOBAL", global.display().to_string()),
            ("GIT_CONFIG_NOSYSTEM", "1".into()),
            ("GIT_TERMINAL_PROMPT", "0".into()),
            ("GIT_AUTHOR_NAME", "tester".into()),
            ("GIT_COMMITTER_NAME", "tester".into()),
            (
                "GIT_AUTHOR_EMAIL",
                "44+tester@users.noreply.github.com".into(),
            ),
            (
                "GIT_COMMITTER_EMAIL",
                "44+tester@users.noreply.github.com".into(),
            ),
        ] {
            env.insert(key.into(), value);
        }
        let fixture = Self {
            root,
            home,
            bin,
            codex,
            global,
            git,
            env,
        };
        accepted(fixture.canonical(&["--strict", "account"], &fixture.root, &[], None));
        fixture
    }
    pub fn command(
        &self,
        program: &Path,
        args: &[&str],
        cwd: &Path,
        extra: &[(&str, &str)],
        input: Option<&[u8]>,
    ) -> Output {
        let mut cmd = Command::new(program);
        cmd.args(args)
            .current_dir(cwd)
            .env_clear()
            .envs(&self.env)
            .envs(extra.iter().copied())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if input.is_some() {
            cmd.stdin(Stdio::piped());
        }
        let mut child = cmd.spawn().unwrap();
        if let Some(data) = input {
            child.stdin.take().unwrap().write_all(data).unwrap();
        }
        child.wait_with_output().unwrap()
    }
    pub fn raw(&self, args: &[&str], cwd: &Path, extra: &[(&str, &str)]) -> String {
        let r = self.command(&self.git, args, cwd, extra, None);
        assert!(r.status.success(), "{}", String::from_utf8_lossy(&r.stderr));
        String::from_utf8(r.stdout).unwrap().trim().into()
    }
    pub fn cli(
        &self,
        args: &[&str],
        cwd: &Path,
        extra: &[(&str, &str)],
        input: Option<&[u8]>,
    ) -> Output {
        self.command(legacy_exe(), args, cwd, extra, input)
    }
    pub fn repo(&self, name: &str) -> PathBuf {
        let repo = self.root.join(name);
        fs::create_dir_all(&repo).unwrap();
        self.raw(&["init", "-q", "-b", "main"], &repo, &[]);
        self.raw(&["config", "user.name", "tester"], &repo, &[]);
        self.raw(
            &["config", "user.email", "44+tester@users.noreply.github.com"],
            &repo,
            &[],
        );
        repo
    }
    pub fn commit(&self, repo: &Path, msg: &str, extra: &[(&str, &str)]) -> String {
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
                msg,
            ],
            repo,
            extra,
        );
        self.raw(&["rev-parse", "HEAD"], repo, &[])
    }
    pub fn bare(&self, name: &str) -> PathBuf {
        let path = self.root.join(name);
        fs::create_dir_all(&path).unwrap();
        self.raw(
            &["-c", "core.hooksPath=/dev/null", "init", "--bare", "-q"],
            &path,
            &[],
        );
        path
    }
    pub fn guard_root(&self) -> PathBuf {
        self.home.join(".local/share/gh-commit-identity/guard")
    }
    pub fn install(&self, extra: &[&str]) -> Output {
        self.install_with(legacy_exe(), extra)
    }
    pub fn canonical_install(&self, extra: &[&str]) -> Output {
        self.install_with(canonical_exe(), extra)
    }
    fn install_with(&self, exe: &Path, extra: &[&str]) -> Output {
        let home = self.home.display().to_string();
        let mut args = vec!["install", "--home", &home];
        args.extend(extra);
        let container_codex = if extra.contains(&"--container") {
            extra
                .windows(2)
                .find(|pair| pair[0] == "--repo")
                .map(|pair| Path::new(pair[1]).join(".codex").display().to_string())
        } else {
            None
        };
        let env = container_codex
            .as_ref()
            .map(|path| vec![("CODEX_HOME", path.as_str())])
            .unwrap_or_default();
        self.command(exe, &args, &self.root, &env, None)
    }
    pub fn guarded(&self, args: &[&str], repo: &Path, extra: &[(&str, &str)]) -> Output {
        self.command(&self.guard_root().join("bin/git"), args, repo, extra, None)
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}
pub fn executable(path: &Path, body: &str) {
    fs::write(path, body).unwrap();
    #[cfg(unix)]
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
    assert!(
        !output.status.success(),
        "unexpected success: {}",
        String::from_utf8_lossy(&output.stdout)
    );
}
pub fn canonical_exe() -> &'static Path {
    Path::new(env!("CARGO_BIN_EXE_commitguard"))
}
pub fn legacy_exe() -> &'static Path {
    Path::new(env!("CARGO_BIN_EXE_gh-commit-guard"))
}
impl Fixture {
    /// Run the canonical `commitguard` executable with the fixture environment.
    pub fn canonical(
        &self,
        args: &[&str],
        cwd: &Path,
        extra: &[(&str, &str)],
        input: Option<&[u8]>,
    ) -> Output {
        self.command(canonical_exe(), args, cwd, extra, input)
    }
    pub fn skill_dirs(&self) -> [PathBuf; 2] {
        [
            self.codex.join("skills/gh-commit-identity"),
            self.home.join(".claude/skills/gh-commit-identity"),
        ]
    }
}
