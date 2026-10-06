#![cfg(unix)]
mod common;
use common::*;
use std::{fs, path::Path};
fn signing_fixture(f: &Fixture) -> (String, std::path::PathBuf) {
    let verifier = f.root.join("fixture verifier");
    let marker = f.root.join("verifier args");
    executable(
        &verifier,
        &format!(
            "#!/bin/sh\nprintf '%s\\n' \"$@\" > '{}'\nexit 23\n",
            marker.display()
        ),
    );
    let config = f.root.join("sign cfg.json");
    let value = serde_json::json!({"schema_version":1,"git":f.git,"gh":f.bin.join("gh"),"root":f.guard_root(),"verify_programs":{"openpgp":verifier,"ssh":verifier,"x509":verifier}});
    fs::write(&config, serde_json::to_vec(&value).unwrap()).unwrap();
    (config.display().to_string(), marker)
}
#[test]
fn signing_verification_preserves_configured_verifier_args_and_exit_without_a_repo() {
    let f = Fixture::new();
    let (config, marker) = signing_fixture(&f);
    fs::remove_file(f.bin.join("gh")).unwrap();
    for (kind, args) in [
        ("openpgp", vec!["--verify", "signature file", "data file"]),
        ("x509", vec!["--verify", "signature file"]),
        ("ssh", vec!["-Y", "verify", "-f", "allowed signers"]),
        (
            "ssh",
            vec!["-Y", "find-principals", "-f", "allowed signers"],
        ),
        ("ssh", vec!["-Y", "check-novalidate", "-n", "git"]),
    ] {
        let mut call = vec!["--config", &config, "sign", kind];
        call.extend(args.iter().copied());
        let output = f.cli(&call, &f.root, &[], None);
        assert_eq!(
            output.status.code(),
            Some(23),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(fs::read_to_string(&marker).unwrap(), args.join("\n") + "\n");
    }
}
#[test]
fn cryptographic_signing_flags_are_refused_before_the_verifier_runs() {
    let f = Fixture::new();
    let (config, marker) = signing_fixture(&f);
    for (kind, args) in [
        ("openpgp", vec!["-bs", "message"]),
        ("openpgp", vec!["--detach-sign", "message"]),
        ("openpgp", vec!["--verify", "--sign", "message"]),
        ("ssh", vec!["-Y", "sign", "-n", "git"]),
        ("x509", vec!["--sign", "message"]),
    ] {
        let mut call = vec!["--config", &config, "sign", kind];
        call.extend(args);
        refused(f.cli(&call, &f.root, &[], None));
        assert!(!marker.exists());
    }
}
fn commitlint(
    f: &Fixture,
    args: &[&str],
    cwd: &Path,
    input: Option<&[u8]>,
) -> std::process::Output {
    f.command(
        Path::new(env!("CARGO_BIN_EXE_commitlint")),
        args,
        cwd,
        &[],
        input,
    )
}
#[test]
fn standalone_commitlint_reads_stdin_without_git_gh_or_authentication() {
    let f = Fixture::new();
    fs::remove_file(f.bin.join("gh")).unwrap();
    fs::remove_file(f.bin.join("git")).unwrap();
    accepted(commitlint(
        &f,
        &[],
        &f.root,
        Some(b"fix: standalone message\n"),
    ));
    accepted(commitlint(
        &f,
        &[],
        &f.root,
        Some(format!("fix: {}", "x".repeat(123)).as_bytes()),
    ));
    for message in [
        "fix: Uppercase",
        "fix: 日本語",
        "banana: invalid",
        &format!("fix: {}", "x".repeat(124)),
    ] {
        refused(commitlint(&f, &[], &f.root, Some(message.as_bytes())));
    }
    refused(commitlint(
        &f,
        &["--config", "relaxed.js"],
        &f.root,
        Some(b"invalid"),
    ));
}
#[test]
fn standalone_commitlint_edit_and_range_check_all_messages_without_gh() {
    let f = Fixture::new();
    let repo = f.repo("commitlint range");
    let base = f.commit(&repo, "fix: base", &[]);
    let good = f.commit(&repo, "fix: own change", &[]);
    let file = f.root.join("explicit message file");
    fs::write(&file, "fix: explicit editor\n").unwrap();
    fs::remove_file(f.bin.join("gh")).unwrap();
    accepted(commitlint(
        &f,
        &["--edit", &file.display().to_string()],
        &repo,
        None,
    ));
    accepted(commitlint(&f, &["--edit"], &repo, None));
    accepted(commitlint(
        &f,
        &["--from", &base, "--to", &good],
        &repo,
        None,
    ));
    f.commit(&repo, "not a conventional commit", &[]);
    let tip = f.commit(&repo, "fix: valid tip", &[]);
    refused(commitlint(
        &f,
        &["--from", &base, "--to", &tip],
        &repo,
        None,
    ));
    refused(commitlint(
        &f,
        &["--from", "--all", "--to", &tip],
        &repo,
        None,
    ));
}
