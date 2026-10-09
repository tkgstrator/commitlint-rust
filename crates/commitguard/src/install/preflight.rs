//! Argument parsing and read-only checks that run before any managed write.
use super::{
    assets::{HOOKS, NAME, REFERENCES, SIGNERS},
    command::Setup,
    paths::{absolute, io, projected},
};
use crate::{Result, util};
use serde_json::Value;
use std::{
    env, fs,
    path::{Path, PathBuf},
};

/// Everything activation needs, resolved without touching managed files.
pub(super) struct Context {
    pub(super) setup: Setup,
    pub(super) home: PathBuf,
    pub(super) skills_only: bool,
    pub(super) codex_skill: PathBuf,
    pub(super) claude_skill: PathBuf,
    pub(super) root: PathBuf,
    pub(super) config_path: PathBuf,
    pub(super) exe: &'static str,
    pub(super) canonical_exe: &'static str,
    pub(super) git_exe: &'static str,
    pub(super) binary: Vec<u8>,
    pub(super) git: PathBuf,
    pub(super) gh: PathBuf,
    pub(super) previous: Option<Value>,
    pub(super) mandate_files: Vec<PathBuf>,
    pub(super) shells: Vec<&'static str>,
    pub(super) local_config: Option<PathBuf>,
}

struct Options {
    home: PathBuf,
    container: bool,
    repo: Option<PathBuf>,
    skills_only: bool,
}

fn parse(args: &[String]) -> Result<Options> {
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
    Ok(Options {
        home,
        container,
        repo,
        skills_only,
    })
}

fn load(path: &Path) -> Result<Value> {
    serde_json::from_slice(&io(fs::read(path), "read existing guard configuration")?)
        .map_err(|_| "existing guard configuration is malformed".into())
}

/// Reads the existing guard record, then validates it and returns the Git it names.
fn previous_config(
    home: &Path,
    root: &Path,
    config_path: &Path,
    container: bool,
    git: &mut PathBuf,
) -> Result<Option<Value>> {
    let mut previous: Option<Value> = None;
    if config_path.exists() {
        previous = Some(load(config_path)?);
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
        *git = PathBuf::from(old_git);
    }
    Ok(previous)
}

/// Container installs must not write into host-mounted agent configuration.
fn check_container(ctx: &Context, repo: &Path, codex: &Path) -> Result<()> {
    let (home, root) = (&ctx.home, &ctx.root);
    if !projected(codex)?.starts_with(projected(repo)?) {
        return Err("container CODEX_HOME must resolve inside the workspace".into());
    }
    if !repo.is_dir() {
        return Err("container repository does not exist".into());
    }
    let protected = [
        projected(&home.join(".codex"))?,
        projected(&home.join(".claude"))?,
    ];
    let mut targets = ctx.mandate_files.clone();
    // Strict bootstrap writes credentials' fingerprints to private state.
    // XDG_STATE_HOME is inherited and may otherwise enter a host mount.
    targets.push(crate::auth::cache_directory_for_home(home)?);
    targets.extend([
        root.clone(),
        ctx.setup.global.clone(),
        root.join("config.json"),
        root.join("bin").join(ctx.exe),
        root.join("bin").join(ctx.canonical_exe),
        root.join("bin").join(ctx.git_exe),
        home.join(".local/share").join(NAME).join("backups"),
    ]);
    if let Some(local) = &ctx.local_config {
        targets.push(local.clone());
    }
    targets.extend(HOOKS.iter().map(|hook| root.join("hooks").join(hook)));
    targets.extend(SIGNERS.iter().map(|kind| root.join(format!("sign-{kind}"))));
    for skill in [&ctx.codex_skill, &ctx.claude_skill] {
        targets.extend(
            REFERENCES
                .iter()
                .map(|(file, _)| skill.join("references").join(file)),
        );
        targets.extend([
            skill.clone(),
            skill.join("SKILL.md"),
            skill.join("bin").join(ctx.exe),
            skill.join("bin").join(ctx.canonical_exe),
            skill.join("agents/openai.yaml"),
            skill.join("scripts/check"),
        ]);
    }
    targets.extend(ctx.shells.iter().map(|file| home.join(file)));
    for target in targets {
        let actual = projected(&target)?;
        if protected.iter().any(|p| actual.starts_with(p)) {
            return Err(
                "container target enters protected host-mounted agent configuration".into(),
            );
        }
    }
    Ok(())
}

pub(super) fn prepare(args: &[String]) -> Result<Context> {
    let Options {
        home,
        container,
        repo,
        skills_only,
    } = parse(args)?;
    let home = absolute(&home)?;
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
    let canonical_exe = if cfg!(windows) {
        "commitguard.exe"
    } else {
        "commitguard"
    };
    let git_exe = if cfg!(windows) { "git.exe" } else { "git" };
    let binary = io(fs::read(&executable), "read native guard binary")?;
    let mut git = util::find_tool("git")?;
    let gh = util::find_tool("gh")?;
    let previous = previous_config(&home, &root, &config_path, container, &mut git)?;
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
    let ctx = Context {
        setup,
        home,
        skills_only,
        codex_skill,
        claude_skill,
        root,
        config_path,
        exe,
        canonical_exe,
        git_exe,
        binary,
        git,
        gh,
        previous,
        mandate_files,
        shells,
        local_config,
    };
    if container {
        check_container(&ctx, repo.as_ref().unwrap(), &codex)?;
    }
    Ok(ctx)
}
