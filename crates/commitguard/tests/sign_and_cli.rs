#![cfg(unix)]
mod common;
use common::*;
use std::fs;
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
