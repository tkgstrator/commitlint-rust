//! Explicit native setup with isolated-home support and transactional activation.
use crate::{Config, Result, util};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    env, fs,
    path::{Path, PathBuf},
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};
const SKILL: &str = include_str!("../resources/SKILL.md");
const NAME: &str = "gh-commit-identity";
const REFERENCES: &[(&str, &str)] = &[
    (
        "host-setup.md",
        include_str!("../resources/references/host-setup.md"),
    ),
    (
        "portable-setup.md",
        include_str!("../resources/references/portable-setup.md"),
    ),
    (
        "mac-hooks.md",
        include_str!("../resources/references/mac-hooks.md"),
    ),
];
const HOOKS: &[&str] = &[
    "applypatch-msg",
    "pre-applypatch",
    "post-applypatch",
    "pre-commit",
    "pre-merge-commit",
    "prepare-commit-msg",
    "commit-msg",
    "post-commit",
    "pre-rebase",
    "post-checkout",
    "post-merge",
    "pre-push",
    "pre-receive",
    "update",
    "proc-receive",
    "post-receive",
    "post-update",
    "reference-transaction",
    "push-to-checkout",
    "pre-auto-gc",
    "post-rewrite",
    "sendemail-validate",
    "fsmonitor-watchman",
    "p4-changelist",
    "p4-prepare-changelist",
    "p4-post-changelist",
    "p4-pre-submit",
    "post-index-change",
];
fn io<T>(result: std::io::Result<T>, action: &str) -> Result<T> {
    result.map_err(|_| format!("cannot {action}"))
}
fn absolute(path: &Path) -> Result<PathBuf> {
    Ok(if path.is_absolute() {
        path.into()
    } else {
        io(env::current_dir(), "read working directory")?.join(path)
    })
}
fn projected(path: &Path) -> Result<PathBuf> {
    projected_depth(path, 0)
}
fn projected_depth(path: &Path, depth: usize) -> Result<PathBuf> {
    if depth > 128 {
        return Err("setup path has too many symlink or parent levels".into());
    }
    if path.exists() {
        return io(fs::canonicalize(path), "resolve setup path");
    }
    if let Ok(meta) = fs::symlink_metadata(path) {
        if meta.file_type().is_symlink() {
            let target = io(fs::read_link(path), "resolve setup symlink")?;
            return projected_depth(
                &if target.is_absolute() {
                    target
                } else {
                    path.parent().unwrap_or(Path::new(".")).join(target)
                },
                depth + 1,
            );
        }
    }
    let parent = path.parent().ok_or("cannot resolve setup parent")?;
    let name = path.file_name().ok_or("cannot resolve setup filename")?;
    Ok(projected_depth(parent, depth + 1)?.join(name))
}
fn quote(value: &Path) -> Result<String> {
    let text = value.to_str().ok_or("setup path is not UTF-8")?;
    if text.contains(['\n', '\r']) {
        return Err("setup paths must not contain line breaks".into());
    }
    Ok(format!("'{}'", text.replace('\'', "'\\''")))
}
fn mode(path: &Path) -> u32 {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::metadata(path)
            .map(|m| m.permissions().mode() & 0o777)
            .unwrap_or(0o644)
    }
    #[cfg(not(unix))]
    {
        let _ = path;
        0o644
    }
}
fn set_mode(path: &Path, value: u32) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        io(
            fs::set_permissions(path, fs::Permissions::from_mode(value)),
            "set managed file permissions",
        )?;
    }
    #[cfg(not(unix))]
    {
        let _ = (path, value);
    }
    Ok(())
}
// Replace executable inodes atomically: Linux refuses writes to a running image.
fn replace_file(path: &Path, bytes: &[u8], permissions: u32) -> Result<()> {
    use std::io::Write;
    let parent = path.parent().ok_or("managed target lacks parent")?;
    io(fs::create_dir_all(parent), "create managed directory")?;
    let time = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| "system clock unavailable")?
        .as_nanos();
    let temporary = parent.join(format!(
        ".gh-commit-guard-{}-{time}.tmp",
        std::process::id()
    ));
    let result = (|| -> Result<()> {
        let mut file = io(
            fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&temporary),
            "create managed temporary file",
        )?;
        set_mode(&temporary, permissions)?;
        io(file.write_all(bytes), "write managed temporary file")?;
        io(file.sync_all(), "flush managed temporary file")?;
        io(fs::rename(&temporary, path), "activate managed file")?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}
struct Transaction {
    saved: BTreeMap<PathBuf, Option<(Vec<u8>, u32)>>,
    order: Vec<PathBuf>,
    backup: PathBuf,
}
impl Transaction {
    fn save(&mut self, path: &Path) -> Result<()> {
        let path = projected(&absolute(path)?)?;
        if self.saved.contains_key(&path) {
            return Ok(());
        }
        let previous = if path.exists() {
            if !path.is_file() {
                return Err("managed target is not a regular file".into());
            }
            Some((io(fs::read(&path), "read managed file")?, mode(&path)))
        } else {
            None
        };
        if let Some((bytes, _)) = &previous {
            io(fs::create_dir_all(&self.backup), "create backup directory")?;
            set_mode(&self.backup, 0o700)?;
            let dest = self.backup.join(format!("{}.bak", self.order.len()));
            io(fs::write(&dest, bytes), "save backup file")?;
            set_mode(&dest, 0o600)?;
        }
        self.order.push(path.clone());
        self.saved.insert(path, previous);
        if self.backup.exists() {
            let manifest = serde_json::to_vec_pretty(&self.order)
                .map_err(|_| "cannot serialize backup paths")?;
            let path = self.backup.join("paths.json");
            io(fs::write(&path, manifest), "write backup manifest")?;
            set_mode(&path, 0o600)?;
        }
        Ok(())
    }
    fn write(&mut self, path: &Path, bytes: &[u8], permissions: u32) -> Result<()> {
        let path = projected(&absolute(path)?)?;
        if fs::read(&path).ok().as_deref() == Some(bytes) && mode(&path) == permissions {
            return Ok(());
        }
        self.save(&path)?;
        replace_file(&path, bytes, permissions)
    }
    fn rollback(&self) -> bool {
        let mut success = true;
        for path in self.order.iter().rev() {
            match &self.saved[path] {
                Some((data, m)) => {
                    if replace_file(path, data, *m).is_err() {
                        success = false
                    }
                }
                None => {
                    if path.exists() && fs::remove_file(path).is_err() {
                        success = false
                    }
                }
            }
        }
        success
    }
}
struct Setup {
    home: PathBuf,
    repo: Option<PathBuf>,
    global: PathBuf,
}
impl Setup {
    fn command(&self, git: &Path, args: &[&str], local: bool, optional: bool) -> Result<Vec<u8>> {
        let mut cmd = Command::new(git);
        cmd.args(args)
            .env("HOME", &self.home)
            .env("GIT_CONFIG_GLOBAL", &self.global)
            .env_remove("GH_DEBUG");
        if local {
            cmd.current_dir(self.repo.as_ref().ok_or("missing repository")?);
        }
        let output = io(cmd.output(), "run native setup command")?;
        if !output.status.success() && !optional {
            return Err("native setup command failed; activation will be rolled back".into());
        }
        Ok(if output.status.success() {
            output.stdout
        } else {
            Vec::new()
        })
    }
}
fn load(path: &Path) -> Result<Value> {
    serde_json::from_slice(&io(fs::read(path), "read existing guard configuration")?)
        .map_err(|_| "existing guard configuration is malformed".into())
}
fn marked(original: &str, body: &str) -> Result<String> {
    let start = format!("<!-- {NAME} -->");
    let end = format!("<!-- /{NAME} -->");
    let block = format!("{start}\n{body}\n{end}");
    match (original.find(&start), original.find(&end)) {
        (Some(a), Some(b)) if b >= a => Ok(format!(
            "{}{}{}",
            &original[..a],
            block,
            &original[b + end.len()..]
        )),
        (None, None) => Ok(format!(
            "{}{}\n{}\n",
            original,
            if original.is_empty() || original.ends_with('\n') {
                ""
            } else {
                "\n"
            },
            block
        )),
        _ => Err("malformed managed instruction block".into()),
    }
}
fn mandate(tx: &mut Transaction, path: &Path, skill: &Path) -> Result<()> {
    let original = if path.exists() {
        io(fs::read_to_string(path), "read agent instructions")?
    } else {
        String::new()
    };
    let body = format!(
        "Before creating, rewriting, signing, pushing, or delegating Git commits, read and apply gh-commit-identity at {}. Its native checker is mandatory: printable ASCII English Conventional Commits, 128 entire-message characters maximum, and raw Author/Committer matching fresh human github.com gh authentication. Preserve recognized AI attribution. Use installed guarded Git; never bypass. Automatically correct only the skill's proven own unpublished safe subset, preserving trees and backup refs, and recheck before one authorized normal push retry. Missing tools/authentication or uncertain/public/foreign history stops. Verify actual delegated commits.",
        skill.display()
    );
    let text = marked(&original, &body)?;
    tx.write(path, text.as_bytes(), mode(path))
}

pub fn run(args: &[String]) -> Result<()> {
    let mut home = env::var_os("HOME")
        .or_else(|| env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .ok_or("cannot determine user home")?;
    let mut container = false;
    let mut repo = None;
    let mut skills_only = false;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--container" => container = true,
            "--skills-only" => skills_only = true,
            "--home" | "--repo" => {
                let option = &args[i];
                i += 1;
                let value = args
                    .get(i)
                    .filter(|v| !v.starts_with("--"))
                    .ok_or("missing setup option value")?;
                let path = absolute(Path::new(value))?;
                if option == "--home" {
                    home = path
                } else {
                    repo = Some(path)
                }
            }
            _ => return Err("unknown native setup argument".into()),
        }
        i += 1;
    }
    home = absolute(&home)?;
    if container && repo.is_none() {
        return Err("container setup requires --repo".into());
    }
    if cfg!(windows) && !skills_only {
        return Err("Windows supports --skills-only; full hooks require POSIX".into());
    }
    let global = env::var_os("GIT_CONFIG_GLOBAL")
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join(".gitconfig"));
    if !skills_only && global == Path::new("/dev/null") {
        return Err("persistent global Git configuration is required".into());
    }
    let setup = Setup {
        home: home.clone(),
        repo: repo.clone(),
        global,
    };
    let codex = env::var_os("CODEX_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            if container {
                repo.as_ref().unwrap().join(".codex")
            } else {
                home.join(".codex")
            }
        });
    let claude = if container {
        repo.as_ref().unwrap().join(".claude")
    } else {
        env::var_os("CLAUDE_CONFIG_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join(".claude"))
    };
    let codex = absolute(&codex)?;
    let claude = absolute(&claude)?;
    let codex_skill = codex.join("skills").join(NAME);
    let claude_skill = claude.join("skills").join(NAME);
    let root = home.join(".local/share").join(NAME).join("guard");
    let config_path = root.join("config.json");
    let executable = io(env::current_exe(), "locate native guard binary")?;
    let exe = if cfg!(windows) {
        "gh-commit-guard.exe"
    } else {
        "gh-commit-guard"
    };
    let git_exe = if cfg!(windows) { "git.exe" } else { "git" };
    let binary = io(fs::read(&executable), "read native guard binary")?;
    let mut git = util::find_tool("git")?;
    let gh = util::find_tool("gh")?;
    let mut previous: Option<Value> = None;
    if config_path.exists() {
        previous = Some(load(&config_path)?);
    } else if !container {
        // In containers ~/.codex can be a host mount. Never migrate its guard
        // configuration or native paths into the container-owned installation.
        let old = home.join(".codex/git-identity-guard/config.json");
        if old.exists() {
            previous = Some(load(&old)?);
        }
    }
    // Only the known native/legacy guard locations can be unwrapped and migrated.
    if let Some(old) = &previous {
        let old_git = old
            .get("git")
            .and_then(Value::as_str)
            .ok_or("incompatible existing guard Git backend")?;
        let old_root = old
            .get("root")
            .and_then(Value::as_str)
            .map(PathBuf::from)
            .or_else(|| {
                old.get("hooks")
                    .and_then(Value::as_str)
                    .and_then(|p| Path::new(p).parent().map(PathBuf::from))
            })
            .ok_or("incompatible existing guard root")?;
        if old_root != root && old_root != home.join(".codex/git-identity-guard") {
            return Err("refusing unrelated guard configuration".into());
        }
        if old.get("gh").and_then(Value::as_str).is_none() {
            return Err("incompatible existing guard authentication tool".into());
        }
        git = PathBuf::from(old_git);
    }
    if git == root.join("bin").join(git_exe)
        || git == home.join(".codex/git-identity-guard/bin/git")
    {
        return Err("native Git backend resolves to a guard shim".into());
    }
    setup.command(&git, &["--version"], false, false)?;
    setup.command(&gh, &["--version"], false, false)?;
    let mandate_files = if container {
        vec![
            repo.as_ref().unwrap().join("AGENTS.md"),
            repo.as_ref().unwrap().join("CLAUDE.md"),
        ]
    } else {
        vec![codex.join("AGENTS.md"), claude.join("CLAUDE.md")]
    };
    let mut shells = vec![".zshenv", ".zprofile", ".zshrc", ".profile", ".bashrc"];
    for login in [".bash_profile", ".bash_login"] {
        if home.join(login).exists() {
            shells.push(login);
        }
    }
    let local_config = if container && !skills_only {
        let raw = String::from_utf8_lossy(&setup.command(
            &git,
            &["rev-parse", "--git-path", "config"],
            true,
            false,
        )?)
        .trim()
        .to_owned();
        let path = PathBuf::from(raw);
        Some(if path.is_absolute() {
            path
        } else {
            repo.as_ref().unwrap().join(path)
        })
    } else {
        None
    };
    if container {
        if !projected(&codex)?.starts_with(projected(repo.as_ref().unwrap())?) {
            return Err("container CODEX_HOME must resolve inside the workspace".into());
        }
        if !repo.as_ref().unwrap().is_dir() {
            return Err("container repository does not exist".into());
        }
        let protected = [
            projected(&home.join(".codex"))?,
            projected(&home.join(".claude"))?,
        ];
        let mut targets = mandate_files.clone();
        targets.extend([
            root.clone(),
            setup.global.clone(),
            root.join("config.json"),
            root.join("bin").join(exe),
            root.join("bin").join(git_exe),
            home.join(".local/share").join(NAME).join("backups"),
        ]);
        if let Some(local) = &local_config {
            targets.push(local.clone());
        }
        targets.extend(HOOKS.iter().map(|hook| root.join("hooks").join(hook)));
        targets.extend(
            ["openpgp", "ssh", "x509"]
                .iter()
                .map(|kind| root.join(format!("sign-{kind}"))),
        );
        for skill in [&codex_skill, &claude_skill] {
            targets.extend(
                REFERENCES
                    .iter()
                    .map(|(file, _)| skill.join("references").join(file)),
            );
            targets.extend([
                skill.clone(),
                skill.join("SKILL.md"),
                skill.join("bin").join(exe),
                skill.join("agents/openai.yaml"),
                skill.join("scripts/check"),
            ]);
        }
        targets.extend(shells.iter().map(|file| home.join(file)));
        for target in targets {
            let actual = projected(&target)?;
            if protected.iter().any(|p| actual.starts_with(p)) {
                return Err(
                    "container target enters protected host-mounted agent configuration".into(),
                );
            }
        }
    }
    let time = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| "system clock unavailable")?
        .as_nanos();
    let mut tx = Transaction {
        saved: BTreeMap::new(),
        order: Vec::new(),
        backup: home
            .join(".local/share")
            .join(NAME)
            .join("backups")
            .join(format!("{time}-{}", std::process::id())),
    };
    let operation = (|| -> Result<()> {
        let skills: BTreeSet<PathBuf> = [codex_skill.clone(), claude_skill.clone()]
            .into_iter()
            .collect();
        for skill in skills {
            tx.write(&skill.join("SKILL.md"), SKILL.as_bytes(), 0o644)?;
            for (file, text) in REFERENCES {
                tx.write(&skill.join("references").join(file), text.as_bytes(), 0o644)?;
            }
            let launcher = format!(
                "#!/bin/sh\nexec {} \"$@\"\n",
                quote(&skill.join("bin").join(exe))?
            );
            tx.write(&skill.join("scripts/check"), launcher.as_bytes(), 0o755)?;
            tx.write(&skill.join("bin").join(exe), &binary, 0o755)?;
            tx.write(&skill.join("agents/openai.yaml"),b"interface:\n  display_name: Git Commit Identity\n  short_description: Check opted-in human gh commit policy\n",0o644)?;
        }
        mandate(&mut tx, &mandate_files[0], &codex_skill.join("SKILL.md"))?;
        mandate(&mut tx, &mandate_files[1], &claude_skill.join("SKILL.md"))?;
        if skills_only {
            return Ok(());
        }
        let hooks = root.join("hooks");
        let cli = root.join("bin").join(exe);
        let old_hooks = String::from_utf8_lossy(&setup.command(
            &git,
            &["config", "--global", "--path", "--get", "core.hooksPath"],
            false,
            true,
        )?)
        .trim()
        .to_owned();
        let mut verifiers: BTreeMap<String, String> = previous
            .as_ref()
            .and_then(|v| v.get("verify_programs"))
            .map(|v| serde_json::from_value(v.clone()).map_err(|_| "invalid verifier records"))
            .transpose()?
            .unwrap_or_default();
        for (kind, key, fallback) in [
            ("openpgp", "gpg.program", "gpg"),
            ("ssh", "gpg.ssh.program", "ssh-keygen"),
            ("x509", "gpg.x509.program", "gpgsm"),
        ] {
            if !verifiers.contains_key(kind) {
                let configured = String::from_utf8_lossy(&setup.command(
                    &git,
                    &["config", "--global", "--get", key],
                    false,
                    true,
                )?)
                .trim()
                .to_owned();
                let value = if configured.is_empty()
                    || configured == root.join(format!("sign-{kind}")).to_string_lossy()
                {
                    util::find_tool(fallback)
                        .map(|v| v.to_string_lossy().into_owned())
                        .unwrap_or_else(|_| fallback.into())
                } else {
                    configured
                };
                verifiers.insert(kind.into(), value);
            }
        }
        let saved_previous = previous
            .as_ref()
            .and_then(|v| v.get("previous_hooks"))
            .cloned()
            .unwrap_or(Value::Null);
        let prev = if !old_hooks.is_empty()
            && Path::new(&old_hooks) != hooks
            && Path::new(&old_hooks) != home.join(".codex/git-identity-guard/hooks")
        {
            Value::String(old_hooks)
        } else {
            saved_previous
        };
        let updated_config = json!({"schema_version":1,"git":git,"gh":gh,"root":root,"previous_hooks":prev,"repo_hooks":previous.as_ref().and_then(|v|v.get("repo_hooks")).cloned().unwrap_or(json!({})),"verify_programs":verifiers});
        let mut config_value = previous.clone().unwrap_or_else(|| json!({}));
        let existing = config_value
            .as_object_mut()
            .ok_or("existing guard configuration is not an object")?;
        for obsolete in ["bun", "python", "commitlint_script", "hooks"] {
            existing.remove(obsolete);
        }
        for (key, value) in updated_config
            .as_object()
            .ok_or("invalid updated guard configuration")?
        {
            existing.insert(key.clone(), value.clone());
        }
        let _: Config = serde_json::from_value(config_value.clone())
            .map_err(|_| "invalid native guard configuration")?;
        tx.write(&cli, &binary, 0o755)?;
        tx.write(&root.join("bin").join(git_exe), &binary, 0o755)?;
        tx.write(
            &config_path,
            &serde_json::to_vec_pretty(&config_value)
                .map_err(|_| "cannot serialize native guard configuration")?,
            0o600,
        )?;
        for hook in HOOKS {
            let script = format!(
                "#!/bin/sh\nexec {} --config {} hook {} \"$@\"\n",
                quote(&cli)?,
                quote(&config_path)?,
                hook
            );
            tx.write(&hooks.join(hook), script.as_bytes(), 0o755)?;
        }
        for kind in ["openpgp", "ssh", "x509"] {
            let script = format!(
                "#!/bin/sh\nexec {} --config {} sign {} \"$@\"\n",
                quote(&cli)?,
                quote(&config_path)?,
                kind
            );
            tx.write(&root.join(format!("sign-{kind}")), script.as_bytes(), 0o755)?;
        }
        let global_mode = mode(&setup.global);
        tx.save(&setup.global)?;
        for (key, value) in [
            ("core.hooksPath", hooks.to_string_lossy().into_owned()),
            ("commit.gpgsign", "false".into()),
            ("tag.gpgsign", "false".into()),
            (
                "gpg.program",
                root.join("sign-openpgp").to_string_lossy().into_owned(),
            ),
            (
                "gpg.openpgp.program",
                root.join("sign-openpgp").to_string_lossy().into_owned(),
            ),
            (
                "gpg.ssh.program",
                root.join("sign-ssh").to_string_lossy().into_owned(),
            ),
            (
                "gpg.x509.program",
                root.join("sign-x509").to_string_lossy().into_owned(),
            ),
            ("devflow.commit-policy", "strict".into()),
        ] {
            setup.command(&git, &["config", "--global", key, &value], false, false)?;
        }
        set_mode(&setup.global, global_mode)?;
        if let Some(local) = &local_config {
            let local_mode = mode(local);
            tx.save(local)?;
            setup.command(
                &git,
                &["config", "--local", "devflow.commit-policy", "strict"],
                true,
                false,
            )?;
            setup.command(
                &git,
                &[
                    "config",
                    "--local",
                    "devflow.commit-policy-root",
                    codex_skill.to_str().ok_or("skill path is not UTF-8")?,
                ],
                true,
                false,
            )?;
            set_mode(local, local_mode)?;
        }
        for file in shells {
            let path = home.join(file);
            let original = if path.exists() {
                io(fs::read_to_string(&path), "read shell configuration")?
            } else {
                String::new()
            };
            let begin = format!("# {NAME} PATH");
            let end = format!("# /{NAME} PATH");
            let block = format!(
                "{begin}\nexport PATH={}:\"$PATH\"\n{end}",
                quote(&root.join("bin"))?
            );
            let text = match (original.find(&begin), original.find(&end)) {
                (Some(a), Some(b)) if b >= a => {
                    format!("{}{}{}", &original[..a], block, &original[b + end.len()..])
                }
                (None, None) => format!(
                    "{}{}\n{}\n",
                    original,
                    if original.is_empty() || original.ends_with('\n') {
                        ""
                    } else {
                        "\n"
                    },
                    block
                ),
                _ => return Err("malformed managed shell block".into()),
            };
            tx.write(&path, text.as_bytes(), mode(&path))?;
        }
        setup.command(
            &cli,
            &[
                "--config",
                config_path
                    .to_str()
                    .ok_or("configuration path is not UTF-8")?,
                "version",
            ],
            false,
            false,
        )?;
        Ok(())
    })();
    if let Err(error) = operation {
        if !tx.rollback() {
            return Err(format!(
                "{error}; rollback could not restore every managed file; inspect saved backups"
            ));
        }
        return Err(error);
    }
    if skills_only {
        println!("Native commit skills installed; no global Git guard activated.")
    } else {
        println!("Native commit policy installed. Open a new terminal for guarded Git on PATH.")
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn preserve_instructions() {
        let body = marked("Existing instructions.\n", "Policy").unwrap();
        assert_eq!(marked(&body, "Policy").unwrap(), body);
        assert!(body.starts_with("Existing instructions."));
        assert!(marked("<!-- gh-commit-identity -->", "Policy").is_err());
    }
    #[test]
    fn shell_quote() {
        assert_eq!(quote(Path::new("a'b c")).unwrap(), "'a'\\''b c'");
        assert!(quote(Path::new("bad\npath")).is_err());
    }
}
