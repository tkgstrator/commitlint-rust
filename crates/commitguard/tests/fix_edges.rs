#![cfg(unix)]
mod common;
use common::*;
use serde_json::Value;
use std::{fs, path::PathBuf};

struct Setup {
    f: Fixture,
    repo: PathBuf,
    base: String,
    source: String,
    plan: PathBuf,
}
impl Setup {
    fn new() -> Self {
        let f = Fixture::new();
        let repo = f.repo("fix edges");
        let base = f.commit(&repo, "feat: base", &[]);
        let remote = f.bare("edge remote");
        f.raw(
            &["remote", "add", "origin", remote.to_str().unwrap()],
            &repo,
            &[],
        );
        f.raw(&["push", "-q", "origin", "main"], &repo, &[]);
        let source = f.commit(&repo, "untyped source message", &[]);
        accepted(f.canonical_install(&[]));
        let plan = f.root.join("edge plan.json");
        Self {
            f,
            repo,
            base,
            source,
            plan,
        }
    }
    fn export(&self, path: &std::path::Path) -> std::process::Output {
        let range = format!("{}..HEAD", self.base);
        self.f.canonical(
            &["fix", "--range", &range, "--plan", path.to_str().unwrap()],
            &self.repo,
            &[],
            None,
        )
    }
    fn approve(&self) {
        accepted(self.export(&self.plan));
        let mut value: Value = serde_json::from_slice(&fs::read(&self.plan).unwrap()).unwrap();
        value["candidates"][0]["message"] = "fix: preserve checkpoint".into();
        fs::write(&self.plan, serde_json::to_vec_pretty(&value).unwrap()).unwrap();
    }
    fn apply(&self) -> std::process::Output {
        self.f.canonical(
            &["fix", "--apply", self.plan.to_str().unwrap()],
            &self.repo,
            &[],
            None,
        )
    }
    fn tip(&self) -> String {
        self.f.raw(&["rev-parse", "HEAD"], &self.repo, &[])
    }
}

#[test]
fn proposal_in_worktree_is_refused_before_creating_it() {
    let s = Setup::new();
    let output = s.repo.join("fixes.json");
    refused(s.export(&output));
    assert!(!output.exists());
    assert_eq!(s.tip(), s.source);
    accepted(s.export(&s.plan)); // Refusal cannot merely mean the feature is absent.
}

#[test]
fn unrelated_untracked_work_is_not_ignored_as_fix_input() {
    let s = Setup::new();
    fs::write(s.repo.join("user work.txt"), b"preserve me").unwrap();
    refused(s.export(&s.plan));
    assert!(!s.plan.exists());
    assert_eq!(
        fs::read(s.repo.join("user work.txt")).unwrap(),
        b"preserve me"
    );
    assert_eq!(s.tip(), s.source);
    fs::remove_file(s.repo.join("user work.txt")).unwrap();
    accepted(s.export(&s.plan));
}

#[test]
fn candidate_symlink_cannot_redirect_preview_or_apply() {
    let s = Setup::new();
    s.approve();
    let original = s.f.root.join("saved candidate.json");
    fs::rename(&s.plan, &original).unwrap();
    std::os::unix::fs::symlink(&original, &s.plan).unwrap();
    refused(s.f.canonical(
        &["fix", "--preview", s.plan.to_str().unwrap()],
        &s.repo,
        &[],
        None,
    ));
    refused(s.apply());
    assert_eq!(s.tip(), s.source);
    assert!(original.is_file());
}

#[test]
fn another_local_branch_is_not_rewritten_or_ignored() {
    let s = Setup::new();
    s.approve();
    s.f.raw(&["branch", "user-branch", &s.source], &s.repo, &[]);
    refused(s.apply());
    assert_eq!(s.tip(), s.source);
    assert_eq!(
        s.f.raw(&["rev-parse", "user-branch"], &s.repo, &[]),
        s.source
    );
}

#[test]
fn malformed_message_is_accepted_only_after_lint_and_stale_rebase_settings_are_controlled() {
    let s = Setup::new();
    s.f.raw(
        &["config", "rebase.instructionFormat", "unexpected %s"],
        &s.repo,
        &[],
    );
    s.f.raw(
        &["config", "rebase.abbreviateCommands", "true"],
        &s.repo,
        &[],
    );
    s.f.raw(&["config", "rebase.updateRefs", "true"], &s.repo, &[]);
    s.f.raw(&["config", "rebase.autoSquash", "true"], &s.repo, &[]);
    s.approve();
    accepted(s.apply());
    let result = s.tip();
    assert_ne!(result, s.source);
    assert_eq!(
        s.f.raw(&["show", "-s", "--format=%B", &result], &s.repo, &[]),
        "fix: preserve checkpoint"
    );
    assert_eq!(s.f.raw(&["rev-parse", "HEAD^"], &s.repo, &[]), s.base);
    accepted(s.f.canonical(&["commits", &result], &s.repo, &[], None));
}

#[test]
fn completed_operation_does_not_permanently_block_a_later_fix() {
    let s = Setup::new();
    s.approve();
    accepted(s.apply());
    let first = s.tip();
    accepted(s.f.guarded(
        &[
            "commit",
            "--allow-empty",
            "-m",
            "fix: subsequent checkpoint",
        ],
        &s.repo,
        &[],
    ));
    let second_source = s.tip();
    let second_plan = s.f.root.join("second plan.json");
    let range = format!("{first}..HEAD");
    accepted(s.f.canonical(
        &[
            "fix",
            "--range",
            &range,
            "--plan",
            second_plan.to_str().unwrap(),
        ],
        &s.repo,
        &[],
        None,
    ));
    let mut value: Value = serde_json::from_slice(&fs::read(&second_plan).unwrap()).unwrap();
    value["candidates"][0]["message"] = "fix: clarify subsequent checkpoint".into();
    fs::write(&second_plan, serde_json::to_vec_pretty(&value).unwrap()).unwrap();
    accepted(s.f.canonical(
        &["fix", "--apply", second_plan.to_str().unwrap()],
        &s.repo,
        &[],
        None,
    ));
    assert_ne!(s.tip(), second_source);
    assert_eq!(s.f.raw(&["rev-parse", "HEAD^"], &s.repo, &[]), first);
    assert_eq!(
        s.f.raw(&["show", "-s", "--format=%B", "HEAD"], &s.repo, &[]),
        "fix: clarify subsequent checkpoint"
    );
    let backups = s.f.raw(
        &[
            "for-each-ref",
            "--format=%(objectname)",
            "refs/commitguard/backups/",
        ],
        &s.repo,
        &[],
    );
    assert_eq!(backups.lines().count(), 2);
    assert!(backups.lines().any(|oid| oid == s.source));
    assert!(backups.lines().any(|oid| oid == second_source));
}

#[test]
fn completed_retry_rejects_changed_candidates_and_wrong_operation_confirmation() {
    let s = Setup::new();
    s.approve();
    accepted(s.apply());
    let completed = s.tip();
    refused(s.f.canonical(
        &[
            "fix",
            "--apply",
            s.plan.to_str().unwrap(),
            "--confirm-author-migration",
            "wrong",
        ],
        &s.repo,
        &[],
        None,
    ));
    let mut value: Value = serde_json::from_slice(&fs::read(&s.plan).unwrap()).unwrap();
    value["candidates"][0]["message"] = "fix: different approved content".into();
    fs::write(&s.plan, serde_json::to_vec(&value).unwrap()).unwrap();
    refused(s.apply());
    assert_eq!(s.tip(), completed);
}

#[test]
fn backup_deleted_by_a_hook_prevents_promotion() {
    let s = Setup::new();
    executable(
        &s.repo.join(".git/hooks/post-commit"),
        "#!/bin/sh\nfor ref in $(git for-each-ref --format='%(refname)' refs/commitguard/backups/); do git update-ref -d \"$ref\"; done\n",
    );
    s.approve();
    refused(s.apply());
    assert_eq!(s.tip(), s.source);
}

#[test]
fn post_promotion_hook_ref_mutation_is_reported_without_rollback() {
    let s = Setup::new();
    executable(
        &s.repo.join(".git/hooks/reference-transaction"),
        "#!/bin/sh\n[ \"$1\" = committed ] || exit 0\nwhile read old new ref; do\n if [ \"$ref\" = refs/heads/main ] && [ \"$old\" != \"$new\" ]; then\n  git update-ref refs/heads/hook-side-effect \"$new\" || exit 1\n fi\ndone\n",
    );
    s.approve();
    refused(s.apply());
    assert_ne!(s.tip(), s.source);
    assert_eq!(
        s.f.raw(&["rev-parse", "refs/heads/hook-side-effect"], &s.repo, &[]),
        s.tip()
    );
    assert_eq!(
        s.f.raw(
            &[
                "for-each-ref",
                "--format=%(objectname)",
                "refs/commitguard/backups/"
            ],
            &s.repo,
            &[]
        ),
        s.source
    );
}

#[test]
fn credential_bearing_http_remote_is_rejected_without_persisting_secret() {
    let s = Setup::new();
    s.f.raw(
        &[
            "remote",
            "set-url",
            "origin",
            "https://dummy-secret@example.invalid/repo.git",
        ],
        &s.repo,
        &[],
    );
    let output = s.export(&s.plan);
    refused(output.clone());
    assert!(!s.plan.exists());
    assert!(!String::from_utf8_lossy(&output.stderr).contains("dummy-secret"));
    assert!(!String::from_utf8_lossy(&output.stdout).contains("dummy-secret"));
    assert_eq!(s.tip(), s.source);
}

#[test]
fn receipt_rejects_changed_credentials_even_for_same_account() {
    let s = Setup::new();
    s.approve();
    let changed = [("GH_TOKEN", "different-fixture-token")];
    accepted(s.f.canonical(&["--strict", "account"], &s.repo, &changed, None));
    refused(s.f.canonical(
        &["fix", "--preview", s.plan.to_str().unwrap()],
        &s.repo,
        &changed,
        None,
    ));
    refused(s.f.canonical(
        &["fix", "--apply", s.plan.to_str().unwrap()],
        &s.repo,
        &changed,
        None,
    ));
    assert_eq!(s.tip(), s.source);
    assert!(!s.repo.join(".git/commitguard-fix/lock").exists());
}
