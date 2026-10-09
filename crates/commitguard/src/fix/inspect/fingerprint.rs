//! Ordered fingerprints of policy, executables, hooks, and shared state.
use super::{paths::absolute, shared_state::shared_state};
use crate::fix::Receipt;
use crate::{Config, Result, core, util};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    env, fs,
    io::Read,
    path::{Path, PathBuf},
};

fn feed(hash: &mut Sha256, bytes: &[u8]) {
    hash.update((bytes.len() as u64).to_be_bytes());
    hash.update(bytes);
}

fn file_fingerprint(hash: &mut Sha256, path: &Path, required: bool) -> Result<()> {
    feed(
        hash,
        path.to_str()
            .ok_or("non-UTF-8 fingerprint path")?
            .as_bytes(),
    );
    match fs::metadata(path) {
        Ok(metadata) => {
            if !metadata.is_file() {
                return Err("fingerprinted executable/config is not a regular file".into());
            }
            let canonical = absolute(path)?;
            feed(
                hash,
                canonical
                    .to_str()
                    .ok_or("non-UTF-8 executable path")?
                    .as_bytes(),
            );
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                feed(hash, &metadata.permissions().mode().to_be_bytes());
            }
            let mut file =
                fs::File::open(path).map_err(|_| "cannot fingerprint executable/config")?;
            let mut digest = Sha256::new();
            let mut buffer = [0u8; 65536];
            loop {
                let n = file
                    .read(&mut buffer)
                    .map_err(|_| "cannot fingerprint executable/config")?;
                if n == 0 {
                    break;
                }
                digest.update(&buffer[..n]);
            }
            feed(hash, &digest.finalize());
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound && !required => feed(hash, b"absent"),
        Err(_) => return Err("required executable/config fingerprint is unavailable".into()),
    }
    Ok(())
}

fn verifier_fingerprint(hash: &mut Sha256, program: &str) -> Result<()> {
    // Unsigned repair never invokes these optional signature verifiers. Bind
    // their configured value without requiring PATH resolution or installation.
    feed(hash, program.as_bytes());
    let path = Path::new(program);
    if path.is_absolute() && path.is_file() && util::executable(path) {
        file_fingerprint(hash, path, true)
    } else {
        feed(hash, b"optional unused signature verifier unavailable");
        Ok(())
    }
}

fn hooks_fingerprint(hash: &mut Sha256, directory: &Path) -> Result<()> {
    feed(
        hash,
        directory
            .to_str()
            .ok_or("non-UTF-8 hook directory")?
            .as_bytes(),
    );
    if !directory.exists() {
        feed(hash, b"absent hooks");
        return Ok(());
    }
    let mut files = Vec::new();
    for item in fs::read_dir(directory).map_err(|_| "cannot inspect hook chain")? {
        let item = item.map_err(|_| "cannot inspect hook chain")?;
        let path = item.path();
        // Git hook entry points are regular executable files. Hash helper files
        // too when regular, since hook scripts may source them.
        if path.is_file() {
            files.push(path);
        } else if fs::symlink_metadata(&path)
            .map_err(|_| "cannot inspect hook entry")?
            .file_type()
            .is_symlink()
        {
            return Err("unresolvable hook-chain symlink".into());
        } else if path.is_dir() {
            files.push(path);
        } else {
            return Err("unsupported special file in hook chain".into());
        }
    }
    files.sort();
    for file in files {
        if file.is_dir() {
            hooks_fingerprint(hash, &file)?;
        } else {
            file_fingerprint(hash, &file, true)?;
        }
    }
    Ok(())
}

pub(super) fn fingerprint(
    git: &Path,
    cfg: Option<&Config>,
    r: &Receipt,
    staging: Option<&Path>,
    backup: Option<&str>,
    promoted_tip: Option<&str>,
) -> Result<String> {
    let mut hash = Sha256::new();
    feed(&mut hash, b"commitguard fix inspection v1");
    let effective = core::query(
        git,
        &[
            "config".into(),
            "--null".into(),
            "--show-origin".into(),
            "--list".into(),
        ],
        None,
    )?;
    feed(&mut hash, &effective);
    // Effective values alone do not detect changes to comments/includes that
    // are currently shadowed; freeze every file Git reports as an origin too.
    let fields: Vec<_> = core::text(&effective)?.split('\0').collect();
    let mut files = BTreeSet::new();
    for pair in fields.chunks(2) {
        if let Some(origin) = pair.first().and_then(|s| s.strip_prefix("file:")) {
            files.insert(PathBuf::from(origin));
        }
    }
    for path in files {
        file_fingerprint(&mut hash, &path, true)?;
    }
    file_fingerprint(&mut hash, git, true)?;
    file_fingerprint(&mut hash, &core::tools(cfg)?.gh, true)?;
    file_fingerprint(
        &mut hash,
        &env::current_exe().map_err(|_| "cannot locate active guard executable")?,
        true,
    )?;
    if let Some(cfg) = cfg {
        feed(
            &mut hash,
            &serde_json::to_vec(cfg)
                .map_err(|_| "cannot serialize effective guard configuration")?,
        );
        file_fingerprint(&mut hash, &cfg.root.join("config.json"), false)?;
        let mut arguments = env::args_os().skip(1);
        if arguments.next().as_deref() == Some(std::ffi::OsStr::new("--config")) {
            let explicit = PathBuf::from(
                arguments
                    .next()
                    .ok_or("missing explicit guard config path")?,
            );
            file_fingerprint(&mut hash, &absolute(&explicit)?, true)?;
        }
        file_fingerprint(&mut hash, &cfg.cli(), true)?;
        file_fingerprint(&mut hash, &cfg.canonical_cli(), true)?;
        file_fingerprint(&mut hash, &cfg.root.join("bin/git"), true)?;
        for kind in ["openpgp", "ssh", "x509"] {
            file_fingerprint(&mut hash, &cfg.root.join(format!("sign-{kind}")), false)?;
        }
        for program in cfg.verify_programs.values() {
            verifier_fingerprint(&mut hash, program)?;
        }
        hooks_fingerprint(&mut hash, &cfg.hooks())?;
    } else {
        feed(
            &mut hash,
            b"portable: apply requires an explicitly installed guard",
        );
    }
    hooks_fingerprint(&mut hash, &r.hooks_dir)?;
    feed(
        &mut hash,
        &shared_state(git, r, staging, backup, promoted_tip)?,
    );
    // Relevant environment is part of effective policy and checkout behavior;
    // never store/log values (which can include credentials), only the digest.
    let mut environment: Vec<_> = env::vars_os()
        .filter(|(k, _)| {
            let k = k.to_string_lossy();
            k.starts_with("GIT_") || k == "HOME" || k == "XDG_CONFIG_HOME" || k == "PATH"
        })
        .collect();
    environment.sort_by(|a, b| a.0.cmp(&b.0));
    for (key, value) in environment {
        feed(
            &mut hash,
            key.to_str().ok_or("non-UTF-8 Git environment")?.as_bytes(),
        );
        feed(
            &mut hash,
            value
                .to_str()
                .ok_or("non-UTF-8 Git environment")?
                .as_bytes(),
        );
    }
    Ok(format!("{:x}", hash.finalize()))
}

#[cfg(all(test, unix))]
mod tests {
    use super::{Digest, Sha256, verifier_fingerprint};
    use crate::fix::inspect::test_support::TestDirectory;
    use std::{fs, os::unix::fs::PermissionsExt};

    #[test]
    fn optional_verifiers_do_not_add_unsigned_runtime_dependencies() {
        let root = TestDirectory::new();
        let digest = |program: &str| {
            let mut hash = Sha256::new();
            verifier_fingerprint(&mut hash, program).unwrap();
            hash.finalize()
        };
        for name in ["gpg", "ssh-keygen", "gpgsm"] {
            let _ = digest(name);
        }
        assert_ne!(digest("gpg"), digest("gpgsm"));
        let absent = root.0.join("absent-verifier");
        let _ = digest(absent.to_str().unwrap());
        let program = root.0.join("verifier");
        fs::write(&program, b"#!/bin/sh\nexit 0\n").unwrap();
        fs::set_permissions(&program, fs::Permissions::from_mode(0o700)).unwrap();
        let before = digest(program.to_str().unwrap());
        fs::write(&program, b"#!/bin/sh\nexit 1\n").unwrap();
        assert_ne!(before, digest(program.to_str().unwrap()));
    }
}
