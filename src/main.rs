use gh_commit_guard::{Config, Result, core, hooks, install, util, wrapper};
use std::path::PathBuf;
fn run() -> Result<i32> {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let stem = std::env::current_exe()
        .ok()
        .and_then(|p| p.file_stem().map(|s| s.to_string_lossy().into_owned()))
        .unwrap_or_default();
    let mut config = util::auto_config()?;
    if stem == "git" {
        return wrapper::run(config.as_ref().ok_or("guard configuration missing")?, &args);
    }
    if args.first().map(String::as_str) == Some("--config") {
        if args.len() < 3 {
            return Err("--config requires a file and command".into());
        }
        config = Some(Config::read(&PathBuf::from(&args[1]))?);
        args.drain(..2);
    }
    let mode=args.first().map(String::as_str).ok_or("usage: gh-commit-guard <account|identity|message|commits|push|pre-push|hook|sign|git|install|version>")?;
    match mode {
        "version" => {
            println!("gh-commit-guard {}", env!("CARGO_PKG_VERSION"));
            Ok(0)
        }
        "install" => {
            install::run(&args[1..])?;
            Ok(0)
        }
        "git" => wrapper::run(
            config
                .as_ref()
                .ok_or("install guard before using Git proxy")?,
            &args[1..],
        ),
        "hook" => {
            let hook = args.get(1).ok_or("hook name required")?;
            hooks::run(
                config.as_ref().ok_or("hook configuration missing")?,
                hook,
                &args[2..],
            )
        }
        "sign" => {
            let kind = args.get(1).ok_or("signing format required")?;
            hooks::run(
                config.as_ref().ok_or("sign configuration missing")?,
                &format!("sign-{kind}"),
                &args[2..],
            )
        }
        "account" | "identity" | "message" | "commits" | "push" | "pre-push" => {
            core::check(mode, &args[1..], config.as_ref())?;
            Ok(0)
        }
        _ => Err("unknown guard command".into()),
    }
}
fn main() {
    match run() {
        Ok(code) => std::process::exit(code),
        Err(error) => {
            eprintln!("[gh-identity] refused: {error}");
            std::process::exit(1)
        }
    }
}
