//! One dispatcher shared by the canonical `commitguard` and the compatible
//! `gh-commit-guard` executables; only the reported name differs.
use crate::{Config, Result, core, hooks, install, util, wrapper};
use std::path::PathBuf;

fn run(name: &str) -> Result<i32> {
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
            println!("{name} {}", env!("CARGO_PKG_VERSION"));
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

/// Entry point for both executables; never returns.
pub fn main(name: &str) -> ! {
    // Setup copies the running image to both public names. argv[0] preserves
    // the invoked name even when a Formula exposes one through a symlink.
    // Guarded-Git dispatch still uses the actual executable path in run().
    let invoked = std::env::args_os().next().and_then(|p| {
        PathBuf::from(p)
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
    });
    let name = match invoked.as_deref() {
        Some("commitguard") => "commitguard",
        Some("gh-commit-guard") => "gh-commit-guard",
        _ => name,
    };
    match run(name) {
        Ok(code) => std::process::exit(code),
        Err(error) => {
            eprintln!("[gh-identity] refused: {error}");
            std::process::exit(1)
        }
    }
}
