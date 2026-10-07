#![cfg(unix)]
mod common;
use common::*;
use std::{fs, os::unix::fs::PermissionsExt};

fn stdout(output: &std::process::Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}
fn stderr(output: &std::process::Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

#[test]
fn canonical_dispatcher_reports_commitguard_and_legacy_keeps_its_name() {
    let f = Fixture::new();
    let version = env!("CARGO_PKG_VERSION");
    let canonical = f.canonical(&["version"], &f.root, &[], None);
    assert!(canonical.status.success(), "{}", stderr(&canonical));
    assert_eq!(stdout(&canonical), format!("commitguard {version}\n"));
    let legacy = f.cli(&["version"], &f.root, &[], None);
    assert_eq!(stdout(&legacy), format!("gh-commit-guard {version}\n"));
}

#[test]
fn canonical_and_legacy_share_one_dispatcher_and_refusal_convention() {
    let f = Fixture::new();
    let repo = f.repo("parity");
    let commit = f.commit(&repo, "fix: parity", &[]);
    for args in [
        vec!["identity"],
        vec!["account"],
        vec!["commits", "HEAD"],
        vec!["unknown-mode"],
        vec!["identity", "extra"],
        vec![],
    ] {
        let canonical = f.canonical(&args, &repo, &[], None);
        let legacy = f.cli(&args, &repo, &[], None);
        assert_eq!(canonical.status.code(), legacy.status.code(), "{args:?}");
        assert_eq!(stdout(&canonical), stdout(&legacy), "{args:?}");
        assert_eq!(stderr(&canonical), stderr(&legacy), "{args:?}");
    }
    assert!(stdout(&f.canonical(&["commits", "HEAD"], &repo, &[], None)).contains(&commit));
    // Gh failures stay fail-closed through the canonical entry point.
    refused(f.canonical(&["identity"], &repo, &[("FIXTURE_GH_FAIL", "1")], None));
    refused(f.canonical(&["unknown-mode"], &repo, &[], None));
}

#[test]
fn install_through_canonical_binary_writes_both_names_everywhere() {
    let f = Fixture::new();
    accepted(f.canonical_install(&[]));
    let bin = f.guard_root().join("bin");
    let legacy = fs::read(bin.join("gh-commit-guard")).unwrap();
    let canonical = fs::read(bin.join("commitguard")).unwrap();
    assert_eq!(legacy, canonical);
    assert_eq!(fs::read(bin.join("git")).unwrap(), legacy);
    for skill in f.skill_dirs() {
        for name in ["gh-commit-guard", "commitguard"] {
            let path = skill.join("bin").join(name);
            assert_eq!(fs::read(&path).unwrap(), legacy, "{}", path.display());
            assert_eq!(
                fs::metadata(&path).unwrap().permissions().mode() & 0o777,
                0o755
            );
        }
        // Generated checkers keep their legacy path.
        let check = fs::read_to_string(skill.join("scripts/check")).unwrap();
        assert!(check.contains("bin/gh-commit-guard"), "{check}");
        assert!(!check.contains("bin/commitguard"), "{check}");
    }
    let config = f.guard_root().join("config.json");
    let hook = fs::read_to_string(f.guard_root().join("hooks/pre-push")).unwrap();
    assert!(hook.contains("bin/gh-commit-guard"), "{hook}");
    assert!(hook.contains(&config.display().to_string()));
    let value: serde_json::Value = serde_json::from_slice(&fs::read(&config).unwrap()).unwrap();
    assert_eq!(value["schema_version"], 1);
    assert_eq!(value["root"], f.guard_root().display().to_string());
}

#[test]
fn installed_canonical_and_legacy_commands_identify_themselves() {
    let f = Fixture::new();
    accepted(f.install(&[]));
    let bin = f.guard_root().join("bin");
    let config = f.guard_root().join("config.json").display().to_string();
    let version = env!("CARGO_PKG_VERSION");
    let canonical = f.command(
        &bin.join("commitguard"),
        &["--config", &config, "version"],
        &f.root,
        &[],
        None,
    );
    assert_eq!(stdout(&canonical), format!("commitguard {version}\n"));
    let legacy = f.command(
        &bin.join("gh-commit-guard"),
        &["--config", &config, "version"],
        &f.root,
        &[],
        None,
    );
    assert_eq!(stdout(&legacy), format!("gh-commit-guard {version}\n"));
    // The installed canonical command resolves the same fresh identity.
    let repo = f.repo("installed canonical");
    let identity = f.command(&bin.join("commitguard"), &["identity"], &repo, &[], None);
    assert!(identity.status.success(), "{}", stderr(&identity));
    assert!(stdout(&identity).contains("tester"));
    // Guarded Git still works when only the guard bin and hook chain are used.
    accepted(f.guarded(&["--version"], &f.root, &[]));
}

#[test]
fn canonical_install_target_in_container_cannot_enter_host_agent_configuration() {
    let f = Fixture::new();
    let repo = f.repo("container canonical");
    let protected = f.home.join(".codex");
    fs::create_dir_all(&protected).unwrap();
    let victim = protected.join("host commitguard");
    fs::write(&victim, "host owned\n").unwrap();
    let bin = f.guard_root().join("bin");
    fs::create_dir_all(&bin).unwrap();
    std::os::unix::fs::symlink(&victim, bin.join("commitguard")).unwrap();
    let global = fs::read(&f.global).unwrap();
    refused(f.install(&["--container", "--repo", &repo.display().to_string()]));
    assert_eq!(fs::read_to_string(&victim).unwrap(), "host owned\n");
    assert_eq!(fs::read(&f.global).unwrap(), global);
    assert!(!bin.join("git").exists());
}

#[test]
fn canonical_write_failure_rolls_back_the_whole_installation() {
    let f = Fixture::new();
    let skill = f.codex.join("skills/gh-commit-identity");
    // A directory where the canonical skill executable belongs fails mid-transaction.
    fs::create_dir_all(skill.join("bin/commitguard")).unwrap();
    fs::write(skill.join("SKILL.md"), "Previous skill text.\n").unwrap();
    let global = fs::read(&f.global).unwrap();
    refused(f.install(&[]));
    assert_eq!(fs::read(&f.global).unwrap(), global);
    assert_eq!(
        fs::read_to_string(skill.join("SKILL.md")).unwrap(),
        "Previous skill text.\n"
    );
    assert!(!skill.join("bin/gh-commit-guard").exists());
    assert!(!f.guard_root().join("bin/git").exists());
    assert!(!f.guard_root().join("bin/commitguard").exists());
}

#[test]
fn reinstall_from_the_guarded_path_keeps_both_names() {
    let f = Fixture::new();
    accepted(f.canonical_install(&[]));
    let bin = f.guard_root().join("bin");
    let path = format!("{}:{}", bin.display(), f.bin.display());
    accepted(f.command(
        &bin.join("commitguard"),
        &["install", "--home", &f.home.display().to_string()],
        &f.root,
        &[("PATH", &path)],
        None,
    ));
    assert_eq!(
        fs::read(bin.join("commitguard")).unwrap(),
        fs::read(bin.join("gh-commit-guard")).unwrap()
    );
    accepted(f.guarded(&["--version"], &f.root, &[]));
}

#[test]
fn standalone_lint_reads_ranges_through_the_real_installed_guard_without_gh() {
    let f = Fixture::new();
    accepted(f.canonical_install(&[]));
    let repo = f.repo("real guard lint range");
    let base = f.commit(&repo, "fix: base", &[]);
    let tip = f.commit(&repo, "fix: guarded range", &[]);
    // Authentication is unavailable. Read-only Git entry points must still
    // allow the independent lint library to inspect every raw range message.
    fs::remove_file(f.bin.join("gh")).unwrap();
    let path = format!(
        "{}:{}",
        f.guard_root().join("bin").display(),
        f.bin.display()
    );
    // Run in a child to isolate cwd and environment from parallel test cases.
    accepted(f.command(
        &std::env::current_exe().unwrap(),
        &["--exact", "lint_range_child", "--ignored", "--nocapture"],
        &repo,
        &[
            ("PATH", &path),
            ("TEST_LINT_FROM", &base),
            ("TEST_LINT_TO", &tip),
        ],
        None,
    ));
}

#[test]
#[ignore = "subprocess fixture invoked by the real installed guard range test"]
fn lint_range_child() {
    let git = commitlint_rust::git::find_git().unwrap();
    let messages = commitlint_rust::git::range_messages(
        &git,
        &std::env::var("TEST_LINT_FROM").unwrap(),
        &std::env::var("TEST_LINT_TO").unwrap(),
    )
    .unwrap();
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0], b"fix: guarded range\n");
    for message in messages {
        commitlint_rust::lint_message(&message).unwrap();
    }
}
