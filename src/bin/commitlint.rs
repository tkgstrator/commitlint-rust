//! Message-only entry point; never requires gh authentication.
use gh_commit_guard::{Result, policy, util};
use std::{
    io::Read,
    path::{Path, PathBuf},
    time::Duration,
};
fn read_file(path: &Path) -> Result<Vec<u8>> {
    std::fs::read(path).map_err(|_| "cannot read commit message file".into())
}
fn run() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("--version") if args.len() == 1 => {
            println!("commitlint-rust {}", env!("CARGO_PKG_VERSION"));
            return Ok(());
        }
        Some("--help") if args.len() == 1 => {
            println!(
                "Usage: commitlint [--edit [FILE] | --from REF --to REF]\nSelected Commitlint conventional rules; strict ASCII/LF and entire-message 128-character policy.\nNo arbitrary JS config/plugins; unsupported options fail."
            );
            return Ok(());
        }
        _ => (),
    }
    let messages = if args.is_empty() {
        let mut bytes = Vec::new();
        std::io::stdin()
            .read_to_end(&mut bytes)
            .map_err(|_| "cannot read message stdin")?;
        vec![bytes]
    } else if args[0] == "--edit" && args.len() <= 2 {
        let path = if let Some(file) = args.get(1) {
            PathBuf::from(file)
        } else {
            let git = util::find_tool("git")?;
            let result = util::capture(
                &git,
                &[
                    "rev-parse".into(),
                    "--git-path".into(),
                    "COMMIT_EDITMSG".into(),
                ],
                None,
                Duration::from_secs(15),
            )?;
            if result.code != 0 {
                return Err("cannot locate Git commit message".into());
            }
            PathBuf::from(
                std::str::from_utf8(&result.stdout)
                    .map_err(|_| "invalid message path encoding")?
                    .trim(),
            )
        };
        vec![read_file(&path)?]
    } else if args.len() == 4 && args[0] == "--from" && args[2] == "--to" {
        let git = util::find_tool("git")?;
        let mut resolved = Vec::new();
        for reference in [&args[1], &args[3]] {
            let output = util::capture(
                &git,
                &[
                    "rev-parse".into(),
                    "--verify".into(),
                    "--end-of-options".into(),
                    format!("{reference}^{{commit}}"),
                ],
                None,
                Duration::from_secs(15),
            )?;
            if output.code != 0 {
                return Err("cannot resolve commit range".into());
            }
            let oid = std::str::from_utf8(&output.stdout)
                .map_err(|_| "invalid commit range")?
                .trim()
                .to_string();
            if !gh_commit_guard::core::oid_valid(&oid) {
                return Err("invalid commit range".into());
            }
            resolved.push(oid)
        }
        let list = util::capture(
            &git,
            &[
                "rev-list".into(),
                format!("{}..{}", resolved[0], resolved[1]),
            ],
            None,
            Duration::from_secs(30),
        )?;
        if list.code != 0 {
            return Err("cannot enumerate commit range".into());
        }
        let mut messages = Vec::new();
        for oid in String::from_utf8(list.stdout)
            .map_err(|_| "invalid revision response")?
            .lines()
        {
            if !gh_commit_guard::core::oid_valid(oid) {
                return Err("invalid revision response".into());
            }
            let raw = util::capture(
                &git,
                &["cat-file".into(), "commit".into(), oid.into()],
                None,
                Duration::from_secs(15),
            )?;
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
        messages
    } else {
        return Err(
            "unsupported commitlint options; use --help for the supported interface".into(),
        );
    };
    for message in messages {
        policy::lint_message(&message)?;
    }
    Ok(())
}
fn main() {
    if let Err(error) = run() {
        eprintln!("commitlint: {error}");
        std::process::exit(1)
    }
}
