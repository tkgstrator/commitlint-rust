#![cfg(unix)]
mod common;
use common::*;
use std::fs;
#[test]
fn installed_native_runtime_needs_no_bun_python_and_preserves_custom_home() {
    let f = Fixture::new();
    let other = f.codex.join("skills/other");
    fs::create_dir_all(&other).unwrap();
    fs::write(other.join("SKILL.md"), "existing skill\n").unwrap();
    fs::write(f.codex.join("AGENTS.md"), "Existing instructions.\n").unwrap();
    accepted(f.install(&[]));
    let original = fs::read(f.codex.join("AGENTS.md")).unwrap();
    accepted(f.install(&[]));
    assert_eq!(original, fs::read(f.codex.join("AGENTS.md")).unwrap());
    assert_eq!(
        fs::read_to_string(other.join("SKILL.md")).unwrap(),
        "existing skill\n"
    );
    assert_eq!(
        f.raw(
            &["config", "--global", "--get", "alias.preserved"],
            &f.root,
            &[]
        ),
        "status"
    );
    let repo = f.repo("native commit");
    accepted(f.guarded(
        &["commit", "--allow-empty", "-m", "fix: native commit"],
        &repo,
        &[],
    ));
    assert_eq!(
        f.raw(&["show", "-s", "--format=%an <%ae>|%cn <%ce>"], &repo, &[]),
        "tester <44+tester@users.noreply.github.com>|tester <44+tester@users.noreply.github.com>"
    );
}
#[test]
fn installed_entrypoint_rejects_bypass_alias_and_exec() {
    let f = Fixture::new();
    accepted(f.install(&[]));
    let repo = f.repo("bypass");
    for args in [
        vec!["commit", "--no-verify", "--allow-empty", "-m", "invalid"],
        vec!["commit", "-n", "--allow-empty", "-m", "invalid"],
        vec![
            "-c",
            "core.hooksPath=/dev/null",
            "commit",
            "--allow-empty",
            "-m",
            "invalid",
        ],
        vec!["rebase", "--exec", "exit 0", "HEAD"],
    ] {
        refused(f.guarded(&args, &repo, &[]));
    }
    f.raw(
        &["config", "alias.unchecked", "commit --no-verify"],
        &repo,
        &[],
    );
    refused(f.guarded(&["unchecked", "--allow-empty", "-m", "invalid"], &repo, &[]));
}
#[test]
fn separated_c_directory_option_is_supported() {
    let f = Fixture::new();
    accepted(f.install(&[]));
    let repo = f.repo("attached dir");
    let arg = repo.display().to_string();
    accepted(f.guarded(
        &[
            "-C",
            &arg,
            "commit",
            "--allow-empty",
            "-m",
            "fix: directory option",
        ],
        &f.root,
        &[],
    ));
}
#[test]
fn explicit_signing_is_refused_without_creating_a_commit() {
    let f = Fixture::new();
    accepted(f.install(&[]));
    let repo = f.repo("signing");
    refused(f.guarded(
        &[
            "-c",
            "gpg.format=ssh",
            "-c",
            "user.signingkey=/dev/null",
            "commit",
            "-S",
            "--allow-empty",
            "-m",
            "fix: signed candidate",
        ],
        &repo,
        &[],
    ));
    let result = f.command(&f.git, &["rev-parse", "--verify", "HEAD"], &repo, &[], None);
    assert!(!result.status.success());
}
#[test]
fn real_bare_push_preserves_existing_prepush_input_and_rejects_foreign_intermediate() {
    let f = Fixture::new();
    accepted(f.install(&[]));
    let repo = f.repo("actual push");
    let bare = f.bare("bare push");
    let url = bare.display().to_string();
    accepted(f.guarded(
        &["commit", "--allow-empty", "-m", "fix: valid push"],
        &repo,
        &[],
    ));
    f.raw(&["remote", "add", "origin", &url], &repo, &[]);
    let payload = f.root.join("hook payload");
    let args = f.root.join("hook args");
    executable(
        &repo.join(".git/hooks/pre-push"),
        &format!(
            "#!/bin/sh\nprintf '%s\\n' \"$1\" \"$2\" > '{}'\n/bin/cat > '{}'\n",
            args.display(),
            payload.display()
        ),
    );
    accepted(f.guarded(&["push", "origin", "main:main"], &repo, &[]));
    let first = f.raw(&["rev-parse", "HEAD"], &repo, &[]);
    assert_eq!(
        fs::read_to_string(payload).unwrap(),
        format!(
            "refs/heads/main {first} refs/heads/main {}\n",
            "0".repeat(40)
        )
    );
    assert_eq!(
        fs::read_to_string(args).unwrap(),
        format!("origin\n{url}\n")
    );
    f.commit(
        &repo,
        "fix: foreign intermediate",
        &[("GIT_AUTHOR_NAME", "foreign")],
    );
    accepted(f.guarded(
        &["commit", "--allow-empty", "-m", "fix: valid tip"],
        &repo,
        &[],
    ));
    refused(f.guarded(&["push", "origin", "main:main"], &repo, &[]));
    assert_eq!(f.raw(&["rev-parse", "refs/heads/main"], &bare, &[]), first);
}
#[test]
fn prior_global_and_local_hook_rejections_are_preserved() {
    let f = Fixture::new();
    let global = f.home.join("prior global hooks");
    fs::create_dir_all(&global).unwrap();
    let marker = f.root.join("global ran");
    executable(
        &global.join("pre-commit"),
        &format!("#!/bin/sh\nprintf ran > '{}'\nexit 13\n", marker.display()),
    );
    f.raw(
        &[
            "config",
            "--global",
            "core.hooksPath",
            &global.display().to_string(),
        ],
        &f.root,
        &[],
    );
    accepted(f.install(&[]));
    let repo = f.repo("chain");
    refused(f.guarded(
        &["commit", "--allow-empty", "-m", "fix: global chain"],
        &repo,
        &[],
    ));
    assert!(marker.is_file());
    let local = repo.join("local hooks");
    fs::create_dir_all(&local).unwrap();
    let local_marker = f.root.join("local ran");
    executable(
        &local.join("pre-commit"),
        &format!(
            "#!/bin/sh\nprintf ran > '{}'\nexit 14\n",
            local_marker.display()
        ),
    );
    f.raw(
        &["config", "core.hooksPath", &local.display().to_string()],
        &repo,
        &[],
    );
    refused(f.guarded(
        &["commit", "--allow-empty", "-m", "fix: local chain"],
        &repo,
        &[],
    ));
    assert!(local_marker.is_file());
}
#[test]
fn editor_comments_scissors_and_linked_worktree_hooks() {
    let f = Fixture::new();
    accepted(f.install(&[]));
    let repo = f.repo("editor original");
    let editor = f.root.join("editor fixture");
    executable(
        &editor,
        "#!/bin/sh\n/bin/cat > \"$1\" <<'END'\nfix: edited message\n\n# Git comment\n# ------------------------ >8 ------------------------\n+verbose diff beyond scissors\nEND\n",
    );
    let editor_arg = format!("'{}'", editor.display());
    accepted(f.guarded(
        &["commit", "--allow-empty", "--cleanup=scissors"],
        &repo,
        &[("GIT_EDITOR", &editor_arg)],
    ));
    assert_eq!(
        f.raw(&["show", "-s", "--format=%B"], &repo, &[]),
        "fix: edited message"
    );
    let linked = f.root.join("linked worktree");
    f.raw(
        &[
            "worktree",
            "add",
            "-q",
            "-b",
            "linked",
            &linked.display().to_string(),
        ],
        &repo,
        &[],
    );
    let marker = f.root.join("common hook ran");
    executable(
        &repo.join(".git/hooks/pre-commit"),
        &format!("#!/bin/sh\nprintf ran > '{}'\nexit 15\n", marker.display()),
    );
    refused(f.guarded(
        &["commit", "--allow-empty", "-m", "fix: linked change"],
        &linked,
        &[],
    ));
    assert!(marker.is_file());
}
#[test]
fn reinstall_while_native_shim_first_on_path_does_not_recurse() {
    let f = Fixture::new();
    accepted(f.install(&[]));
    let path = format!(
        "{}:{}",
        f.guard_root().join("bin").display(),
        f.bin.display()
    );
    accepted(f.cli(
        &["install", "--home", &f.home.display().to_string()],
        &f.root,
        &[("PATH", &path)],
        None,
    ));
    accepted(f.guarded(&["--version"], &f.root, &[]));
}
#[test]
fn container_global_config_protection_and_normal_repo_mandates() {
    let f = Fixture::new();
    let repo = f.repo("container");
    let protected = f.home.join(".codex");
    fs::create_dir_all(&protected).unwrap();
    let config = protected.join("mounted Git config");
    let before = "[alias]\n  preserved = status\n";
    fs::write(&config, before).unwrap();
    refused(f.cli(
        &[
            "install",
            "--home",
            &f.home.display().to_string(),
            "--container",
            "--repo",
            &repo.display().to_string(),
        ],
        &f.root,
        &[("GIT_CONFIG_GLOBAL", &config.display().to_string())],
        None,
    ));
    assert_eq!(fs::read_to_string(&config).unwrap(), before);
    fs::write(repo.join("AGENTS.md"), "Repo instructions.\n").unwrap();
    fs::write(repo.join("CLAUDE.md"), "Claude instructions.\n").unwrap();
    accepted(f.install(&["--container", "--repo", &repo.display().to_string()]));
    for file in ["AGENTS.md", "CLAUDE.md"] {
        let text = fs::read_to_string(repo.join(file)).unwrap();
        assert!(text.contains("instructions."));
        assert!(text.contains("gh-commit-identity"));
    }
}
#[test]
fn missing_gh_refuses_without_activation_or_credential_copy() {
    let f = Fixture::new();
    let gh = f.bin.join("gh");
    fs::remove_file(gh).unwrap();
    let before = fs::read(&f.global).unwrap();
    refused(f.install(&[]));
    assert_eq!(fs::read(&f.global).unwrap(), before);
    assert!(!f.guard_root().join("bin/git").exists());
}
#[test]
fn failed_setup_rolls_back_existing_files_and_global_config() {
    let f = Fixture::new();
    let skill = f.codex.join("skills/gh-commit-identity");
    fs::create_dir_all(&skill).unwrap();
    fs::write(skill.join("SKILL.md"), "Previous skill text.\n").unwrap();
    fs::write(f.codex.join("AGENTS.md"), "Previous agent rules.\n").unwrap();
    let claude = f.home.join(".claude");
    fs::create_dir_all(&claude).unwrap();
    fs::write(
        claude.join("CLAUDE.md"),
        "<!-- gh-commit-identity -->\nmalformed block\n",
    )
    .unwrap();
    let global = fs::read(&f.global).unwrap();
    refused(f.install(&[]));
    assert_eq!(fs::read(&f.global).unwrap(), global);
    assert_eq!(
        fs::read_to_string(skill.join("SKILL.md")).unwrap(),
        "Previous skill text.\n"
    );
    assert_eq!(
        fs::read_to_string(f.codex.join("AGENTS.md")).unwrap(),
        "Previous agent rules.\n"
    );
    assert!(!f.guard_root().join("bin/git").exists());
}
#[test]
fn container_symlinked_skill_target_is_refused_before_host_file_changes() {
    let f = Fixture::new();
    let repo = f.repo("container symlinks");
    let protected = f.home.join(".claude/skills/gh-commit-identity");
    fs::create_dir_all(&protected).unwrap();
    fs::write(protected.join("SKILL.md"), "Protected host skill.\n").unwrap();
    fs::create_dir_all(repo.join(".claude/skills")).unwrap();
    std::os::unix::fs::symlink(&protected, repo.join(".claude/skills/gh-commit-identity")).unwrap();
    let global = fs::read(&f.global).unwrap();
    refused(f.install(&["--container", "--repo", &repo.display().to_string()]));
    assert_eq!(
        fs::read_to_string(protected.join("SKILL.md")).unwrap(),
        "Protected host skill.\n"
    );
    assert_eq!(fs::read(&f.global).unwrap(), global);
    assert!(!f.guard_root().join("bin/git").exists());
}
#[test]
fn container_symlink_loop_stops_without_activating_global_config() {
    let f = Fixture::new();
    let repo = f.repo("symlink loop");
    fs::create_dir(repo.join(".codex")).unwrap();
    std::os::unix::fs::symlink("skills", repo.join(".codex/skills")).unwrap();
    let before = fs::read(&f.global).unwrap();
    refused(f.install(&["--container", "--repo", &repo.display().to_string()]));
    assert_eq!(fs::read(&f.global).unwrap(), before);
    assert!(!f.guard_root().join("bin/git").exists());
}
#[test]
fn ordinary_rebase_rejects_foreign_effective_committer_and_noverify_variants() {
    let f = Fixture::new();
    accepted(f.install(&[]));
    let repo = f.repo("rebase identities");
    let base = f.commit(&repo, "fix: base", &[]);
    f.raw(&["checkout", "-q", "-b", "feature"], &repo, &[]);
    let feature = f.commit(&repo, "fix: feature", &[]);
    f.raw(&["checkout", "-q", "main"], &repo, &[]);
    f.commit(&repo, "fix: main", &[]);
    f.raw(&["checkout", "-q", "feature"], &repo, &[]);
    refused(f.guarded(
        &["rebase", "main"],
        &repo,
        &[("GIT_COMMITTER_NAME", "foreign")],
    ));
    assert_eq!(f.raw(&["rev-parse", "HEAD"], &repo, &[]), feature);
    for flag in ["--no-verify", "--no-ver", "--no-v"] {
        refused(f.guarded(&["rebase", flag, &base], &repo, &[]));
    }
}
#[test]
fn autocorrect_cannot_turn_a_typo_into_an_unchecked_commit() {
    let f = Fixture::new();
    accepted(f.install(&[]));
    let repo = f.repo("autocorrect bypass");
    f.raw(&["config", "help.autocorrect", "-1"], &repo, &[]);
    refused(f.guarded(
        &["comit", "-n", "--allow-empty", "-m", "fix: typo attempt"],
        &repo,
        &[],
    ));
    assert!(
        !f.command(&f.git, &["rev-parse", "--verify", "HEAD"], &repo, &[], None)
            .status
            .success()
    );
}
#[test]
fn config_environment_and_relative_hooks_cannot_create_foreign_identity_commits() {
    let f = Fixture::new();
    accepted(f.install(&[]));
    let repo = f.repo("config environment");
    fs::create_dir(repo.join("emptyhooks")).unwrap();
    for extra in [
        vec![
            ("GIT_CONFIG_COUNT", "2"),
            ("GIT_CONFIG_KEY_0", "core.hooksPath"),
            ("GIT_CONFIG_VALUE_0", "emptyhooks"),
            ("GIT_CONFIG_KEY_1", "gpg.program"),
            ("GIT_CONFIG_VALUE_1", "/bin/false"),
        ],
        vec![(
            "GIT_CONFIG_PARAMETERS",
            "'core.hooksPath=emptyhooks' 'gpg.program=/bin/false'",
        )],
    ] {
        assert_eq!(
            f.raw(&["config", "--get", "core.hooksPath"], &repo, &extra),
            "emptyhooks"
        );
        assert_eq!(
            f.raw(&["config", "--get", "gpg.program"], &repo, &extra),
            "/bin/false"
        );
    }
    let args = [
        "commit",
        "--allow-empty",
        "-m",
        "fix: valid message wrong identity",
    ];
    refused(f.guarded(
        &args,
        &repo,
        &[
            ("GIT_AUTHOR_NAME", "foreign"),
            ("GIT_CONFIG_COUNT", "2"),
            ("GIT_CONFIG_KEY_0", "core.hooksPath"),
            ("GIT_CONFIG_VALUE_0", "emptyhooks"),
            ("GIT_CONFIG_KEY_1", "gpg.program"),
            ("GIT_CONFIG_VALUE_1", "/bin/false"),
        ],
    ));
    refused(f.guarded(
        &args,
        &repo,
        &[
            ("GIT_AUTHOR_NAME", "foreign"),
            (
                "GIT_CONFIG_PARAMETERS",
                "'core.hooksPath=emptyhooks' 'gpg.program=/bin/false'",
            ),
        ],
    ));
    refused(f.guarded(
        &[
            "-c",
            "core.hooksPath=emptyhooks",
            "commit",
            "--allow-empty",
            "-m",
            "fix: relative hooks wrong identity",
        ],
        &repo,
        &[("GIT_AUTHOR_NAME", "foreign")],
    ));
    assert!(
        !f.command(&f.git, &["rev-parse", "--verify", "HEAD"], &repo, &[], None)
            .status
            .success()
    );
}
#[test]
fn symlinked_profile_preserves_symlink_mode_and_existing_content() {
    use std::os::unix::fs::PermissionsExt;
    let f = Fixture::new();
    let target = f.home.join("custom profile");
    fs::write(&target, "# Existing profile content\n").unwrap();
    fs::set_permissions(&target, fs::Permissions::from_mode(0o600)).unwrap();
    std::os::unix::fs::symlink("custom profile", f.home.join(".profile")).unwrap();
    accepted(f.install(&[]));
    assert!(
        fs::symlink_metadata(f.home.join(".profile"))
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert_eq!(
        fs::metadata(&target).unwrap().permissions().mode() & 0o777,
        0o600
    );
    let body = fs::read_to_string(&target).unwrap();
    assert!(body.contains("# Existing profile content"));
    assert!(body.contains("gh-commit-identity"));
}
#[test]
fn existing_global_config_lock_failure_rolls_back_activation() {
    let f = Fixture::new();
    let before = fs::read(&f.global).unwrap();
    let lock = f.home.join(".gitconfig.lock");
    fs::write(&lock, "another writer owns this lock\n").unwrap();
    refused(f.install(&[]));
    assert_eq!(fs::read(&f.global).unwrap(), before);
    assert_eq!(
        fs::read_to_string(&lock).unwrap(),
        "another writer owns this lock\n"
    );
    assert!(!f.guard_root().join("bin/git").exists());
    assert!(!f.home.join(".profile").exists());
}
#[test]
fn legacy_guard_migration_preserves_records_without_executing_old_runtime() {
    let f = Fixture::new();
    let repo = f.repo("legacy mapped repo");
    let old = f.home.join(".codex/git-identity-guard");
    fs::create_dir_all(old.join("bin")).unwrap();
    fs::create_dir_all(old.join("hooks")).unwrap();
    let invoked = f.root.join("old runtime must not run");
    executable(
        &old.join("bin/git"),
        &format!("#!/bin/sh\nprintf ran > '{}'\nexit 99\n", invoked.display()),
    );
    let original = f.root.join("legacy repo hooks");
    fs::create_dir_all(&original).unwrap();
    let chained = f.root.join("legacy chained");
    executable(
        &original.join("pre-commit"),
        &format!("#!/bin/sh\nprintf ran > '{}'\nexit 19\n", chained.display()),
    );
    let common = repo.join(".git");
    let mut mappings = serde_json::Map::new();
    mappings.insert(
        common.display().to_string(),
        serde_json::Value::String(original.display().to_string()),
    );
    let value = serde_json::json!({"git":f.git,"gh":f.bin.join("gh"),"hooks":old.join("hooks"),"repo_hooks":mappings,"previous_hooks":original,"verify_programs":{"ssh":"/fixture/old-ssh-verifier","openpgp":"/fixture/old-gpg-verifier"},"bun":"/missing/bun","python":"/missing/python3","commitlint_script":"/missing/lint.mjs","user_metadata":{"keep":"unchanged"}});
    fs::write(old.join("config.json"), serde_json::to_vec(&value).unwrap()).unwrap();
    f.raw(
        &[
            "config",
            "--global",
            "core.hooksPath",
            &old.join("hooks").display().to_string(),
        ],
        &f.root,
        &[],
    );
    let path = format!("{}:{}", old.join("bin").display(), f.bin.display());
    accepted(f.cli(
        &["install", "--home", &f.home.display().to_string()],
        &f.root,
        &[("PATH", &path)],
        None,
    ));
    assert!(!invoked.exists());
    let config: serde_json::Value =
        serde_json::from_slice(&fs::read(f.guard_root().join("config.json")).unwrap()).unwrap();
    assert_eq!(config["repo_hooks"], value["repo_hooks"]);
    assert_eq!(
        config["verify_programs"]["ssh"],
        value["verify_programs"]["ssh"]
    );
    assert_eq!(config["user_metadata"], value["user_metadata"]);
    for obsolete in ["bun", "python", "commitlint_script", "hooks"] {
        assert!(config.get(obsolete).is_none());
    }
    accepted(f.guarded(&["--version"], &f.root, &[]));
    refused(f.guarded(
        &[
            "commit",
            "--allow-empty",
            "-m",
            "fix: preserved legacy hooks",
        ],
        &repo,
        &[],
    ));
    assert!(chained.exists());
}
#[test]
fn optional_untracked_flags_never_consume_a_following_noverify_flag() {
    let f = Fixture::new();
    let unwrapped = f.repo("native proof");
    f.raw(
        &[
            "-c",
            "core.hooksPath=/dev/null",
            "commit",
            "--allow-empty",
            "-u",
            "-n",
            "-m",
            "invalid format",
        ],
        &unwrapped,
        &[],
    );
    assert_eq!(
        f.raw(&["show", "-s", "--format=%B"], &unwrapped, &[]),
        "invalid format"
    );
    accepted(f.install(&[]));
    let repo = f.repo("guarded optional flags");
    for options in [
        vec!["-u", "-n"],
        vec!["--untracked-files", "-n"],
        vec!["-un"],
        vec!["-nu"],
    ] {
        let mut args = vec!["commit", "--allow-empty"];
        args.extend(options);
        args.extend(["-m", "invalid format"]);
        refused(f.guarded(&args, &repo, &[]));
        assert!(
            !f.command(&f.git, &["rev-parse", "--verify", "HEAD"], &repo, &[], None)
                .status
                .success()
        );
    }
    for option in ["-uno", "-unormal", "-uall"] {
        accepted(f.guarded(
            &[
                "commit",
                "--allow-empty",
                option,
                "-m",
                "fix: valid untracked mode",
            ],
            &repo,
            &[],
        ));
    }
}
#[test]
fn container_codex_home_must_resolve_within_the_repository() {
    let f = Fixture::new();
    let repo = f.repo("container workspace");
    let sentinel = f.codex.join("AGENTS.md");
    fs::write(&sentinel, "Outside host rules.\n").unwrap();
    let before = fs::read(&f.global).unwrap();
    refused(f.cli(
        &[
            "install",
            "--home",
            &f.home.display().to_string(),
            "--container",
            "--repo",
            &repo.display().to_string(),
        ],
        &f.root,
        &[],
        None,
    ));
    assert_eq!(
        fs::read_to_string(sentinel).unwrap(),
        "Outside host rules.\n"
    );
    assert_eq!(fs::read(&f.global).unwrap(), before);
    let inside = repo.join(".codex-container").display().to_string();
    accepted(f.cli(
        &[
            "install",
            "--home",
            &f.home.display().to_string(),
            "--container",
            "--repo",
            &repo.display().to_string(),
        ],
        &f.root,
        &[("CODEX_HOME", &inside)],
        None,
    ));
    assert!(
        repo.join(".codex-container/skills/gh-commit-identity/SKILL.md")
            .is_file()
    );
}
#[test]
fn native_push_accepts_revision_expression_and_checks_its_supplied_commit() {
    let f = Fixture::new();
    accepted(f.install(&[]));
    let repo = f.repo("revision expression");
    let first = f.commit(&repo, "fix: first", &[]);
    f.commit(&repo, "fix: second", &[]);
    let bare = f.bare("revision expression remote");
    let url = bare.display().to_string();
    accepted(f.guarded(&["push", &url, "HEAD~1:refs/heads/earlier"], &repo, &[]));
    assert_eq!(
        f.raw(&["rev-parse", "refs/heads/earlier"], &bare, &[]),
        first
    );
    f.commit(
        &repo,
        "fix: foreign source",
        &[("GIT_AUTHOR_NAME", "foreign")],
    );
    f.commit(&repo, "fix: valid current tip", &[]);
    refused(f.guarded(&["push", &url, "HEAD~1:refs/heads/bad-source"], &repo, &[]));
    assert!(
        !f.command(
            &f.git,
            &["rev-parse", "--verify", "refs/heads/bad-source"],
            &bare,
            &[],
            None
        )
        .status
        .success()
    );
}
#[test]
fn custom_comment_character_is_cleaned_and_auto_is_fail_closed() {
    let f = Fixture::new();
    accepted(f.install(&[]));
    let repo = f.repo("comment character");
    f.raw(&["config", "core.commentChar", ";"], &repo, &[]);
    let editor = f.root.join("semicolon editor");
    executable(
        &editor,
        "#!/bin/sh\n/bin/cat > \"$1\" <<'END'\n; editor comment\nfix: custom comment\n\n; ------------------------ >8 ------------------------\n+verbose diff\nEND\n",
    );
    let arg = format!("'{}'", editor.display());
    accepted(f.guarded(
        &["commit", "--allow-empty", "--cleanup=scissors"],
        &repo,
        &[("GIT_EDITOR", &arg)],
    ));
    assert_eq!(
        f.raw(&["show", "-s", "--format=%B"], &repo, &[]),
        "fix: custom comment"
    );
    let auto = f.repo("auto comment");
    f.raw(&["config", "core.commentChar", "auto"], &auto, &[]);
    refused(f.guarded(
        &["commit", "--allow-empty", "-m", "fix: automatic comment"],
        &auto,
        &[],
    ));
    assert!(
        !f.command(&f.git, &["rev-parse", "--verify", "HEAD"], &auto, &[], None)
            .status
            .success()
    );
}
#[test]
fn safe_own_interactive_reword_preserves_trees_topology_and_backup_before_push() {
    let f = Fixture::new();
    let repo = f.repo("safe own reword");
    fs::write(repo.join("fixture.txt"), "base data\n").unwrap();
    f.raw(&["add", "fixture.txt"], &repo, &[]);
    let base = f.commit(&repo, "fix: base data", &[]);
    fs::write(repo.join("fixture.txt"), "meaningful repaired data\n").unwrap();
    f.raw(&["add", "fixture.txt"], &repo, &[]);
    let original = f.commit(&repo, "repair important fixture data", &[]);
    f.raw(
        &["update-ref", "refs/backup/agent-reword-fixture", &original],
        &repo,
        &[],
    );
    let tree = f.raw(&["rev-parse", "HEAD^{tree}"], &repo, &[]);
    let count = f.raw(&["rev-list", "--count", "HEAD"], &repo, &[]);
    accepted(f.install(&[]));
    let sequence = f.root.join("reword sequence editor");
    executable(
        &sequence,
        "#!/bin/sh\nwhile IFS= read -r line; do\n case \"$line\" in pick*) printf 'reword%s\\n' \"${line#pick}\";; *) printf '%s\\n' \"$line\";; esac\ndone < \"$1\" > \"$1.fixture\"\n/bin/mv \"$1.fixture\" \"$1\"\n",
    );
    let editor = f.root.join("meaningful reword editor");
    executable(
        &editor,
        "#!/bin/sh\nprintf 'fix: update fixture data\\n' > \"$1\"\n",
    );
    let sequence_arg = format!("'{}'", sequence.display());
    let editor_arg = format!("'{}'", editor.display());
    accepted(f.guarded(
        &["rebase", "-i", &base],
        &repo,
        &[
            ("GIT_SEQUENCE_EDITOR", &sequence_arg),
            ("GIT_EDITOR", &editor_arg),
        ],
    ));
    let rewritten = f.raw(&["rev-parse", "HEAD"], &repo, &[]);
    assert_ne!(rewritten, original);
    assert_eq!(f.raw(&["rev-parse", "HEAD^{tree}"], &repo, &[]), tree);
    assert_eq!(f.raw(&["rev-list", "--count", "HEAD"], &repo, &[]), count);
    assert_eq!(f.raw(&["rev-parse", "HEAD^"], &repo, &[]), base);
    assert_eq!(
        f.raw(
            &["rev-parse", "refs/backup/agent-reword-fixture"],
            &repo,
            &[]
        ),
        original
    );
    assert_eq!(
        f.raw(&["show", "-s", "--format=%B"], &repo, &[]),
        "fix: update fixture data"
    );
    accepted(f.cli(&["commits", &rewritten], &repo, &[], None));
    let bare = f.bare("reword preflight destination");
    let url = bare.display().to_string();
    accepted(f.cli(&["push", &url, "refs/heads/main"], &repo, &[], None));
    assert_eq!(
        f.raw(&["for-each-ref", "--format=%(refname)"], &bare, &[]),
        ""
    );
}
#[test]
fn reword_source_gate_still_refuses_foreign_author_or_credit() {
    for foreign_author in [true, false] {
        let f = Fixture::new();
        let repo = f.repo("foreign reword source");
        let base = f.commit(&repo, "fix: base", &[]);
        let message = if foreign_author {
            "fix: foreign author"
        } else {
            "fix: foreign credit\n\nCo-authored-by: Other <other@example.com>"
        };
        let extra = if foreign_author {
            vec![("GIT_AUTHOR_NAME", "foreign")]
        } else {
            vec![]
        };
        let source = f.commit(&repo, message, &extra);
        accepted(f.install(&[]));
        refused(f.guarded(
            &["rebase", "-i", &base],
            &repo,
            &[("GIT_SEQUENCE_EDITOR", "/bin/true")],
        ));
        assert_eq!(f.raw(&["rev-parse", "HEAD"], &repo, &[]), source);
    }
}
#[test]
fn unchanged_invalid_own_wording_is_rejected_before_any_push_publication() {
    let f = Fixture::new();
    let repo = f.repo("ordinary pick validation");
    let base = f.commit(&repo, "fix: base", &[]);
    f.commit(&repo, "invalid own wording", &[]);
    accepted(f.install(&[]));
    accepted(f.guarded(&["rebase", "--force-rebase", &base], &repo, &[]));
    let bare = f.bare("ordinary pick remote");
    let url = bare.display().to_string();
    refused(f.cli(&["push", &url, "refs/heads/main"], &repo, &[], None));
    refused(f.guarded(&["push", &url, "main:main"], &repo, &[]));
    assert_eq!(
        f.raw(&["for-each-ref", "--format=%(refname)"], &bare, &[]),
        ""
    );
}
#[test]
fn container_ignores_mounted_host_legacy_config_and_preserves_local_hooks() {
    let f = Fixture::new();
    let repo = f.repo("mounted host legacy");
    let host = f.home.join(".codex/git-identity-guard");
    fs::create_dir_all(&host).unwrap();
    let host_config = host.join("config.json");
    let bytes=br#"{"git":"/host/missing/git","gh":"/host/missing/gh","hooks":"/Users/host-user/.codex/git-identity-guard/hooks","verify_programs":{"ssh":"/host/missing/ssh"},"user_metadata":"mounted host sentinel"}"#;
    fs::write(&host_config, bytes).unwrap();
    f.raw(
        &[
            "config",
            "--global",
            "core.hooksPath",
            "/Users/host-user/.codex/git-identity-guard/hooks",
        ],
        &f.root,
        &[],
    );
    let marker = f.root.join("container native common hook ran");
    executable(
        &repo.join(".git/hooks/pre-commit"),
        &format!("#!/bin/sh\nprintf ran > '{}'\nexit 21\n", marker.display()),
    );
    accepted(f.install(&["--container", "--repo", &repo.display().to_string()]));
    assert_eq!(fs::read(&host_config).unwrap(), bytes);
    let config: serde_json::Value =
        serde_json::from_slice(&fs::read(f.guard_root().join("config.json")).unwrap()).unwrap();
    assert_eq!(
        config["git"].as_str().unwrap(),
        f.git.canonicalize().unwrap().display().to_string()
    );
    assert_eq!(
        config["gh"].as_str().unwrap(),
        f.bin
            .join("gh")
            .canonicalize()
            .unwrap()
            .display()
            .to_string()
    );
    refused(f.guarded(
        &[
            "commit",
            "--allow-empty",
            "-m",
            "fix: local container hooks",
        ],
        &repo,
        &[],
    ));
    assert!(marker.exists());
    let local = repo.join("mapped hooks");
    fs::create_dir(&local).unwrap();
    let local_marker = f.root.join("container local configured hook ran");
    executable(
        &local.join("pre-commit"),
        &format!(
            "#!/bin/sh\nprintf ran > '{}'\nexit 22\n",
            local_marker.display()
        ),
    );
    f.raw(
        &["config", "core.hooksPath", &local.display().to_string()],
        &repo,
        &[],
    );
    refused(f.guarded(
        &[
            "commit",
            "--allow-empty",
            "-m",
            "fix: configured container hooks",
        ],
        &repo,
        &[],
    ));
    assert!(local_marker.exists());
}
