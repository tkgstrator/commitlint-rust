#![cfg(unix)]
mod common;
use common::*;
use std::{fs, path::Path};

#[test]
fn stdin_needs_no_git_gh_or_authentication() {
    let f = Fixture::new();
    // Empty PATH: nothing at all can be resolved.
    let run = |args: &[&str], input: &[u8]| {
        f.run(
            Path::new(env!("CARGO_BIN_EXE_commitlint")),
            args,
            &f.root,
            &[],
            Some(input),
            "",
        )
    };
    accepted(run(&[], b"fix: standalone message\n"));
    accepted(run(&[], format!("fix: {}", "x".repeat(123)).as_bytes()));
    for message in [
        "fix: Uppercase",
        "fix: 日本語",
        "banana: invalid",
        &format!("fix: {}", "x".repeat(124)),
    ] {
        refused(run(&[], message.as_bytes()));
    }
    refused(run(&["--config", "relaxed.js"], b"invalid"));
    // Explicit files also need neither Git nor gh.
    let file = f.root.join("explicit file");
    fs::write(&file, "fix: explicit\n").unwrap();
    accepted(run(&["--edit", &file.display().to_string()], b""));
    fs::write(&file, "bad message\n").unwrap();
    refused(run(&["--edit", &file.display().to_string()], b""));
}

#[test]
fn malformed_guard_configuration_never_affects_standalone_lint() {
    let f = Fixture::new();
    // A guard-looking root beside the tools on PATH: bin/<tool> plus config.json,
    // bin/gh-commit-guard and a hostile/malformed configuration.
    let guard = f.root.join("git-identity-guard");
    fs::create_dir_all(guard.join("bin")).unwrap();
    fs::write(guard.join("config.json"), "{not json").unwrap();
    executable(&guard.join("bin/gh-commit-guard"), "#!/bin/sh\nexit 99\n");
    executable(&guard.join("bin/gh"), "#!/bin/sh\nexit 99\n");
    let path = guard.join("bin").display().to_string();
    let exe = Path::new(env!("CARGO_BIN_EXE_commitlint"));
    accepted(f.run(exe, &[], &f.root, &[], Some(b"fix: ok\n"), &path));
    refused(f.run(exe, &[], &f.root, &[], Some(b"nope\n"), &path));
    let file = f.root.join("message");
    fs::write(&file, "fix: ok\n").unwrap();
    accepted(f.run(
        exe,
        &["--edit", &file.display().to_string()],
        &f.root,
        &[],
        None,
        &path,
    ));
    // Configuration beside the executable must not matter either.
    let copy = f.root.join("copy/bin");
    fs::create_dir_all(&copy).unwrap();
    fs::copy(exe, copy.join("commitlint")).unwrap();
    fs::write(f.root.join("copy/config.json"), "\u{0}garbage").unwrap();
    accepted(f.run(
        &copy.join("commitlint"),
        &[],
        &f.root,
        &[],
        Some(b"fix: ok\n"),
        "",
    ));
}

#[test]
fn edit_and_range_check_all_messages_without_gh() {
    let f = Fixture::new();
    let repo = f.repo("range");
    let base = f.commit(&repo, "fix: base");
    let good = f.commit(&repo, "fix: own change");
    let file = f.root.join("explicit message file");
    fs::write(&file, "fix: explicit editor\n").unwrap();
    accepted(f.lint(&["--edit", &file.display().to_string()], &repo, None));
    accepted(f.lint(&["--edit"], &repo, None));
    accepted(f.lint(&["--from", &base, "--to", &good], &repo, None));
    f.commit(&repo, "not a conventional commit");
    let tip = f.commit(&repo, "fix: valid tip");
    refused(f.lint(&["--from", &base, "--to", &tip], &repo, None));
    refused(f.lint(&["--from", "--all", "--to", &tip], &repo, None));
}

#[test]
fn range_uses_the_first_git_on_path_verbatim_without_unwrapping_guard_config() {
    let f = Fixture::new();
    let repo = f.repo("guarded range");
    let base = f.commit(&repo, "fix: base");
    let tip = f.commit(&repo, "fix: tip");
    // Installed-guard layout: root/bin/git is the guard's Git entry point beside
    // root/bin/gh-commit-guard and a (here malformed) root/config.json. The
    // linter must run exactly that PATH entry and never read the configuration
    // or look for gh. The entry delegates to native Git, as the real guard does.
    let root = f.root.join("guard root");
    fs::create_dir_all(root.join("bin")).unwrap();
    let marker = f.root.join("shim ran");
    executable(
        &root.join("bin/git"),
        &format!(
            "#!/bin/sh\necho \"$*\" >> '{}'\nexec '{}' \"$@\"\n",
            marker.display(),
            f.git.display()
        ),
    );
    executable(&root.join("bin/gh-commit-guard"), "#!/bin/sh\nexit 98\n");
    fs::write(root.join("config.json"), "{broken").unwrap();
    let path = format!("{}:/nonexistent", root.join("bin").display());
    let exe = Path::new(env!("CARGO_BIN_EXE_commitlint"));
    accepted(f.run(
        exe,
        &["--from", &base, "--to", &tip],
        &repo,
        &[],
        None,
        &path,
    ));
    let log = fs::read_to_string(&marker).unwrap();
    assert!(
        log.contains("rev-list") && log.contains("cat-file"),
        "{log}"
    );
    // A guard whose shim cannot reach Git fails closed instead of passing.
    executable(&root.join("bin/git"), "#!/bin/sh\nexit 97\n");
    refused(f.run(
        exe,
        &["--from", &base, "--to", &tip],
        &repo,
        &[],
        None,
        &path,
    ));
}

#[test]
fn git_helpers_never_see_replace_objects_or_the_callers_replacement_flag() {
    let f = Fixture::new();
    let repo = f.repo("env probe");
    let base = f.commit(&repo, "fix: base");
    let tip = f.commit(&repo, "fix: tip");
    let probe = f.root.join("env probe bin");
    fs::create_dir_all(&probe).unwrap();
    let log = f.root.join("env log");
    executable(
        &probe.join("git"),
        &format!(
            "#!/bin/sh\necho \"${{GIT_NO_REPLACE_OBJECTS:-unset}}\" >> '{}'\nexec '{}' \"$@\"\n",
            log.display(),
            f.git.display()
        ),
    );
    let exe = Path::new(env!("CARGO_BIN_EXE_commitlint"));
    accepted(f.run(
        exe,
        &["--from", &base, "--to", &tip],
        &repo,
        &[("GIT_NO_REPLACE_OBJECTS", "0")],
        None,
        &probe.display().to_string(),
    ));
    let seen = fs::read_to_string(&log).unwrap();
    assert!(seen.lines().count() >= 4);
    assert!(seen.lines().all(|line| line == "1"), "{seen}");
}

#[test]
fn range_reads_ignore_replace_refs_and_hostile_helper_output() {
    let f = Fixture::new();
    let repo = f.repo("replace");
    let base = f.commit(&repo, "fix: base");
    let bad = f.commit(&repo, "not conventional");
    let good = f.commit(&repo, "fix: replacement text");
    f.raw(&["replace", &bad, &good], &repo);
    refused(f.lint(&["--from", &base, "--to", &bad], &repo, None));
}

#[test]
fn hung_git_capture_is_bounded_and_refused() {
    let f = Fixture::new();
    let slow = f.root.join("slow bin");
    fs::create_dir_all(&slow).unwrap();
    executable(&slow.join("git"), "#!/bin/sh\n/bin/sleep 60 &\nwait\n");
    let start = std::time::Instant::now();
    let result = commitlint_rust::git::capture(
        &slow.join("git"),
        &["rev-parse", "HEAD"],
        std::time::Duration::from_millis(100),
    );
    assert_eq!(result.err().as_deref(), Some("required command timed out"));
    assert!(start.elapsed() >= std::time::Duration::from_millis(100));
    assert!(start.elapsed() < std::time::Duration::from_secs(20));
}

#[test]
fn version_and_help_keep_legacy_semantics() {
    let f = Fixture::new();
    let version = f.lint(&["--version"], &f.root, None);
    assert_eq!(
        String::from_utf8_lossy(&version.stdout),
        format!("commitlint-rust {}\n", env!("CARGO_PKG_VERSION"))
    );
    accepted(f.lint(&["--help"], &f.root, None));
    refused(f.lint(&["--version", "extra"], &f.root, None));
}
