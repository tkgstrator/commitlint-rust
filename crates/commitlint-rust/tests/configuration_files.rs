#![cfg(unix)]
mod common;
use common::*;
use std::{fs, os::unix::fs::symlink, path::Path, process::Output};

fn lint(f: &Fixture, cwd: &Path, args: &[&str], input: &[u8]) -> Output {
    f.run(
        Path::new(env!("CARGO_BIN_EXE_commitlint")),
        args,
        cwd,
        &[],
        Some(input),
        "",
    )
}

#[test]
fn discovery_uses_the_nearest_directory_before_format_priority() {
    let f = Fixture::new();
    let child = f.root.join("parent/child");
    fs::create_dir_all(&child).unwrap();
    fs::write(
        f.root.join(".commitlintrc.json"),
        r#"{"rules":{"type-enum":[2,"always",["fix"]]}}"#,
    )
    .unwrap();
    accepted(lint(&f, &child, &[], "fix: 日本語 Uppercase.".as_bytes()));
    refused(lint(&f, &child, &[], b"feat: change"));
    fs::write(
        child.join(".commitlintrc.yml"),
        "rules:\n  type-enum: [2, always, [feat]]\n",
    )
    .unwrap();
    accepted(lint(&f, &child, &[], "feat: 日本語 Uppercase.".as_bytes()));
    refused(lint(&f, &child, &[], b"fix: change"));
}

#[test]
fn same_directory_priority_is_json_then_yaml_then_yml() {
    let f = Fixture::new();
    for (name, text) in [
        (
            ".commitlintrc.json",
            r#"{"rules":{"type-enum":[2,"always",["fix"]]}}"#,
        ),
        (
            ".commitlintrc.yaml",
            "rules: {type-enum: [2, always, [feat]]}",
        ),
        (
            ".commitlintrc.yml",
            "rules: {type-enum: [2, always, [docs]]}",
        ),
    ] {
        fs::write(f.root.join(name), text).unwrap();
    }
    for (name, accepted_type, rejected_type) in [
        (".commitlintrc.json", "fix", "feat"),
        (".commitlintrc.yaml", "feat", "docs"),
        (".commitlintrc.yml", "docs", "fix"),
    ] {
        accepted(lint(
            &f,
            &f.root,
            &[],
            format!("{accepted_type}: 日本語").as_bytes(),
        ));
        refused(lint(
            &f,
            &f.root,
            &[],
            format!("{rejected_type}: change").as_bytes(),
        ));
        fs::remove_file(f.root.join(name)).unwrap();
    }
}

#[test]
fn invalid_or_unreadable_discovered_candidate_never_falls_back() {
    let f = Fixture::new();
    let child = f.root.join("child");
    fs::create_dir(&child).unwrap();
    fs::write(f.root.join(".commitlintrc.json"), "{}").unwrap();
    fs::write(child.join(".commitlintrc.yaml"), "{}").unwrap();
    let preferred = child.join(".commitlintrc.json");
    fs::write(&preferred, "{invalid json").unwrap();
    let output = lint(&f, &child, &[], b"fix: change");
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("invalid JSON configuration"));
    fs::remove_file(&preferred).unwrap();
    fs::create_dir(&preferred).unwrap();
    let output = lint(&f, &child, &[], b"fix: change");
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("cannot read configuration"));
    fs::remove_dir(&preferred).unwrap();
    symlink(child.join("missing"), &preferred).unwrap();
    let output = lint(&f, &child, &[], b"fix: change");
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("cannot read configuration"));
}

#[test]
fn explicit_configuration_skips_discovery_and_unknown_extensions_remain_json() {
    let f = Fixture::new();
    fs::write(f.root.join(".commitlintrc.json"), "{invalid").unwrap();
    let explicit = f.root.join("rules.custom");
    fs::write(&explicit, r#"{"rules":{"type-enum":[2,"always",["fix"]]}}"#).unwrap();
    accepted(lint(
        &f,
        &f.root,
        &["--config", explicit.to_str().unwrap()],
        "fix: 日本語".as_bytes(),
    ));
    let explicit = f.root.join("rules.yaml");
    fs::write(&explicit, "rules: {type-enum: [2, always, [fix]]}").unwrap();
    accepted(lint(
        &f,
        &f.root,
        &["--config", explicit.to_str().unwrap()],
        "fix: 日本語".as_bytes(),
    ));
    let output = lint(&f, &f.root, &["--config", "missing.js"], b"");
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("JavaScript configuration"));
}

#[test]
fn yaml_diagnostics_keep_rule_insertion_order_and_common_validation() {
    let f = Fixture::new();
    let path = f.root.join(".commitlintrc.yaml");
    fs::write(&path, "defaultIgnores: false\nrules:\n  subject-empty: [2, never]\n  type-empty: [2, never]\n  header-full-stop: [1, always, '.']\n").unwrap();
    let output = lint(&f, &f.root, &[], b"invalid header");
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    let subject = stderr.find("subject-empty").unwrap();
    let kind = stderr.find("type-empty").unwrap();
    let warning = stderr.find("header-full-stop").unwrap();
    assert!(subject < kind && kind < warning, "{stderr}");
    for text in [
        "plugins: []",
        "rules: {unknown-rule: [0]}",
        "rules: {type-enum: [2]}",
        "rules: {type-empty: [1.5, never]}",
        "rules: {type-enum: [2, always, null]}",
        "parserPreset: {parserOpts: {headerPattern: '.*'}}",
        "defaultIgnores: yes",
    ] {
        fs::write(&path, text).unwrap();
        let output = lint(&f, &f.root, &[], b"");
        assert!(
            !output.status.success(),
            "unsupported configuration passed: {text}"
        );
        assert!(String::from_utf8_lossy(&output.stderr).contains("commitlint:"));
    }
}

#[test]
fn yaml_rejects_unsupported_structures_instead_of_normalizing_them() {
    let f = Fixture::new();
    let path = f.root.join(".commitlintrc.yaml");
    for text in [
        "",
        "# only a comment\n",
        "null",
        "~",
        "---\n...\n",
        "rules: null",
        "rules: {}\n---\nrules: {}",
        "rules: {}\n---\n",
        "!custom {rules: {}}",
        "rules: {type-enum: [2, always, !custom [fix]]}",
        "rules: {}\nrules: {}",
        "rules: {type-empty: [2, never], type-empty: [0]}",
        "parserPreset: {parserOpts: {commentChar: '#', commentChar: ';'}}",
        "2: {}",
        "!!int 2: {}",
        "true: {}",
        "? [rules]\n: {}",
        "rules: {2: [0]}",
        "rules: {!!int 2: [0]}",
        "parserPreset: {parserOpts: {2: value}}",
        "rules: {scope-enum: [2, always, {2: value}]}",
        "rules: {<<: {type-empty: [2, never]}}",
        "rules: {!!merge '<<': {type-empty: [2, never]}}",
    ] {
        fs::write(&path, text).unwrap();
        let output = lint(&f, &f.root, &[], b"fix: change");
        assert!(!output.status.success(), "invalid YAML passed: {text:?}");
        assert!(
            String::from_utf8_lossy(&output.stderr).contains("invalid YAML configuration"),
            "{text:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

#[test]
fn yaml_aliases_have_the_same_rule_semantics_and_are_bounded() {
    let f = Fixture::new();
    let path = f.root.join(".commitlintrc.yaml");
    fs::write(
        &path,
        "rules:\n  type-enum: [2, always, &choices [fix]]\n  scope-enum: [2, always, *choices]\n",
    )
    .unwrap();
    accepted(lint(&f, &f.root, &[], "fix(fix): 日本語".as_bytes()));
    let output = lint(&f, &f.root, &[], b"fix(api): change");
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("scope-enum"));
    fs::write(&path, "rules:\n  type-enum: &cycle [2, always, *cycle]\n").unwrap();
    let output = lint(&f, &f.root, &[], b"fix: change");
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("invalid YAML configuration"));
}

#[test]
fn yaml_edit_defaults_to_hash_comments_and_respects_overrides() {
    let f = Fixture::new();
    let path = f.root.join(".commitlintrc.yaml");
    let message = f.root.join("message");
    let args = ["--edit", message.to_str().unwrap()];
    fs::write(&path, "rules: {body-max-line-length: [2, always, 10]}").unwrap();
    fs::write(&message, "fix: x\n\n# a very long ignored comment\n# ------------------------ >8 ------------------------\nunwanted scissors content").unwrap();
    accepted(lint(&f, &f.root, &args, b""));
    fs::write(&path, "parserPreset: {parserOpts: {commentChar: ';'}}\nrules: {body-max-line-length: [2, always, 10]}").unwrap();
    fs::write(&message, "fix: x\n\n; a very long ignored comment").unwrap();
    accepted(lint(&f, &f.root, &args, b""));
    fs::write(&path, "parserPreset: {parserOpts: {commentChar: ''}}\nrules: {body-max-line-length: [2, always, 10]}").unwrap();
    fs::write(&message, "fix: x\n\n# a very long retained body line").unwrap();
    let output = lint(&f, &f.root, &args, b"");
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("body-max-line-length"));
}

#[test]
fn help_and_version_do_not_discover_invalid_configuration() {
    let f = Fixture::new();
    fs::write(f.root.join(".commitlintrc.json"), "{invalid").unwrap();
    accepted(lint(&f, &f.root, &["--version"], b""));
    accepted(lint(&f, &f.root, &["--help"], b""));
}
