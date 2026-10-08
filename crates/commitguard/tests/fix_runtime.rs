#![cfg(unix)]
//! Public acceptance contract for receipt-bound repair and Author migration.
//!
//! Proposed editable JSON (no inspection/identity/ownership fields):
//! {"plan_id":"<64 lowercase hex>","candidates":[
//!   {"source_oid":"<full oid>","message":"<proposed bytes>"}]}
//! Ownership JSON: {"sources":[{"source_oid":"<full oid>",
//!   "old_author":"<exact raw author header value, including date>","owned":true}]}.
//! The immutable native receipt, not either external file, authorizes apply.
//! Preview prints source OIDs, old/new identities and a labeled 64-hex apply
//! digest. Durable successful journal JSON retains `backup_ref` and `mapping`
//! entries {"source_oid":"...","new_oid":"..."}; its filename is not prescribed.
//! These are explicit schema candidates for implementation, not private Rust APIs.
mod common;
use common::*;
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Output,
};

const IDENTITY: &str = "tester <44+tester@users.noreply.github.com>";
const OLD_NAME: &str = "mistaken tester";
const OLD_EMAIL: &str = "mistake@example.test";
const CREDIT: &str = "Co-authored-by: Claude <noreply@anthropic.com>";
const DATES: [&str; 2] = ["1000000000 +0530", "1000000060 -0700"];

type HistorySnapshot = ((String, String, String, Option<Vec<u8>>), String);

struct History {
    f: Fixture,
    repo: PathBuf,
    remote: PathBuf,
    base: String,
    sources: Vec<String>,
    plan: PathBuf,
}

impl History {
    fn new(wrong_authors: &[usize], installed: bool) -> Self {
        let f = Fixture::new();
        let repo = f.repo("repair source with spaces");
        fs::write(repo.join("tracked"), "base\n").unwrap();
        f.raw(&["add", "tracked"], &repo, &[]);
        let base = f.commit(&repo, "feat: establish published base", &[]);
        let remote = f.bare("published base remote");
        let url = remote.to_str().unwrap();
        f.raw(&["remote", "add", "origin", url], &repo, &[]);
        // Fixture-only publication: no network and no actual project refs.
        f.raw(&["push", "-q", "origin", "main"], &repo, &[]);
        let messages = [
            format!("not conventional {}\n\n{CREDIT}", "x".repeat(130)),
            "another malformed message".into(),
        ];
        let mut sources = Vec::new();
        for (i, message) in messages.iter().enumerate() {
            if i == 0 {
                fs::write(repo.join("tracked"), "first unpublished change\n").unwrap();
                f.raw(&["add", "tracked"], &repo, &[]);
            } // The second source is deliberately empty, with a different date.
            let mut extra = vec![
                ("GIT_AUTHOR_DATE", DATES[i]),
                ("GIT_COMMITTER_DATE", "1000000200 +0000"),
                ("GIT_COMMITTER_NAME", "old committer"),
                ("GIT_COMMITTER_EMAIL", "old@example.test"),
            ];
            if wrong_authors.contains(&i) {
                extra.extend([
                    ("GIT_AUTHOR_NAME", OLD_NAME),
                    ("GIT_AUTHOR_EMAIL", OLD_EMAIL),
                ]);
            }
            sources.push(f.commit(&repo, message, &extra));
        }
        if installed {
            accepted(f.canonical_install(&[]));
        }
        let plan = f.root.join("candidate messages.json");
        Self {
            f,
            repo,
            remote,
            base,
            sources,
            plan,
        }
    }

    fn fix(&self, args: &[&str], extra: &[(&str, &str)]) -> Output {
        let mut argv = vec!["fix"];
        argv.extend_from_slice(args);
        self.f.canonical(&argv, &self.repo, extra, None)
    }

    fn export(&self, migration: Option<&Path>) {
        let before = self.snapshot();
        let range = format!("{}..HEAD", self.base);
        let mut args = vec!["--range", &range, "--plan", self.plan.to_str().unwrap()];
        if let Some(ownership) = migration {
            args.extend(["--author", "gh", "--ownership", ownership.to_str().unwrap()]);
        }
        accepted(self.fix(&args, &[]));
        assert_eq!(
            self.snapshot(),
            before,
            "planning must not mutate Git state"
        );
    }

    fn proposal(&self) -> Value {
        serde_json::from_slice(&fs::read(&self.plan).unwrap()).unwrap()
    }

    fn write_proposal(&self, value: &Value) {
        fs::write(&self.plan, serde_json::to_vec_pretty(value).unwrap()).unwrap();
    }

    fn approve(&self) {
        let mut proposal = self.proposal();
        for (entry, message) in proposal["candidates"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .zip(self.messages())
        {
            entry["message"] = json!(message);
        }
        self.write_proposal(&proposal);
    }

    fn messages(&self) -> [String; 2] {
        [
            format!("fix: record tracked change\n\n{CREDIT}"),
            "fix: retain empty checkpoint".into(),
        ]
    }

    fn preview(&self) -> Output {
        self.fix(&["--preview", self.plan.to_str().unwrap()], &[])
    }

    fn apply(&self, confirmation: Option<&str>, extra: &[(&str, &str)]) -> Output {
        let mut args = vec!["--apply", self.plan.to_str().unwrap()];
        if let Some(digest) = confirmation {
            args.extend(["--confirm-author-migration", digest]);
        }
        self.fix(&args, extra)
    }

    fn ownership(&self, indices: &[usize]) -> PathBuf {
        let path = self.f.root.join("owned sources.json");
        let sources: Vec<Value> = indices
            .iter()
            .map(|&i| {
                json!({
                    "source_oid": self.sources[i],
                    "old_author": header(&self.f, &self.repo, &self.sources[i], "author"),
                    "owned": true,
                })
            })
            .collect();
        fs::write(
            &path,
            serde_json::to_vec_pretty(&json!({"sources":sources})).unwrap(),
        )
        .unwrap();
        path
    }

    fn tip(&self) -> String {
        self.f.raw(&["rev-parse", "HEAD"], &self.repo, &[])
    }

    fn source_state(&self) -> (String, String, String, Option<Vec<u8>>) {
        (
            self.tip(),
            self.f.raw(&["write-tree"], &self.repo, &[]),
            self.f.raw(
                &["status", "--porcelain=v1", "--untracked-files=all"],
                &self.repo,
                &[],
            ),
            match fs::read(self.repo.join("tracked")) {
                Ok(bytes) => Some(bytes),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
                Err(e) => panic!("cannot inspect fixture tracked data: {e}"),
            },
        )
    }

    fn snapshot(&self) -> HistorySnapshot {
        (
            self.source_state(),
            self.f.raw(&["show-ref"], &self.repo, &[]),
        )
    }

    fn refuse_without_promotion(
        &self,
        output: Output,
        before: &(String, String, String, Option<Vec<u8>>),
    ) {
        refused(output);
        assert_eq!(
            &self.source_state(),
            before,
            "failure must leave the source ref/index/files alone"
        );
        assert_eq!(
            self.f
                .raw(&["rev-parse", "refs/heads/main"], &self.remote, &[]),
            self.base
        );
    }

    fn verify_success(&self) {
        let oids = self.f.raw(
            &["rev-list", "--reverse", &format!("{}..HEAD", self.base)],
            &self.repo,
            &[],
        );
        let new: Vec<&str> = oids.lines().collect();
        assert_eq!(
            new.len(),
            self.sources.len(),
            "empty commits must not disappear"
        );
        assert_ne!(self.tip(), *self.sources.last().unwrap());
        let mut parent = self.base.clone();
        for ((old, new), message) in self
            .sources
            .iter()
            .zip(new.iter().copied())
            .zip(self.messages())
        {
            assert_ne!(old, new);
            assert_eq!(header(&self.f, &self.repo, new, "parent"), parent);
            assert_eq!(
                header(&self.f, &self.repo, old, "tree"),
                header(&self.f, &self.repo, new, "tree")
            );
            for kind in ["author", "committer"] {
                let identity = header(&self.f, &self.repo, new, kind);
                assert_eq!(identity.rsplitn(3, ' ').last().unwrap(), IDENTITY);
            }
            let author_date = |oid: &str| {
                let value = header(&self.f, &self.repo, oid, "author");
                value
                    .rsplitn(3, ' ')
                    .take(2)
                    .collect::<Vec<_>>()
                    .into_iter()
                    .rev()
                    .collect::<Vec<_>>()
                    .join(" ")
            };
            assert_eq!(author_date(old), author_date(new));
            let committer = header(&self.f, &self.repo, new, "committer");
            let timestamp: i64 = committer.rsplit(' ').nth(1).unwrap().parse().unwrap();
            assert!(
                timestamp > 1000000200,
                "new Committer dates must be generated, not copied"
            );
            let raw = raw_commit(&self.f, &self.repo, new);
            let (headers, actual_message) = raw.split_once("\n\n").unwrap();
            assert!(!headers.lines().any(|line| line.starts_with("gpgsig")));
            assert_eq!(
                actual_message,
                format!("{message}\n"),
                "created bytes must equal approved normalization"
            );
            accepted(self.f.canonical(&["commits", new], &self.repo, &[], None));
            parent = new.to_string();
        }
        assert_eq!(
            header(&self.f, &self.repo, new[0], "tree"),
            header(&self.f, &self.repo, new[1], "tree")
        );
        assert_eq!(
            self.f.raw(&["status", "--porcelain=v1"], &self.repo, &[]),
            ""
        );
        assert_eq!(
            fs::read(self.repo.join("tracked")).unwrap(),
            b"first unpublished change\n"
        );
        assert_eq!(
            self.f
                .raw(&["rev-parse", "refs/heads/main"], &self.remote, &[]),
            self.base,
            "apply never pushes"
        );
        self.verify_backup_mapping(&new);
    }

    fn verify_backup_mapping(&self, new: &[&str]) {
        let refs = self.f.raw(
            &[
                "for-each-ref",
                "--format=%(refname) %(objectname)",
                "refs/commitguard/backups/",
            ],
            &self.repo,
            &[],
        );
        let backups: Vec<&str> = refs.lines().collect();
        assert_eq!(
            backups.len(),
            1,
            "successful apply retains one recovery backup"
        );
        let (backup_ref, old_tip) = backups[0].split_once(' ').unwrap();
        assert_eq!(old_tip, self.sources.last().unwrap());
        let git_dir = self
            .f
            .raw(&["rev-parse", "--absolute-git-dir"], &self.repo, &[]);
        let mut documents = Vec::new();
        json_documents(Path::new(&git_dir), &mut documents);
        let expected: Vec<Value> = self
            .sources
            .iter()
            .zip(new)
            .map(|(old, new)| {
                json!({
                    "source_oid": old, "new_oid": new,
                })
            })
            .collect();
        assert!(
            documents.iter().any(|value| {
                value["backup_ref"] == backup_ref
                    && value["mapping"].as_array().is_some_and(|mapping| {
                        mapping.len() == expected.len()
                            && mapping.iter().zip(&expected).all(|(entry, expected)| {
                                entry["source_oid"] == expected["source_oid"]
                                    && entry["new_oid"] == expected["new_oid"]
                            })
                    })
            }),
            "durable journal must retain the complete verified source/result mapping and backup"
        );
    }
}

fn raw_commit(f: &Fixture, repo: &Path, oid: &str) -> String {
    let output = f.command(&f.git, &["cat-file", "commit", oid], repo, &[], None);
    assert!(output.status.success());
    String::from_utf8(output.stdout).unwrap()
}

fn header(f: &Fixture, repo: &Path, oid: &str, name: &str) -> String {
    let raw = raw_commit(f, repo, oid);
    let prefix = format!("{name} ");
    let values: Vec<&str> = raw
        .split_once("\n\n")
        .unwrap()
        .0
        .lines()
        .filter_map(|line| line.strip_prefix(&prefix))
        .collect();
    assert_eq!(values.len(), 1, "expected exactly one {name} header");
    values[0].into()
}

fn json_documents(dir: &Path, documents: &mut Vec<Value>) {
    for entry in fs::read_dir(dir).unwrap() {
        let entry = entry.unwrap();
        let kind = entry.file_type().unwrap();
        let path = entry.path();
        if kind.is_dir() {
            // Native operation metadata lives under the Git directory, not in objects.
            if entry.file_name() != "objects" && entry.file_name() != "logs" {
                json_documents(&path, documents);
            }
        } else if kind.is_file()
            && entry.metadata().unwrap().len() <= 4 * 1024 * 1024
            && let Ok(value) = serde_json::from_slice(&fs::read(path).unwrap())
        {
            documents.push(value);
        }
    }
}

fn apply_digest(output: Output, sources: &[String]) -> String {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let text = String::from_utf8(output.stdout).unwrap();
    for oid in sources {
        assert!(
            text.contains(oid),
            "preview must identify every full source OID"
        );
    }
    assert!(
        text.contains(IDENTITY),
        "preview must show the canonical destination"
    );
    // Permit human-readable or JSON output, but not an unlabeled plan hash.
    let pattern = regex::Regex::new(r"(?i)apply[-_ ]digest[^0-9a-f\n]*([0-9a-f]{64})").unwrap();
    let captures = pattern
        .captures(&text)
        .expect("preview must expose its labeled apply digest");
    captures[1].to_ascii_lowercase()
}

#[test]
fn export_two_unpublished_messages_and_preview_without_git_mutation() {
    let h = History::new(&[], false);
    let before = h.snapshot();
    h.export(None);
    let proposal = h.proposal();
    let object = proposal.as_object().unwrap();
    assert_eq!(
        object.len(),
        2,
        "editable JSON is plan ID and candidates only"
    );
    let id = proposal["plan_id"].as_str().unwrap();
    assert_eq!(id.len(), 64);
    assert!(
        id.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    );
    let entries = proposal["candidates"].as_array().unwrap();
    assert_eq!(entries.len(), 2);
    for (entry, oid) in entries.iter().zip(&h.sources) {
        assert_eq!(entry.as_object().unwrap().len(), 2);
        assert_eq!(entry["source_oid"], *oid);
        assert_eq!(
            entry["message"].as_str().unwrap().trim_end_matches('\n'),
            raw_commit(&h.f, &h.repo, oid)
                .split_once("\n\n")
                .unwrap()
                .1
                .trim_end_matches('\n')
        );
    }
    refused(h.preview());
    assert_eq!(h.snapshot(), before);
    h.approve();
    let digest = apply_digest(h.preview(), &h.sources);
    assert_eq!(
        digest,
        apply_digest(h.preview(), &h.sources),
        "unchanged preview is deterministic"
    );
    assert_eq!(h.snapshot(), before);
    let contents = fs::read(&h.plan).unwrap();
    let range = format!("{}..HEAD", h.base);
    refused(h.fix(
        &["--range", &range, "--plan", h.plan.to_str().unwrap()],
        &[],
    ));
    assert_eq!(
        fs::read(&h.plan).unwrap(),
        contents,
        "export must not overwrite a proposal"
    );
}

#[test]
fn invalid_proposals_and_removed_credit_refuse_preview_and_apply() {
    let h = History::new(&[], true);
    h.export(None);
    h.approve();
    let valid = h.proposal();
    let before = h.snapshot();
    for message in [
        format!("not a conventional commit\n\n{CREDIT}"),
        format!("fix: 日本語\n\n{CREDIT}"),
        format!("fix: {}\n\n{CREDIT}", "a".repeat(124)),
        "fix: record tracked change".to_string(), // Cannot delete the frozen credit.
        format!(
            "fix: record tracked change\n\n{CREDIT}\nCo-authored-by: Other <other@example.test>"
        ),
    ] {
        let mut proposal = valid.clone();
        proposal["candidates"][0]["message"] = json!(message);
        h.write_proposal(&proposal);
        refused(h.preview());
        h.refuse_without_promotion(h.apply(None, &[]), &before.0);
        assert_eq!(
            h.snapshot(),
            before,
            "invalid candidates must fail before backup/replay"
        );
    }
    h.write_proposal(&valid);
    apply_digest(h.preview(), &h.sources);
}

#[test]
fn preview_enforces_whole_message_128_boundary_and_binds_normalized_bytes() {
    let h = History::new(&[], true);
    h.export(None);
    h.approve();
    let before = h.snapshot();
    let mut proposal = h.proposal();
    let message = format!("fix: {}", "a".repeat(123));
    assert_eq!(message.len(), 128);
    proposal["candidates"][1]["message"] = json!(message);
    h.write_proposal(&proposal);
    let digest = apply_digest(h.preview(), &h.sources);
    proposal["candidates"][1]["message"] = json!(format!("{message}\n\n"));
    h.write_proposal(&proposal);
    assert_eq!(
        digest,
        apply_digest(h.preview(), &h.sources),
        "final LF terminators are normalized before digesting"
    );
    proposal["candidates"][1]["message"] = json!(format!("{message}a"));
    h.write_proposal(&proposal);
    refused(h.preview());
    h.refuse_without_promotion(h.apply(None, &[]), &before.0);
    assert_eq!(h.snapshot(), before);
}

#[test]
fn duplicate_missing_extra_tampered_and_unknown_plan_fields_are_rejected() {
    let h = History::new(&[], true);
    h.export(None);
    h.approve();
    let valid = h.proposal();
    let mut cases = Vec::new();
    let mut duplicate = valid.clone();
    duplicate["candidates"][1] = duplicate["candidates"][0].clone();
    cases.push(duplicate);
    let mut missing = valid.clone();
    missing["candidates"].as_array_mut().unwrap().pop();
    cases.push(missing);
    let mut extra = valid.clone();
    extra["candidates"]
        .as_array_mut()
        .unwrap()
        .push(json!({"source_oid":h.base,"message":"fix: rewrite base"}));
    cases.push(extra);
    let mut changed_oid = valid.clone();
    changed_oid["candidates"][0]["source_oid"] = json!(h.base);
    cases.push(changed_oid);
    let mut id = valid.clone();
    id["plan_id"] = json!("0".repeat(64));
    cases.push(id);
    let mut injected = valid.clone();
    injected["author"] = json!("attacker <attacker@example.test>");
    cases.push(injected);
    let mut candidate_field = valid.clone();
    candidate_field["candidates"][0]["owned"] = json!(true);
    cases.push(candidate_field);
    let mut version = valid.clone();
    version["schema_version"] = json!(999);
    cases.push(version);
    let before = h.snapshot();
    for proposal in cases {
        h.write_proposal(&proposal);
        refused(h.preview());
        h.refuse_without_promotion(h.apply(None, &[]), &before.0);
        assert_eq!(h.snapshot(), before);
    }
    // A duplicate JSON key must not be silently collapsed by deserialization.
    let encoded = serde_json::to_string(&valid).unwrap();
    let duplicate_key = encoded.replacen("{", "{\"plan_id\":\"bad\",", 1);
    fs::write(&h.plan, duplicate_key).unwrap();
    refused(h.preview());
    h.refuse_without_promotion(h.apply(None, &[]), &before.0);
}

#[test]
fn guarded_repair_preserves_trees_count_empty_author_dates_and_recovery_mapping() {
    let h = History::new(&[], true);
    h.export(None);
    h.approve();
    apply_digest(h.preview(), &h.sources);
    accepted(h.apply(None, &[]));
    h.verify_success();
    let after = h.snapshot();
    // A receipt for old sources is not a license for another rewrite.
    let retry = h.apply(None, &[]);
    assert_eq!(h.snapshot(), after);
    // Either an explicit no-op or a stale/finished-operation refusal is safe.
    if retry.status.success() {
        assert!(
            !retry.stdout.is_empty(),
            "successful retry must report its no-op"
        );
    }
}

#[test]
fn wrong_author_is_not_inferred_as_owned_by_default() {
    let h = History::new(&[0], true);
    let range = format!("{}..HEAD", h.base);
    let before = h.snapshot();
    refused(h.fix(
        &["--range", &range, "--plan", h.plan.to_str().unwrap()],
        &[],
    ));
    assert_eq!(h.snapshot(), before);
    assert!(!h.plan.exists());
}

#[test]
fn explicit_exact_owned_author_migration_requires_current_digest_and_freezes_ownership() {
    let h = History::new(&[0, 1], true);
    let ownership = h.ownership(&[0, 1]);
    h.export(Some(&ownership));
    h.approve();
    let preview = h.preview();
    let text = String::from_utf8_lossy(&preview.stdout);
    assert!(
        text.contains(&format!("{OLD_NAME} <{OLD_EMAIL}>")),
        "migration preview must expose the old Author"
    );
    let digest = apply_digest(preview, &h.sources);
    let before = h.source_state();
    h.refuse_without_promotion(h.apply(None, &[]), &before);
    h.refuse_without_promotion(h.apply(Some(&"0".repeat(64)), &[]), &before);
    let mut changed = h.proposal();
    changed["candidates"][1]["message"] = json!("fix: retain deliberate empty checkpoint");
    h.write_proposal(&changed);
    let changed_digest = apply_digest(h.preview(), &h.sources);
    assert_ne!(
        digest, changed_digest,
        "confirmation must bind edited messages"
    );
    h.refuse_without_promotion(h.apply(Some(&digest), &[]), &before);
    h.approve();
    assert_eq!(digest, apply_digest(h.preview(), &h.sources));
    // The external declaration is not read as new authority at apply time.
    fs::write(&ownership, b"{\"sources\":[]}").unwrap();
    accepted(h.apply(Some(&digest), &[]));
    h.verify_success();
}

#[test]
fn migration_refuses_undeclared_wrong_author_and_inexact_or_unowned_declarations() {
    for case in ["missing", "inexact", "unowned", "extra"] {
        let h = History::new(&[0, 1], true);
        let ownership = h.ownership(if case == "missing" { &[0] } else { &[0, 1] });
        let mut value: Value = serde_json::from_slice(&fs::read(&ownership).unwrap()).unwrap();
        match case {
            "inexact" => {
                value["sources"][1]["old_author"] =
                    json!(format!("{OLD_NAME} <{OLD_EMAIL}> 1 +0000"))
            }
            "unowned" => value["sources"][1]["owned"] = json!(false),
            "extra" => value["sources"].as_array_mut().unwrap().push(json!({
                "source_oid":h.base,"old_author":header(&h.f,&h.repo,&h.base,"author"),"owned":true
            })),
            _ => (),
        }
        fs::write(&ownership, serde_json::to_vec(&value).unwrap()).unwrap();
        let range = format!("{}..HEAD", h.base);
        let before = h.snapshot();
        refused(h.fix(
            &[
                "--range",
                &range,
                "--author",
                "gh",
                "--ownership",
                ownership.to_str().unwrap(),
                "--plan",
                h.plan.to_str().unwrap(),
            ],
            &[],
        ));
        assert_eq!(h.snapshot(), before);
        assert!(!h.plan.exists());
    }
}

#[test]
fn changed_account_or_failed_authentication_invalidates_frozen_plan() {
    let h = History::new(&[], true);
    h.export(None);
    h.approve();
    let before = h.snapshot();
    for extra in [
        [("FIXTURE_GH_ACCOUNT", "switched")],
        [("FIXTURE_GH_FAIL", "1")],
    ] {
        refused(h.fix(&["--preview", h.plan.to_str().unwrap()], &extra));
        h.refuse_without_promotion(h.apply(None, &extra), &before.0);
        assert_eq!(h.snapshot(), before);
    }
}

#[test]
fn changed_tip_is_not_overwritten_by_apply() {
    let h = History::new(&[], true);
    h.export(None);
    h.approve();
    accepted(h.f.guarded(
        &[
            "commit",
            "--allow-empty",
            "-m",
            "fix: concurrent checkpoint",
        ],
        &h.repo,
        &[],
    ));
    let before = h.snapshot();
    refused(h.preview());
    h.refuse_without_promotion(h.apply(None, &[]), &before.0);
    assert_eq!(h.snapshot(), before);
}

#[test]
fn apply_requires_guard_configuration_without_installing_it_implicitly() {
    let h = History::new(&[], false);
    h.export(None);
    h.approve();
    let before = h.snapshot();
    let global = fs::read(&h.f.global).unwrap();
    h.refuse_without_promotion(h.apply(None, &[]), &before.0);
    assert_eq!(h.snapshot(), before);
    assert_eq!(fs::read(&h.f.global).unwrap(), global);
    assert!(!h.f.guard_root().exists());
}

#[test]
fn original_commit_hooks_run_and_veto_or_mutation_cannot_promote() {
    for (name, body) in [
        ("pre-commit", "exit 23\n".to_string()),
        (
            "commit-msg",
            format!("printf 'fix: hook changed approved message\\n\\n{CREDIT}\\n' > \"$1\"\n"),
        ),
        (
            "pre-commit",
            "printf 'hook tree mutation\\n' > tracked\ngit add tracked\n".to_string(),
        ),
    ] {
        let h = History::new(&[], true);
        let marker = h.f.root.join("original hook ran");
        executable(
            &h.repo.join(".git/hooks").join(name),
            &format!(
                "#!/bin/sh\nprintf 'ran\\n' >> '{}'\n{body}",
                marker.display()
            ),
        );
        h.export(None); // Freeze the hook before planning, not after it.
        h.approve();
        let before = h.source_state();
        h.refuse_without_promotion(h.apply(None, &[]), &before);
        assert!(marker.exists(), "the original hook must execute in staging");
        assert_eq!(
            h.f.raw(
                &[
                    "for-each-ref",
                    "--format=%(objectname)",
                    "refs/commitguard/backups/"
                ],
                &h.repo,
                &[]
            ),
            *h.sources.last().unwrap(),
            "failed replay retains its backup"
        );
    }
}

#[test]
fn hook_chain_change_after_plan_is_stale_not_a_new_execution_authority() {
    let h = History::new(&[], true);
    let hook = h.repo.join(".git/hooks/pre-commit");
    executable(&hook, "#!/bin/sh\nexit 0\n");
    h.export(None);
    h.approve();
    let marker = h.f.root.join("changed hook ran");
    executable(
        &hook,
        &format!("#!/bin/sh\nprintf ran > '{}'\n", marker.display()),
    );
    let before = h.snapshot();
    h.refuse_without_promotion(h.apply(None, &[]), &before.0);
    assert_eq!(h.snapshot(), before);
    assert!(!marker.exists(), "do not execute a substituted hook chain");
}

#[test]
fn changed_guard_configuration_is_rejected_before_replay() {
    let h = History::new(&[], true);
    h.export(None);
    h.approve();
    let path = h.f.guard_root().join("config.json");
    let mut config: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    let replacement_hooks = h.f.root.join("substituted original hooks");
    fs::create_dir(&replacement_hooks).unwrap();
    config["previous_hooks"] = json!(replacement_hooks);
    fs::write(&path, serde_json::to_vec_pretty(&config).unwrap()).unwrap();
    let before = h.snapshot();
    h.refuse_without_promotion(h.apply(None, &[]), &before.0);
    assert_eq!(h.snapshot(), before);
}

#[test]
fn published_tag_and_second_push_destination_refuse_inspection() {
    for tag in [false, true] {
        let h = History::new(&[], true);
        let published = h.f.bare("second published destination");
        let url = published.to_str().unwrap();
        // The independent fixture remote imports objects and sets its own ref.
        // Do not disable the source's correctly rejecting pre-push guard.
        h.f.raw(
            &["fetch", "-q", h.repo.to_str().unwrap(), &h.sources[0]],
            &published,
            &[],
        );
        let destination = if tag {
            "refs/tags/published-source"
        } else {
            "refs/heads/published"
        };
        h.f.raw(&["update-ref", destination, &h.sources[0]], &published, &[]);
        h.f.raw(
            &[
                "remote",
                "set-url",
                "--add",
                "--push",
                "origin",
                h.remote.to_str().unwrap(),
            ],
            &h.repo,
            &[],
        );
        h.f.raw(
            &["remote", "set-url", "--add", "--push", "origin", url],
            &h.repo,
            &[],
        );
        let before = h.snapshot();
        let range = format!("{}..HEAD", h.base);
        refused(h.fix(
            &["--range", &range, "--plan", h.plan.to_str().unwrap()],
            &[],
        ));
        assert_eq!(h.snapshot(), before);
    }
}

#[test]
fn publication_changes_after_plan_are_rechecked_before_apply() {
    let h = History::new(&[], true);
    h.export(None);
    h.approve();
    // Simulate another actor publishing the selected source to the fixture remote.
    h.f.raw(
        &[
            "fetch",
            "-q",
            h.repo.to_str().unwrap(),
            h.sources.last().unwrap(),
        ],
        &h.remote,
        &[],
    );
    h.f.raw(
        &["update-ref", "refs/heads/main", h.sources.last().unwrap()],
        &h.remote,
        &[],
    );
    let before = h.source_state();
    refused(h.apply(None, &[]));
    assert_eq!(h.source_state(), before);
    assert_eq!(
        h.f.raw(&["rev-parse", "refs/heads/main"], &h.remote, &[]),
        h.tip()
    );
}

#[test]
fn unknown_advertised_object_and_zero_or_unreachable_remotes_fail_closed() {
    for case in ["unknown", "none", "unreachable"] {
        let h = History::new(&[], true);
        h.export(None);
        h.approve();
        match case {
            "unknown" => {
                let foreign = h.f.repo("unknown remote objects");
                h.f.commit(&foreign, "feat: unrelated remote history", &[]);
                h.f.raw(
                    &[
                        "push",
                        "-q",
                        h.remote.to_str().unwrap(),
                        "main:refs/heads/unknown",
                    ],
                    &foreign,
                    &[],
                );
            }
            "none" => {
                h.f.raw(&["remote", "remove", "origin"], &h.repo, &[]);
            }
            "unreachable" => {
                h.f.raw(
                    &[
                        "remote",
                        "set-url",
                        "origin",
                        h.f.root.join("missing remote").to_str().unwrap(),
                    ],
                    &h.repo,
                    &[],
                );
            }
            _ => unreachable!(),
        }
        let before = h.snapshot();
        h.refuse_without_promotion(h.apply(None, &[]), &before.0);
        assert_eq!(h.snapshot(), before);
    }
}

#[test]
fn in_worktree_proposal_is_rejected_before_any_file_is_created() {
    let h = History::new(&[], true);
    let range = format!("{}..HEAD", h.base);
    let path = h.repo.join("candidate.json");
    let before = h.snapshot();
    refused(h.fix(&["--range", &range, "--plan", path.to_str().unwrap()], &[]));
    assert!(!path.exists());
    assert_eq!(h.snapshot(), before);
}

#[test]
fn shared_branch_tag_stash_and_other_worktree_sources_refuse_planning() {
    for reference in ["refs/heads/other", "refs/tags/local-source", "refs/stash"] {
        let h = History::new(&[], true);
        h.f.raw(&["update-ref", reference, &h.sources[0]], &h.repo, &[]);
        let before = h.snapshot();
        let range = format!("{}..HEAD", h.base);
        refused(h.fix(
            &["--range", &range, "--plan", h.plan.to_str().unwrap()],
            &[],
        ));
        assert_eq!(h.snapshot(), before);
        assert!(!h.plan.exists());
    }
    let h = History::new(&[], true);
    let other = h.f.root.join("other detached worktree");
    h.f.raw(
        &[
            "worktree",
            "add",
            "--detach",
            other.to_str().unwrap(),
            &h.sources[0],
        ],
        &h.repo,
        &[],
    );
    let before = h.snapshot();
    let range = format!("{}..HEAD", h.base);
    refused(h.fix(
        &["--range", &range, "--plan", h.plan.to_str().unwrap()],
        &[],
    ));
    assert_eq!(h.snapshot(), before);
}

#[test]
fn annotated_live_tag_and_missing_tag_object_fail_closed() {
    for known in [false, true] {
        let h = History::new(&[], true);
        if known {
            h.f.raw(
                &[
                    "-c",
                    "tag.gpgsign=false",
                    "tag",
                    "-a",
                    "publication",
                    "-m",
                    "published",
                    &h.sources[0],
                ],
                &h.repo,
                &[],
            );
            h.f.raw(
                &[
                    "fetch",
                    "-q",
                    h.repo.to_str().unwrap(),
                    "refs/tags/publication:refs/tags/publication",
                ],
                &h.remote,
                &[],
            );
            // Remove the conflicting local tag; remote annotation still exists locally.
            h.f.raw(&["tag", "-d", "publication"], &h.repo, &[]);
        } else {
            let foreign = h.f.repo("unknown annotated tag origin");
            h.f.commit(&foreign, "feat: unrelated tag history", &[]);
            h.f.raw(
                &[
                    "-c",
                    "tag.gpgsign=false",
                    "tag",
                    "-a",
                    "publication",
                    "-m",
                    "published",
                ],
                &foreign,
                &[],
            );
            h.f.raw(
                &[
                    "push",
                    "-q",
                    h.remote.to_str().unwrap(),
                    "refs/tags/publication",
                ],
                &foreign,
                &[],
            );
        }
        let before = h.snapshot();
        let range = format!("{}..HEAD", h.base);
        refused(h.fix(
            &["--range", &range, "--plan", h.plan.to_str().unwrap()],
            &[],
        ));
        assert_eq!(h.snapshot(), before);
        assert!(!h.plan.exists());
    }
}

#[test]
fn original_relative_hook_directory_is_frozen_for_staging() {
    let h = History::new(&[], true);
    let hooks = h.repo.join(".git/relative original hooks");
    fs::create_dir(&hooks).unwrap();
    let marker = h.f.root.join("relative hook ran");
    executable(
        &hooks.join("pre-commit"),
        &format!("#!/bin/sh\nprintf ran > '{}'\nexit 29\n", marker.display()),
    );
    h.f.raw(
        &["config", "core.hooksPath", ".git/relative original hooks"],
        &h.repo,
        &[],
    );
    h.export(None);
    h.approve();
    let before = h.source_state();
    h.refuse_without_promotion(h.apply(None, &[]), &before);
    assert!(
        marker.exists(),
        "staging must preserve source-relative original hook context"
    );
}

#[test]
fn hook_dirty_staging_is_retained_and_retry_does_not_replay() {
    let h = History::new(&[], true);
    let marker = h.f.root.join("post commit calls");
    executable(
        &h.repo.join(".git/hooks/post-commit"),
        &format!(
            "#!/bin/sh\nprintf ran >> '{}'\nprintf evidence > hook-evidence\n",
            marker.display()
        ),
    );
    h.export(None);
    h.approve();
    let before = h.source_state();
    h.refuse_without_promotion(h.apply(None, &[]), &before);
    let calls = fs::read(&marker).unwrap();
    let worktrees = h.f.raw(&["worktree", "list", "--porcelain"], &h.repo, &[]);
    let paths: Vec<PathBuf> = worktrees
        .lines()
        .filter_map(|line| line.strip_prefix("worktree ").map(PathBuf::from))
        .collect();
    assert!(
        paths
            .iter()
            .any(|path| path != &h.repo && path.join("hook-evidence").exists()),
        "hook-created evidence must survive failure"
    );
    h.refuse_without_promotion(h.apply(None, &[]), &before);
    assert_eq!(
        fs::read(marker).unwrap(),
        calls,
        "retry must not repeat hook side effects"
    );
}

#[test]
fn migration_snapshot_removes_missing_paths_and_preserves_modes_and_symlinks() {
    use std::os::unix::fs::{PermissionsExt, symlink};
    let mut h = History::new(&[0, 1], false);
    // Extend the selected suffix with our own deliberately mistaken Author.
    fs::remove_file(h.repo.join("tracked")).unwrap();
    fs::write(h.repo.join("executable"), "#!/bin/sh\nexit 0\n").unwrap();
    fs::set_permissions(h.repo.join("executable"), fs::Permissions::from_mode(0o755)).unwrap();
    symlink("executable", h.repo.join("symbolic")).unwrap();
    h.f.raw(&["add", "-A"], &h.repo, &[]);
    let last = h.f.commit(
        &h.repo,
        "bad deletion checkpoint",
        &[
            ("GIT_AUTHOR_NAME", OLD_NAME),
            ("GIT_AUTHOR_EMAIL", OLD_EMAIL),
            ("GIT_AUTHOR_DATE", "1000000120 +0000"),
        ],
    );
    h.sources.push(last);
    accepted(h.f.canonical_install(&[]));
    let ownership = h.ownership(&[0, 1, 2]);
    h.export(Some(&ownership));
    let mut proposal = h.proposal();
    for (entry, message) in proposal["candidates"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .zip([
            h.messages()[0].clone(),
            h.messages()[1].clone(),
            "fix: remove tracked path and retain file modes".into(),
        ])
    {
        entry["message"] = json!(message);
    }
    h.write_proposal(&proposal);
    let digest = apply_digest(h.preview(), &h.sources);
    accepted(h.apply(Some(&digest), &[]));
    let new = h.f.raw(
        &["rev-list", "--reverse", &format!("{}..HEAD", h.base)],
        &h.repo,
        &[],
    );
    let mapped: Vec<&str> = new.lines().collect();
    assert_eq!(mapped.len(), 3);
    for (old, new) in h.sources.iter().zip(&mapped) {
        assert_eq!(
            header(&h.f, &h.repo, old, "tree"),
            header(&h.f, &h.repo, new, "tree")
        );
    }
    assert!(!h.repo.join("tracked").exists());
    assert_eq!(
        fs::read_link(h.repo.join("symbolic")).unwrap(),
        PathBuf::from("executable")
    );
    assert_ne!(
        fs::metadata(h.repo.join("executable"))
            .unwrap()
            .permissions()
            .mode()
            & 0o111,
        0
    );
    h.verify_backup_mapping(&mapped);
}

#[test]
fn hostile_rebase_and_editor_configuration_cannot_expand_the_todo() {
    let h = History::new(&[], true);
    for (key, value) in [
        ("rebase.instructionFormat", "%s\nexec must-not-run"),
        ("rebase.abbreviateCommands", "true"),
        ("rebase.autoSquash", "true"),
        ("rebase.updateRefs", "true"),
        ("rebase.autoStash", "true"),
        ("core.abbrev", "4"),
        ("core.editor", "must-not-run"),
        ("sequence.editor", "must-not-run"),
        ("core.commentChar", "!"),
        ("core.commentString", "!"),
    ] {
        h.f.raw(&["config", key, value], &h.repo, &[]);
    }
    h.export(None);
    h.approve();
    accepted(h.apply(None, &[]));
    h.verify_success();
}

#[test]
fn proposal_symlink_is_refused_without_following_it() {
    use std::os::unix::fs::symlink;
    let h = History::new(&[], true);
    h.export(None);
    h.approve();
    let target = h.f.root.join("real candidate bytes.json");
    fs::rename(&h.plan, &target).unwrap();
    symlink(&target, &h.plan).unwrap();
    let bytes = fs::read(&target).unwrap();
    let before = h.snapshot();
    refused(h.preview());
    h.refuse_without_promotion(h.apply(None, &[]), &before.0);
    assert_eq!(fs::read(target).unwrap(), bytes);
    assert_eq!(h.snapshot(), before);
}

#[test]
fn private_helpers_have_no_authority_without_active_journal() {
    let h = History::new(&[], true);
    let input = h.f.root.join("unrelated editable file");
    fs::write(&input, b"must remain untouched\n").unwrap();
    let before = h.snapshot();
    refused(h.fix(
        &[
            "--private-editor",
            h.f.root.to_str().unwrap(),
            &"0".repeat(64),
            "message",
            input.to_str().unwrap(),
        ],
        &[],
    ));
    assert_eq!(fs::read(input).unwrap(), b"must remain untouched\n");
    assert_eq!(h.snapshot(), before);
}

#[test]
fn unknown_or_conflicting_flags_and_arbitrary_destination_authors_are_rejected() {
    let h = History::new(&[0], false);
    let range = format!("{}..HEAD", h.base);
    let path = h.plan.to_str().unwrap();
    let ownership = h.ownership(&[0]);
    let before = h.snapshot();
    for args in [
        vec!["--range", &range, "--plan", path, "--surprise"],
        vec!["--range", &range, "--plan", path, "--apply", path],
        vec!["--preview", path, "--apply", path],
        vec![
            "--range",
            &range,
            "--author",
            "other",
            "--ownership",
            ownership.to_str().unwrap(),
            "--plan",
            path,
        ],
        vec!["--range", &range, "--author", "gh", "--plan", path],
        vec![
            "--range",
            &range,
            "--ownership",
            ownership.to_str().unwrap(),
            "--plan",
            path,
        ],
    ] {
        refused(h.fix(&args, &[]));
        assert_eq!(h.snapshot(), before);
        assert!(!h.plan.exists());
    }
}
