#![cfg(unix)]
mod common;
use common::*;
use std::fs;
#[test]
fn fresh_human_auth_and_effective_identity() {
    let f = Fixture::new();
    let repo = f.repo("identity");
    accepted(f.cli(&["identity"], &repo, &[], None));
    for extra in [
        [("FIXTURE_GH_ACCOUNT", "bot")],
        [("FIXTURE_GH_ACCOUNT", "missing-type")],
        [("FIXTURE_GH_ACCOUNT", "switched")],
        [("FIXTURE_GH_FAIL", "1")],
        [("GIT_AUTHOR_NAME", "foreign")],
        [("GIT_COMMITTER_EMAIL", "foreign@example.com")],
    ] {
        refused(f.cli(&["identity"], &repo, &extra, None));
    }
}
#[test]
fn message_auth_trailer_rules_are_stricter_than_lint() {
    let f = Fixture::new();
    let repo = f.repo("messages");
    let file = f.root.join("message");
    let arg = file.display().to_string();
    for message in [
        "fix: change\n\nCo-authored-by: Codex",
        "fix: change\n\nCo-authored-by: Claude <noreply@anthropic.com>",
        "fix: change\n\nSigned-off-by: tester <44+tester@users.noreply.github.com>",
    ] {
        fs::write(&file, message).unwrap();
        accepted(f.cli(&["message", &arg], &repo, &[], None));
    }
    f.raw(
        &["config", "trailer.co-authored-by.key", "Hidden"],
        &repo,
        &[],
    );
    f.raw(&["config", "trailer.separators", "#"], &repo, &[]);
    for message in [
        "fix: change\n\nCo-authored-by: Other <other@example.com>",
        "fix: change\n\nCo-authored-by: Other <other@example.com>\n\nDescription.",
        "fix: change\n\nSigned-off-by: Other <other@example.com>",
        "fix: 日本語",
    ] {
        fs::write(&file, message).unwrap();
        refused(f.cli(&["message", &arg], &repo, &[], None));
    }
}
#[test]
fn raw_author_and_committer_beat_mailmap() {
    let f = Fixture::new();
    let repo = f.repo("raw identities");
    for extra in [
        [("GIT_AUTHOR_NAME", "foreign")],
        [("GIT_COMMITTER_NAME", "foreign")],
    ] {
        let oid = f.commit(&repo, "fix: foreign identity", &extra);
        fs::write(repo.join(".mailmap"),"tester <44+tester@users.noreply.github.com> foreign <44+tester@users.noreply.github.com>\n").unwrap();
        refused(f.cli(&["commits", &oid], &repo, &[], None));
    }
}
#[test]
fn whole_128_and_129_chars_are_checked_on_raw_commit() {
    let f = Fixture::new();
    let repo = f.repo("length");
    let good = f.commit(&repo, &format!("fix: a\n\n{}", "x".repeat(120)), &[]);
    accepted(f.cli(&["commits", &good], &repo, &[], None));
    let bad = f.commit(&repo, &format!("fix: a\n\n{}", "x".repeat(121)), &[]);
    refused(f.cli(&["commits", &bad], &repo, &[], None));
}
#[test]
fn embedded_signature_duplicate_identity_and_encoding_are_rejected() {
    let f = Fixture::new();
    let repo = f.repo("raw headers");
    let oid = f.commit(&repo, "fix: base", &[]);
    let raw = f
        .command(&f.git, &["cat-file", "commit", &oid], &repo, &[], None)
        .stdout;
    let text = String::from_utf8(raw).unwrap();
    for addition in [
        "gpgsig -----BEGIN PGP SIGNATURE-----\n fixture\n -----END PGP SIGNATURE-----",
        "encoding ISO-8859-1",
        "author tester <44+tester@users.noreply.github.com> 1 +0000",
    ] {
        let modified = text.replacen("\n\n", &format!("\n{addition}\n\n"), 1);
        let result = f.command(
            &f.git,
            &["hash-object", "-t", "commit", "-w", "--stdin"],
            &repo,
            &[],
            Some(modified.as_bytes()),
        );
        assert!(result.status.success());
        let bad = String::from_utf8(result.stdout).unwrap();
        refused(f.cli(&["commits", bad.trim()], &repo, &[], None));
    }
}
#[test]
fn outgoing_intermediate_commit_is_checked_and_live_old_history_excluded() {
    let f = Fixture::new();
    let repo = f.repo("outgoing");
    let old = f.commit(&repo, "fix: foreign old", &[("GIT_AUTHOR_NAME", "foreign")]);
    let bare = f.bare("actual push remote");
    let url = bare.display().to_string();
    refused(f.cli(&["push", &url, "refs/heads/main"], &repo, &[], None));
    f.raw(&["push", "-q", &url, "main"], &repo, &[]);
    f.commit(&repo, "fix: new own", &[]);
    accepted(f.cli(&["push", &url, "refs/heads/main"], &repo, &[], None));
    f.commit(
        &repo,
        "fix: bad intermediate",
        &[("GIT_COMMITTER_NAME", "foreign")],
    );
    f.commit(&repo, "fix: own tip", &[]);
    refused(f.cli(&["push", &url, "refs/heads/main"], &repo, &[], None));
    assert_eq!(f.raw(&["rev-parse", "refs/heads/main"], &bare, &[]), old);
}
#[test]
fn prepush_uses_supplied_oid_even_when_branch_moves() {
    let f = Fixture::new();
    let repo = f.repo("exact ref");
    let bad = f.commit(&repo, "fix: foreign", &[("GIT_AUTHOR_NAME", "foreign")]);
    let empty = f.bare("empty");
    let url = empty.display().to_string();
    f.raw(&["checkout", "--orphan", "clean"], &repo, &[]);
    f.commit(&repo, "fix: clean tip", &[]);
    let payload = format!(
        "refs/heads/clean {bad} refs/heads/main {}\n",
        "0".repeat(40)
    );
    refused(f.cli(&["pre-push", &url], &repo, &[], Some(payload.as_bytes())));
}
#[test]
fn all_push_urls_are_checked_instead_of_only_first_remote() {
    let f = Fixture::new();
    let repo = f.repo("two destinations");
    f.commit(
        &repo,
        "fix: foreign history",
        &[("GIT_AUTHOR_NAME", "foreign")],
    );
    let published = f.bare("already published");
    let fresh = f.bare("fresh destination");
    let published = published.display().to_string();
    let fresh = fresh.display().to_string();
    f.raw(&["push", "-q", &published, "main"], &repo, &[]);
    f.raw(&["remote", "add", "origin", &published], &repo, &[]);
    f.raw(
        &["remote", "set-url", "--push", "origin", &published],
        &repo,
        &[],
    );
    f.raw(
        &["remote", "set-url", "--add", "--push", "origin", &fresh],
        &repo,
        &[],
    );
    f.commit(&repo, "fix: own new", &[]);
    refused(f.cli(&["push", "origin", "refs/heads/main"], &repo, &[], None));
}
#[test]
fn portable_message_clean_checks_editor_bytes_and_fresh_effective_identity() {
    let f = Fixture::new();
    let repo = f.repo("portable clean editor");
    let file = f.root.join("editor message");
    let content = "# Git editor comment\nfix: cleaned editor\n\n# ------------------------ >8 ------------------------\n+verbose diff beyond scissors\n";
    fs::write(&file, content).unwrap();
    let path = file.display().to_string();
    refused(f.cli(&["message", &path], &repo, &[], None));
    refused(f.cli(
        &["message", "--clean", &path],
        &repo,
        &[("GIT_COMMITTER_NAME", "foreign")],
        None,
    ));
    assert_eq!(fs::read_to_string(&file).unwrap(), content);
    accepted(f.cli(&["message", "--clean", &path], &repo, &[], None));
    assert_eq!(fs::read_to_string(&file).unwrap(), "fix: cleaned editor\n");
    accepted(f.cli(&["message", &path], &repo, &[], None));
}
#[test]
fn git_custom_trailer_aliases_enforce_foreign_identity_and_allow_recognized_ai() {
    let f = Fixture::new();
    let repo = f.repo("custom credits");
    f.raw(
        &["config", "trailer.credit.key", "Co-authored-by"],
        &repo,
        &[],
    );
    let file = f.root.join("aliased credit message");
    let path = file.display().to_string();
    fs::write(
        &file,
        "fix: aliased credit\n\nCredit: Other <other@example.com>",
    )
    .unwrap();
    refused(f.cli(&["message", &path], &repo, &[], None));
    let bad = f.commit(
        &repo,
        "fix: aliased credit\n\nCredit: Other <other@example.com>",
        &[],
    );
    refused(f.cli(&["commits", &bad], &repo, &[], None));
    fs::write(
        &file,
        "fix: aliased credit\n\nCredit: Claude <noreply@anthropic.com>",
    )
    .unwrap();
    accepted(f.cli(&["message", &path], &repo, &[], None));
}
