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

// Synthetic raw objects are written only inside this test's owned fixture.
// --literally ensures malformed extension headers reach the native checker.
fn origin_test_object(f: &Fixture, repo: &std::path::Path, raw: &str) -> String {
    let result = f.command(
        &f.git,
        &[
            "hash-object",
            "--literally",
            "-t",
            "commit",
            "-w",
            "--stdin",
        ],
        repo,
        &[],
        Some(raw.as_bytes()),
    );
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    String::from_utf8(result.stdout).unwrap().trim().into()
}

fn origin_test_raw(f: &Fixture, repo: &std::path::Path, oid: &str) -> String {
    let result = f.command(&f.git, &["cat-file", "commit", oid], repo, &[], None);
    assert!(result.status.success());
    String::from_utf8(result.stdout).unwrap()
}

fn malformed_origin_objects(raw: &str, digest: &str) -> Vec<(&'static str, String)> {
    let (headers, body) = raw.split_once("\n\n").unwrap();
    let append = |header: &str| format!("{headers}\n{header}\n\n{body}");
    let valid = format!("source-sha256 {digest}");
    vec![
        ("bare key", append("source-sha256")),
        ("tab delimiter", append(&format!("source-sha256\t{digest}"))),
        (
            "noncanonical key case",
            append(&format!("Source-Sha256 {digest}")),
        ),
        ("missing digest", append("source-sha256 ")),
        (
            "short digest",
            append(&format!("source-sha256 {}", &digest[..63])),
        ),
        ("long digest", append(&format!("source-sha256 {digest}0"))),
        (
            "uppercase hex",
            append(&format!("source-sha256 {}", digest.to_uppercase())),
        ),
        (
            "nonhex digest",
            append(&format!("source-sha256 {}g", &digest[..63])),
        ),
        (
            "extra separator",
            append(&format!("source-sha256  {digest}")),
        ),
        ("trailing space", append(&format!("{valid} "))),
        ("trailing text", append(&format!("{valid} extra"))),
        ("duplicate", append(&format!("{valid}\n{valid}"))),
        ("folded", append(&format!("{valid}\n continuation"))),
        (
            "not final",
            append(&format!("{valid}\nx-ordinary retained")),
        ),
        (
            "not adjacent to committer",
            append(&format!("x-ordinary retained\n{valid}")),
        ),
        (
            "before committer",
            format!(
                "{}\n\n{body}",
                headers.replacen("\ncommitter ", &format!("\n{valid}\ncommitter "), 1)
            ),
        ),
    ]
}

#[test]
fn source_sha256_actual_objects_accept_final_header_without_consuming_message_budget() {
    let f = Fixture::new();
    let repo = f.repo("origin raw valid");
    let ordinary = f.commit(&repo, &format!("fix: a\n\n{}", "x".repeat(120)), &[]);
    let raw = origin_test_raw(&f, &repo, &ordinary);
    let digest = "0123456789abcdef".repeat(4);
    let modified = raw.replacen("\n\n", &format!("\nsource-sha256 {digest}\n\n"), 1);
    let oid = origin_test_object(&f, &repo, &modified);
    assert_eq!(origin_test_raw(&f, &repo, &oid), modified);
    accepted(f.canonical(&["--strict", "commits", &oid], &repo, &[], None));
    accepted(f.cli(&["--strict", "commits", &oid], &repo, &[], None));
    refused(f.canonical(
        &["--strict", "commits", &oid],
        &repo,
        &[("FIXTURE_GH_ACCOUNT", "bot")],
        None,
    ));
    refused(f.canonical(
        &["--strict", "commits", &oid],
        &repo,
        &[("FIXTURE_GH_FAIL", "1")],
        None,
    ));
    let overlong = modified.replacen(&"x".repeat(120), &"x".repeat(121), 1);
    let overlong = origin_test_object(&f, &repo, &overlong);
    refused(f.canonical(&["--strict", "commits", &overlong], &repo, &[], None));
    let foreign = modified.replacen("\nauthor tester ", "\nauthor foreign ", 1);
    let foreign = origin_test_object(&f, &repo, &foreign);
    refused(f.canonical(&["--strict", "commits", &foreign], &repo, &[], None));
}

#[test]
fn source_sha256_actual_objects_refuse_each_malformed_known_header() {
    let f = Fixture::new();
    let repo = f.repo("origin raw malformed");
    let ordinary = f.commit(&repo, "fix: valid canonical message", &[]);
    let raw = origin_test_raw(&f, &repo, &ordinary);
    let digest = "0123456789abcdef".repeat(4);
    for (case, malformed) in malformed_origin_objects(&raw, &digest) {
        let oid = origin_test_object(&f, &repo, &malformed);
        let result = f.canonical(&["--strict", "commits", &oid], &repo, &[], None);
        assert!(
            !result.status.success(),
            "accepted malformed source-sha256 case: {case}"
        );
    }
}

#[test]
fn source_sha256_body_lookalikes_and_other_unknown_headers_keep_ordinary_behavior() {
    let f = Fixture::new();
    let repo = f.repo("origin body text");
    let ordinary = f.commit(
        &repo,
        "fix: describe metadata\n\nsource-sha256 NOT-A-HEADER\nsource-sha256",
        &[],
    );
    accepted(f.canonical(&["--strict", "commits", &ordinary], &repo, &[], None));
    let raw = origin_test_raw(&f, &repo, &ordinary);
    let unknown = raw.replacen("\n\n", "\nx-ordinary retained\n\n", 1);
    let oid = origin_test_object(&f, &repo, &unknown);
    accepted(f.canonical(&["--strict", "commits", &oid], &repo, &[], None));
}

#[test]
fn source_sha256_actual_guarded_push_rejects_malformed_outgoing_then_publishes_valid_header() {
    let f = Fixture::new();
    let repo = f.repo("origin actual push");
    let base = f.commit(&repo, "feat: published base", &[]);
    let remote = f.bare("origin destination");
    let url = remote.to_str().unwrap();
    f.raw(&["push", "-q", url, "main"], &repo, &[]);
    let source = f.commit(&repo, "fix: outgoing canonical change", &[]);
    let raw = origin_test_raw(&f, &repo, &source);
    let digest = "0123456789abcdef".repeat(4);
    accepted(f.canonical_install(&[]));
    for (case, malformed) in malformed_origin_objects(&raw, &digest) {
        let oid = origin_test_object(&f, &repo, &malformed);
        f.raw(&["update-ref", "refs/heads/main", &oid], &repo, &[]);
        let result = f.guarded(&["push", "-q", url, "main"], &repo, &[]);
        assert!(
            !result.status.success(),
            "pushed malformed source-sha256 case: {case}"
        );
        assert_eq!(
            f.raw(&["rev-parse", "refs/heads/main"], &remote, &[]),
            base,
            "remote changed for {case}"
        );
    }
    let valid = raw.replacen("\n\n", &format!("\nsource-sha256 {digest}\n\n"), 1);
    let valid = origin_test_object(&f, &repo, &valid);
    f.raw(&["update-ref", "refs/heads/main", &valid], &repo, &[]);
    refused(f.guarded(
        &["push", "-q", url, "main"],
        &repo,
        &[("FIXTURE_GH_FAIL", "1")],
    ));
    assert_eq!(f.raw(&["rev-parse", "refs/heads/main"], &remote, &[]), base);
    accepted(f.guarded(&["push", "-q", url, "main"], &repo, &[]));
    assert_eq!(
        f.raw(&["rev-parse", "refs/heads/main"], &remote, &[]),
        valid
    );
}

#[test]
fn source_sha256_text_in_mergetag_continuation_is_not_commit_metadata() {
    let f = Fixture::new();
    let repo = f.repo("origin nested tag text");
    let base = f.commit(&repo, "feat: parent", &[]);
    let source = f.commit(&repo, "fix: retain embedded tag text", &[]);
    let raw = origin_test_raw(&f, &repo, &source);
    let tag = format!(
        "mergetag object {base}\n type commit\n tag embedded\n tagger tester <44+tester@users.noreply.github.com> 1 +0000\n \n source-sha256 is only embedded tag prose\n"
    );
    let candidate = raw.replacen("\n\n", &format!("\n{tag}\n"), 1);
    let source = origin_test_object(&f, &repo, &candidate);
    accepted(f.canonical(&["--strict", "commits", &source], &repo, &[], None));
}

#[test]
fn source_sha256_normal_guarded_amend_retains_existing_origin() {
    let f = Fixture::new();
    let repo = f.repo("origin guarded amend");
    let source = f.commit(&repo, "fix: original fixture", &[]);
    let raw = origin_test_raw(&f, &repo, &source);
    let digest = "0123456789abcdef".repeat(4);
    let raw = raw.replacen("\n\n", &format!("\nsource-sha256 {digest}\n\n"), 1);
    let source = origin_test_object(&f, &repo, &raw);
    f.raw(&["update-ref", "refs/heads/main", &source], &repo, &[]);
    accepted(f.canonical_install(&[]));
    accepted(f.guarded(
        &[
            "-c",
            "commit.gpgsign=false",
            "commit",
            "--amend",
            "--allow-empty",
            "-m",
            "fix: amend approved fixture",
        ],
        &repo,
        &[],
    ));
    let new = f.raw(&["rev-parse", "HEAD"], &repo, &[]);
    assert_ne!(new, source);
    let raw = origin_test_raw(&f, &repo, &new);
    assert!(
        raw.contains(&format!("\nsource-sha256 {digest}\n\n")),
        "{raw}"
    );
    accepted(f.canonical(&["--strict", "commits", &new], &repo, &[], None));
}
