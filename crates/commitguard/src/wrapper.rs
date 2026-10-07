//! Guard the normal Git entry point while preserving Git's terminal and stdin.
use crate::{Config, Result, util};
use std::{env, path::PathBuf, process::Command, time::Duration};

fn probe(cfg: &Config, args: Vec<String>) -> Result<util::Output> {
    util::capture(&cfg.git, &args, None, Duration::from_secs(15))
}
fn split(args: &[String]) -> Result<usize> {
    let mut i = 0;
    while i < args.len() && args[i].starts_with('-') {
        let value = args[i].as_str();
        if [
            "-C",
            "-c",
            "--config-env",
            "--git-dir",
            "--work-tree",
            "--namespace",
        ]
        .contains(&value)
        {
            if i + 1 >= args.len() {
                return Err("missing Git global option argument".into());
            }
            i += 2;
        } else if [
            "-C",
            "-c",
            "--config-env=",
            "--git-dir=",
            "--work-tree=",
            "--namespace=",
        ]
        .iter()
        .any(|prefix| value.starts_with(prefix))
        {
            i += 1;
        } else if [
            "--no-pager",
            "--paginate",
            "--bare",
            "--no-replace-objects",
            "--no-optional-locks",
            "--literal-pathspecs",
            "--glob-pathspecs",
            "--noglob-pathspecs",
            "--icase-pathspecs",
            "--no-lazy-fetch",
            "--no-advice",
        ]
        .contains(&value)
        {
            i += 1;
        } else if [
            "--version",
            "--help",
            "--exec-path",
            "--html-path",
            "--man-path",
            "--info-path",
        ]
        .contains(&value)
        {
            return Ok(i);
        } else {
            return Err("unsupported Git global option; guard must remain active".into());
        }
    }
    Ok(i)
}
fn verify_options(command: &str, tail: &[String]) -> Result<()> {
    let mut i = 0;
    while i < tail.len() {
        let value = &tail[i];
        if value == "--" {
            break;
        }
        if value.starts_with("--no-v") && "--no-verify".starts_with(value) {
            return Err("--no-verify is forbidden by commit policy".into());
        }
        if command == "rebase" && (value.starts_with("-x") || value.starts_with("--exec")) {
            return Err("rebase exec commands are forbidden; use a checked reword workflow".into());
        }
        if command == "commit" {
            if value.starts_with('-') && !value.starts_with("--") {
                for (index, short) in value[1..].char_indices() {
                    if short == 'n' {
                        return Err("commit -n is forbidden by commit policy".into());
                    }
                    if short == 'u' {
                        let mode = &value[1 + index + short.len_utf8()..];
                        if !mode.is_empty() && !["no", "normal", "all"].contains(&mode) {
                            return Err("unsupported commit untracked-files mode".into());
                        }
                        break; // optional argument is attached only; never consume the next token
                    }
                    if "mFCct".contains(short) {
                        if index + short.len_utf8() == value[1..].len() {
                            i += 1;
                        }
                        break;
                    }
                }
            } else if [
                "--message",
                "--file",
                "--reuse-message",
                "--reedit-message",
                "--fixup",
                "--squash",
                "--author",
                "--date",
                "--cleanup",
                "--template",
                "--trailer",
                "--pathspec-from-file",
            ]
            .contains(&value.as_str())
            {
                i += 1;
            }
        }
        i += 1;
    }
    Ok(())
}

pub fn run(cfg: &Config, args: &[String]) -> Result<i32> {
    let index = split(args)?;
    let command = args.get(index).map(String::as_str).unwrap_or("");
    let tail = if index < args.len() {
        &args[index + 1..]
    } else {
        &[]
    };
    verify_options(command, tail)?;
    let prefix = &args[..index];
    if !command.is_empty() && !command.starts_with('-') {
        let builtins = probe(cfg, vec!["--list-cmds=builtins".into()])?;
        if builtins.code != 0 {
            return Err("cannot verify Git built-in commands".into());
        }
        if !String::from_utf8_lossy(&builtins.stdout)
            .split_whitespace()
            .any(|v| v == command)
        {
            let mut query = prefix.to_vec();
            query.extend(["config".into(), "--get".into(), format!("alias.{command}")]);
            if probe(cfg, query)?.code == 0 {
                return Err(
                    "Git aliases are forbidden by strict policy; use the built-in command".into(),
                );
            }
        }
    }
    let mut query = prefix.to_vec();
    query.extend([
        "config".into(),
        "--path".into(),
        "--get".into(),
        "core.hooksPath".into(),
    ]);
    let previous = probe(cfg, query)?;
    let mut native = Command::new(&cfg.git);
    native
        .args(prefix)
        .args(["-c", "help.autocorrect=0"])
        .args(["-c", &format!("core.hooksPath={}", cfg.hooks().display())]);
    for (kind, key) in [
        ("openpgp", "gpg.program"),
        ("openpgp", "gpg.openpgp.program"),
        ("ssh", "gpg.ssh.program"),
        ("x509", "gpg.x509.program"),
    ] {
        let signer = cfg.root.join(format!("sign-{kind}"));
        native.arg("-c").arg(format!("{key}={}", signer.display()));
    }
    native
        .args(&args[index..])
        .env_remove("GIT_IDENTITY_PREVIOUS_HOOKS_PATH");
    if crate::auth::is_strict() {
        native.env("COMMITGUARD_STRICT", "1");
    }
    if previous.code == 0 {
        let value = String::from_utf8_lossy(&previous.stdout).trim().to_owned();
        let expanded = if value == "~" {
            env::var_os("HOME").map(PathBuf::from).unwrap_or_default()
        } else if let Some(rest) = value.strip_prefix("~/") {
            env::var_os("HOME")
                .map(PathBuf::from)
                .unwrap_or_default()
                .join(rest)
        } else {
            PathBuf::from(&value)
        };
        if !value.is_empty() && expanded != cfg.hooks() {
            native.env("GIT_IDENTITY_PREVIOUS_HOOKS_PATH", value);
        }
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        let error = native.exec();
        Err(format!("cannot execute native Git: {error}"))
    }
    #[cfg(not(unix))]
    {
        let status = native
            .status()
            .map_err(|_| "cannot execute native Git".to_string())?;
        Ok(status.code().unwrap_or(1))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn noverify_and_values() {
        assert!(verify_options("commit", &["-nmvalue".into()]).is_err());
        assert!(verify_options("commit", &["-m-n".into()]).is_ok());
        assert!(verify_options("commit", &["-m".into(), "-n".into()]).is_ok());
        assert!(verify_options("commit", &["--message".into(), "--no-ver".into()]).is_ok());
        assert!(verify_options("push", &["-n".into()]).is_ok());
        assert!(verify_options("commit", &["--no-ver".into()]).is_err());
        assert!(verify_options("rebase", &["--exec=git status".into()]).is_err());
    }
    #[test]
    fn global_values() {
        assert_eq!(
            split(&["-C/path".into(), "-c".into(), "x=y".into(), "commit".into()]).unwrap(),
            3
        );
        assert!(split(&["--git-dir".into()]).is_err());
    }
}
