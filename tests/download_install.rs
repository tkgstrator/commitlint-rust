#![cfg(unix)]
use std::{
    collections::BTreeMap,
    fs,
    os::unix::fs::{PermissionsExt, symlink},
    path::{Path, PathBuf},
    process::{Command, Output},
    sync::atomic::{AtomicU64, Ordering},
};
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Download {
    root: PathBuf,
    home: PathBuf,
    tmp: PathBuf,
    payload: PathBuf,
    archive: PathBuf,
    sums: PathBuf,
    log: PathBuf,
    executed: PathBuf,
    env: BTreeMap<String, String>,
}
fn executable(path: &Path, text: &str) {
    fs::write(path, text).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
}
fn native(program: &str, args: &[&str], cwd: &Path) -> Output {
    let name = Path::new(program).file_name().unwrap().to_str().unwrap();
    Command::new(system_tool(name).expect("fixture requires standard OS tools"))
        .args(args)
        .current_dir(cwd)
        .output()
        .unwrap()
}
fn system_tool(name: &str) -> Option<PathBuf> {
    [
        PathBuf::from(format!("/usr/bin/{name}")),
        PathBuf::from(format!("/bin/{name}")),
    ]
    .into_iter()
    .find(|p| p.is_file())
}
impl Download {
    fn new(real_guard: bool) -> Self {
        let root = std::env::temp_dir().join(format!(
            "download installer spaces {} {}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&root).unwrap();
        let home = root.join("home");
        let tmp = root.join("private temp");
        let payload = root.join("archive payload");
        let bin = root.join("tools");
        for p in [&home, &tmp, &payload, &bin] {
            fs::create_dir(p).unwrap();
        }
        for name in [
            "git", "tar", "mktemp", "awk", "rm", "mkdir", "sort", "wc", "tr", "find", "chmod",
            "gzip",
        ] {
            let tool = system_tool(name).expect(name);
            symlink(tool, bin.join(name)).unwrap();
        }
        for name in ["sha256sum", "shasum"] {
            if let Some(tool) = system_tool(name) {
                symlink(tool, bin.join(name)).unwrap();
            }
        }
        executable(
            &bin.join("uname"),
            "#!/bin/sh\ncase \"$1\" in -s) echo \"${FIXTURE_OS:-Darwin}\";; -m) echo \"${FIXTURE_ARCH:-arm64}\";; esac\n",
        );
        let log = root.join("gh release call");
        let executed = root.join("payload executed");
        executable(
            &bin.join("gh"),
            "#!/bin/sh\ncase \"$1 $2\" in\n 'release download')\n printf '%s\\n' \"$@\" > \"$FIXTURE_DOWNLOAD_LOG\"\n printf '%s|%s' \"${GH_DEBUG:-unset}\" \"${DEBUG:-unset}\" > \"$FIXTURE_DEBUG_LOG\"\n [ \"${FIXTURE_NETWORK_FAIL:-0}\" != 1 ] || exit 21\n while [ $# -gt 0 ]; do if [ \"$1\" = --dir ]; then destination=$2; fi; shift; done\n /bin/cp \"$FIXTURE_ARCHIVE\" \"$destination/commitlint-rust-aarch64-apple-darwin.tar.gz\"\n [ \"${FIXTURE_MISSING_ASSET:-0}\" = 1 ] || /bin/cp \"$FIXTURE_SUMS\" \"$destination/SHA256SUMS\";;\n *) case \"$*\" in *--version*) echo 'gh fixture';; *) echo '{\"login\":\"tester\",\"id\":44,\"type\":\"User\"}';; esac;;\nesac\n",
        );
        if real_guard {
            fs::copy(
                env!("CARGO_BIN_EXE_gh-commit-guard"),
                payload.join("gh-commit-guard"),
            )
            .unwrap();
            fs::set_permissions(
                payload.join("gh-commit-guard"),
                fs::Permissions::from_mode(0o755),
            )
            .unwrap();
        } else {
            executable(
                &payload.join("gh-commit-guard"),
                "#!/bin/sh\nprintf '%s\\n' \"$@\" > \"$FIXTURE_EXECUTED\"\n: > \"$FIXTURE_MODE_PROBE\"\n",
            );
        }
        executable(&payload.join("commitlint"), "#!/bin/sh\nexit 0\n");
        fs::write(payload.join("LICENSE"), "MIT fixture\n").unwrap();
        fs::write(payload.join("README.md"), "Fixture native release\n").unwrap();
        let archive = root.join("commitlint-rust-aarch64-apple-darwin.tar.gz");
        let sums = root.join("SHA256SUMS");
        let mut env = BTreeMap::new();
        for (k, v) in [
            ("HOME", home.display().to_string()),
            ("CODEX_HOME", home.join("codex").display().to_string()),
            (
                "CLAUDE_CONFIG_DIR",
                home.join("claude").display().to_string(),
            ),
            (
                "GIT_CONFIG_GLOBAL",
                home.join(".gitconfig").display().to_string(),
            ),
            ("GIT_CONFIG_NOSYSTEM", "1".into()),
            ("PATH", bin.display().to_string()),
            ("TMPDIR", tmp.display().to_string()),
            ("FIXTURE_ARCHIVE", archive.display().to_string()),
            ("FIXTURE_SUMS", sums.display().to_string()),
            ("FIXTURE_DOWNLOAD_LOG", log.display().to_string()),
            (
                "FIXTURE_DEBUG_LOG",
                root.join("debug log").display().to_string(),
            ),
            ("FIXTURE_EXECUTED", executed.display().to_string()),
            (
                "FIXTURE_MODE_PROBE",
                root.join("umask probe").display().to_string(),
            ),
            ("GH_DEBUG", "api".into()),
            ("DEBUG", "secret trace".into()),
        ] {
            env.insert(k.into(), v);
        }
        fs::write(home.join(".gitconfig"), "[alias]\n  preserved = status\n").unwrap();
        let f = Self {
            root,
            home,
            tmp,
            payload,
            archive,
            sums,
            log,
            executed,
            env,
        };
        f.repack();
        f
    }
    fn repack(&self) {
        let output = native(
            "/usr/bin/tar",
            &[
                "-czf",
                &self.archive.display().to_string(),
                "-C",
                &self.payload.display().to_string(),
                "gh-commit-guard",
                "commitlint",
                "LICENSE",
                "README.md",
            ],
            &self.root,
        );
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        self.rehash();
    }
    fn rehash(&self) {
        let path = self.archive.display().to_string();
        let output = if system_tool("sha256sum").is_some() {
            native("sha256sum", &[&path], &self.root)
        } else {
            native("shasum", &["-a", "256", &path], &self.root)
        };
        assert!(output.status.success());
        let digest = String::from_utf8(output.stdout)
            .unwrap()
            .split_whitespace()
            .next()
            .unwrap()
            .to_owned();
        fs::write(
            &self.sums,
            format!("{digest}  commitlint-rust-aarch64-apple-darwin.tar.gz\n"),
        )
        .unwrap();
    }
    fn run(&self, args: &[&str], extra: &[(&str, &str)]) -> Output {
        let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("install.sh");
        Command::new("/bin/sh")
            .arg(script)
            .args(args)
            .env_clear()
            .envs(&self.env)
            .envs(extra.iter().copied())
            .current_dir(&self.root)
            .output()
            .unwrap()
    }
    fn cleanup_checked(&self) {
        assert_eq!(
            fs::read_dir(&self.tmp).unwrap().count(),
            0,
            "download temp folders leaked"
        );
    }
    fn refuses(&self, extra: &[(&str, &str)]) {
        let out = self.run(&[], extra);
        assert!(
            !out.status.success(),
            "unexpected success: {}",
            String::from_utf8_lossy(&out.stdout)
        );
        assert!(!self.executed.exists());
        self.cleanup_checked();
    }
}
impl Drop for Download {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}
#[test]
fn bootstrap_verifies_fixed_release_and_forwards_native_arguments_privately() {
    let f = Download::new(false);
    let args = ["--home", &f.home.display().to_string(), "--skills-only"];
    let result = f.run(&args, &[]);
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert_eq!(
        fs::read_to_string(&f.executed).unwrap(),
        format!("install\n--home\n{}\n--skills-only\n", f.home.display())
    );
    let log = fs::read_to_string(&f.log).unwrap();
    assert!(log.contains("v0.1.0\n"));
    assert!(log.contains("tkgstrator/commitlint-rust\n"));
    assert!(log.contains("commitlint-rust-aarch64-apple-darwin.tar.gz\n"));
    assert!(log.contains("SHA256SUMS\n"));
    assert_eq!(
        fs::read_to_string(f.root.join("debug log")).unwrap(),
        "unset|unset"
    );
    assert_eq!(
        fs::metadata(f.root.join("umask probe"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o600
    );
    f.cleanup_checked();
}
#[test]
fn bootstrap_real_native_install_is_isolated_and_skills_only_is_idempotent() {
    let f = Download::new(true);
    let home = f.home.display().to_string();
    let args = ["--home", &home, "--skills-only"];
    for _ in 0..2 {
        let result = f.run(&args, &[]);
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        f.cleanup_checked();
    }
    assert!(
        f.home
            .join("codex/skills/gh-commit-identity/SKILL.md")
            .is_file()
    );
    assert!(
        f.home
            .join("claude/skills/gh-commit-identity/SKILL.md")
            .is_file()
    );
    assert!(
        !f.home
            .join(".local/share/gh-commit-identity/guard/bin/git")
            .exists()
    );
    assert_eq!(
        fs::read_to_string(f.home.join(".gitconfig")).unwrap(),
        "[alias]\n  preserved = status\n"
    );
    let result = f.run(&["--home", &home, "--unknown-native-option"], &[]);
    assert!(!result.status.success());
    f.cleanup_checked();
    let result = f.run(&["--home", &home], &[]);
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(
        f.home
            .join(".local/share/gh-commit-identity/guard/bin/git")
            .is_file()
    );
    f.cleanup_checked();
}
#[test]
fn bootstrap_rejects_network_missing_assets_and_unknown_platform() {
    for extra in [
        [("FIXTURE_NETWORK_FAIL", "1")],
        [("FIXTURE_MISSING_ASSET", "1")],
        [("FIXTURE_OS", "UnknownOS")],
    ] {
        let f = Download::new(false);
        f.refuses(&extra);
        if extra[0].0 == "FIXTURE_OS" {
            assert!(!f.log.exists(), "unsupported platform downloaded assets");
        }
    }
}
#[test]
fn bootstrap_rejects_mismatch_duplicate_uppercase_and_missing_digest() {
    for mode in [
        "mismatch",
        "duplicate",
        "uppercase",
        "missing",
        "short",
        "extra-column",
    ] {
        let f = Download::new(false);
        let original = fs::read_to_string(&f.sums).unwrap();
        let changed = match mode {
            "mismatch" => format!(
                "{}  commitlint-rust-aarch64-apple-darwin.tar.gz\n",
                "0".repeat(64)
            ),
            "duplicate" => original.repeat(2),
            "uppercase" => format!(
                "{}  commitlint-rust-aarch64-apple-darwin.tar.gz\n",
                original.split_whitespace().next().unwrap().to_uppercase()
            ),
            "short" => format!(
                "{}  commitlint-rust-aarch64-apple-darwin.tar.gz\n",
                "a".repeat(63)
            ),
            "extra-column" => original.trim_end().to_string() + " unexpected-column\n",
            _ => "unrelated digest entry\n".into(),
        };
        fs::write(&f.sums, changed).unwrap();
        f.refuses(&[]);
    }
}
#[test]
fn bootstrap_rejects_extra_payload_and_symbolic_links() {
    for link in [false, true] {
        let f = Download::new(false);
        if link {
            fs::remove_file(f.payload.join("commitlint")).unwrap();
            symlink("/bin/true", f.payload.join("commitlint")).unwrap();
            f.repack();
        } else {
            fs::write(f.payload.join("extra.txt"), "extra\n").unwrap();
            let r = native(
                "/usr/bin/tar",
                &[
                    "-czf",
                    &f.archive.display().to_string(),
                    "-C",
                    &f.payload.display().to_string(),
                    "gh-commit-guard",
                    "commitlint",
                    "LICENSE",
                    "README.md",
                    "extra.txt",
                ],
                &f.root,
            );
            assert!(r.status.success());
            f.rehash();
        }
        f.refuses(&[]);
    }
}
#[test]
fn bootstrap_rejects_traversal_archive_before_any_extraction_or_execution() {
    let f = Download::new(false);
    let archive = f.archive.display().to_string();
    let payload = f.payload.display().to_string();
    let mut args = vec!["-czf", archive.as_str(), "-C", payload.as_str()];
    #[cfg(target_os = "macos")]
    args.extend(["-s", "|^README.md$|../README.md|"]);
    #[cfg(not(target_os = "macos"))]
    args.extend(["--transform=s,^README.md$,../README.md,"]);
    args.extend(["gh-commit-guard", "commitlint", "LICENSE", "README.md"]);
    let output = native("/usr/bin/tar", &args, &f.root);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    f.rehash();
    f.refuses(&[]);
    assert!(!f.tmp.join("README.md").exists());
}
#[test]
fn bootstrap_rejects_unsafe_release_tag_and_missing_tools_before_download() {
    for value in ["latest", "v0.1.0;touch injected"] {
        let f = Download::new(false);
        f.refuses(&[("COMMITLINT_RUST_VERSION", value)]);
        assert!(!f.log.exists());
    }
    for tool in ["git", "gh"] {
        let f = Download::new(false);
        fs::remove_file(Path::new(&f.env["PATH"]).join(tool)).unwrap();
        f.refuses(&[]);
        assert!(!f.log.exists());
    }
}
#[test]
fn bootstrap_checks_digest_command_failure_even_with_valid_digest_stdout() {
    let f = Download::new(false);
    let digest = fs::read_to_string(&f.sums)
        .unwrap()
        .split_whitespace()
        .next()
        .unwrap()
        .to_string();
    let fake = Path::new(&f.env["PATH"]).join("sha256sum");
    if fake.exists() {
        fs::remove_file(&fake).unwrap();
    }
    executable(
        &fake,
        &format!("#!/bin/sh\nprintf '%s\\n' '{digest}'\nexit 41\n"),
    );
    f.refuses(&[]);
}
