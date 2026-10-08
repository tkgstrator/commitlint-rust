#![cfg(unix)]
mod common;
use common::*;
use std::{fs, path::Path};

#[test]
fn json_configuration_uses_its_own_rules_without_legacy_rejection() {
    let f = Fixture::new();
    let config = f.root.join("commitlint.json");
    fs::write(&config, r#"{"rules":{"type-enum":[2,"always",["fix"]]}}"#).unwrap();
    let result = f.run(
        Path::new(env!("CARGO_BIN_EXE_commitlint")),
        &["--config", config.to_str().unwrap()],
        &f.root,
        &[],
        Some("fix: 日本語 Uppercase.\n".as_bytes()),
        "",
    );
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
}

#[test]
fn warning_only_configuration_succeeds_and_prints_diagnostics() {
    let f = Fixture::new();
    let config = f.root.join("commitlint.json");
    fs::write(
        &config,
        r#"{"rules":{"subject-full-stop":[1,"never","."]}}"#,
    )
    .unwrap();
    let result = f.run(
        Path::new(env!("CARGO_BIN_EXE_commitlint")),
        &["--config", config.to_str().unwrap()],
        &f.root,
        &[],
        Some(b"fix: subject."),
        "",
    );
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(String::from_utf8_lossy(&result.stderr).contains("subject-full-stop"));
}

#[test]
fn disabled_trailer_rule_needs_no_git() {
    let f = Fixture::new();
    let config = f.root.join("commitlint.json");
    fs::write(&config, r#"{"rules":{"trailer-exists":[0]}}"#).unwrap();
    let result = f.run(
        Path::new(env!("CARGO_BIN_EXE_commitlint")),
        &["--config", config.to_str().unwrap()],
        &f.root,
        &[],
        Some(b"fix: change"),
        "",
    );
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
}

#[test]
fn numeric_severity_spellings_and_ignore_failures_work_at_cli_boundary() {
    let f = Fixture::new();
    let config = f.root.join("commitlint.json");
    for severity in ["2.0", "2e0", "0.0", "0e0"] {
        fs::write(
            &config,
            format!(r#"{{"rules":{{"type-empty":[{severity},"never"]}}}}"#),
        )
        .unwrap();
        let result = f.run(
            Path::new(env!("CARGO_BIN_EXE_commitlint")),
            &["--config", config.to_str().unwrap()],
            &f.root,
            &[],
            Some(b"fix: a"),
            "",
        );
        assert!(
            result.status.success(),
            "severity {severity}: {}",
            String::from_utf8_lossy(&result.stderr)
        );
    }
    fs::write(&config, r#"{"rules":{"type-empty":[2,"never"]}}"#).unwrap();
    for raw in [
        "Merge x\r into main".to_owned(),
        format!("v1.2.3+{}", "a".repeat(250)),
    ] {
        let result = f.run(
            Path::new(env!("CARGO_BIN_EXE_commitlint")),
            &["--config", config.to_str().unwrap()],
            &f.root,
            &[],
            Some(raw.as_bytes()),
            "",
        );
        assert!(!result.status.success());
        assert!(String::from_utf8_lossy(&result.stderr).contains("type-empty"));
    }
}

#[test]
fn configured_edit_defaults_to_hash_comments_without_git() {
    let f = Fixture::new();
    let config = f.root.join("commitlint.json");
    let message = f.root.join("message");
    fs::write(
        &config,
        r#"{"rules":{"type-empty":[2,"never"],"body-max-line-length":[2,"always",10]}}"#,
    )
    .unwrap();
    for raw in [
        "fix: x\n\nbody line\n\n# a very long commit template comment\n# ------------------------ >8 ------------------------\nunwanted content beyond the scissors",
        "# comment-only message",
    ] {
        fs::write(&message, raw).unwrap();
        let output = f.run(
            Path::new(env!("CARGO_BIN_EXE_commitlint")),
            &[
                "--config",
                config.to_str().unwrap(),
                "--edit",
                message.to_str().unwrap(),
            ],
            &f.root,
            &[],
            None,
            "",
        );
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

#[test]
fn configured_edit_preserves_explicit_comment_character_or_disable() {
    let f = Fixture::new();
    let config = f.root.join("commitlint.json");
    let message = f.root.join("message");
    fs::write(&config, r#"{"parserPreset":{"parserOpts":{"commentChar":";"}},"rules":{"type-empty":[2,"never"],"body-max-line-length":[2,"always",10]}}"#).unwrap();
    fs::write(
        &message,
        "; ignored\nfix: x\n\nbody line\n; very long comment removed by the explicit character",
    )
    .unwrap();
    let output = f.run(
        Path::new(env!("CARGO_BIN_EXE_commitlint")),
        &[
            "--config",
            config.to_str().unwrap(),
            "--edit",
            message.to_str().unwrap(),
        ],
        &f.root,
        &[],
        None,
        "",
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    fs::write(&config, r#"{"parserPreset":{"parserOpts":{"commentChar":""}},"rules":{"body-max-line-length":[2,"always",10]}}"#).unwrap();
    fs::write(
        &message,
        "fix: x\n\n# a very long body line retained because comment filtering is disabled",
    )
    .unwrap();
    let output = f.run(
        Path::new(env!("CARGO_BIN_EXE_commitlint")),
        &[
            "--config",
            config.to_str().unwrap(),
            "--edit",
            message.to_str().unwrap(),
        ],
        &f.root,
        &[],
        None,
        "",
    );
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("body-max-line-length"));
}
