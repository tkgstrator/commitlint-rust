//! Message-only command line; never reads guard configuration or needs gh.
use crate::{Result, git, policy};
use std::{
    io::Read,
    path::{Path, PathBuf},
};

fn read_file(path: &Path) -> Result<Vec<u8>> {
    std::fs::read(path).map_err(|_| "cannot read commit message file".into())
}

pub fn run(args: Vec<String>) -> Result<()> {
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
        let path = match args.get(1) {
            Some(file) => PathBuf::from(file),
            None => git::commit_message_path(&git::find_git()?)?,
        };
        vec![read_file(&path)?]
    } else if args.len() == 4 && args[0] == "--from" && args[2] == "--to" {
        git::range_messages(&git::find_git()?, &args[1], &args[3])?
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
