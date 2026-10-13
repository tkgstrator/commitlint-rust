//! Message-only command line; never reads guard configuration or needs gh.
use crate::{Result, configured, git, policy, rules::EvaluationContext};
use std::{
    io::Read,
    path::{Path, PathBuf},
};

fn read_file(path: &Path) -> Result<Vec<u8>> {
    std::fs::read(path).map_err(|_| "cannot read commit message file".into())
}

fn discover_configuration() -> Result<Option<PathBuf>> {
    let mut directory = std::env::current_dir().map_err(|_| "cannot find current directory")?;
    loop {
        for name in [
            ".commitlintrc.json",
            ".commitlintrc.yaml",
            ".commitlintrc.yml",
        ] {
            let candidate = directory.join(name);
            match std::fs::symlink_metadata(&candidate) {
                Ok(_) => return Ok(Some(candidate)),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
                Err(_) => {
                    return Err(format!(
                        "cannot inspect configuration: {}",
                        candidate.display()
                    ));
                }
            }
        }
        if !directory.pop() {
            return Ok(None);
        }
    }
}

pub fn run(args: Vec<String>) -> Result<()> {
    match args.first().map(String::as_str) {
        Some("--version") if args.len() == 1 => {
            println!("commitlint-rust {}", env!("CARGO_PKG_VERSION"));
            return Ok(());
        }
        Some("--help") if args.len() == 1 => {
            println!(
                "Usage: commitlint [--verbose] [--config FILE] [--edit [FILE] | --from REF --to REF]\nConfiguration: discover .commitlintrc.json, .commitlintrc.yaml or .commitlintrc.yml from cwd to root.\nNearest directory wins; same-directory priority is JSON, YAML, then YML. --config skips discovery.\nWithout configuration: fixed ASCII/LF and entire-message 128-character policy.\nWith configuration: native Commitlint rules using JSON or YAML.\nNo arbitrary JS config/plugins; unsupported options fail."
            );
            return Ok(());
        }
        _ => (),
    }
    let mut verbose = false;
    let mut config_path = None;
    let mut offset = 0;
    while let Some(flag) = args.get(offset) {
        match flag.as_str() {
            "--verbose" if !verbose => {
                verbose = true;
                offset += 1;
            }
            "--config" if config_path.is_none() => {
                let path = args
                    .get(offset + 1)
                    .ok_or("--config requires a JSON or YAML file")?;
                if path.starts_with("--") {
                    return Err("--config requires a JSON or YAML file".into());
                }
                config_path = Some(PathBuf::from(path));
                offset += 2;
            }
            _ => break,
        }
    }
    let args = args[offset..].to_vec();
    let config_path = match config_path {
        Some(path) => Some(path),
        None => discover_configuration()?,
    };
    let configuration = config_path.map(|path| {
        if matches!(path.extension().and_then(|e| e.to_str()), Some("js" | "cjs" | "mjs" | "ts" | "cts" | "mts")) {
            return Err("unsupported commitlint options: JavaScript configuration requires the upstream runtime; use native JSON or YAML".into());
        }
        let text = std::fs::read_to_string(&path).map_err(|_| format!("cannot read configuration: {}", path.display()))?;
        let yaml = matches!(path.extension().and_then(|e| e.to_str()), Some("yaml" | "yml"));
        let edit = args.first().is_some_and(|mode| mode == "--edit");
        configured::parse_file_configuration(&text, yaml, edit)
    }).transpose()?;
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
        if let Some(config) = &configuration {
            let raw = std::str::from_utf8(&message)
                .map_err(|_| "configured message requires valid UTF-8")?;
            let outcome = configured::lint_configured(raw, config, &EvaluationContext::default())?;
            for item in outcome.errors.iter().chain(&outcome.warnings) {
                eprintln!("{:?} {}: {}", item.severity, item.name, item.message);
            }
            if !outcome.valid {
                return Err("commit message does not satisfy configured rules".into());
            }
            continue;
        }
        if verbose {
            let outcome = policy::lint_detailed(&message);
            for item in outcome.errors.iter().chain(&outcome.warnings) {
                eprintln!("{:?} {}: {}", item.severity, item.name, item.message);
            }
        }
        policy::lint_message(&message)?;
    }
    Ok(())
}
