#![cfg(unix)]
mod common;
use common::{Fixture, accepted, executable, refused};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Output,
};
fn sha(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
struct Tags {
    f: Fixture,
    repo: PathBuf,
    manifest: PathBuf,
    api: PathBuf,
}
impl Tags {
    fn new() -> Self {
        let mut f = Fixture::new();
        let api = f.root.join("tag-api-count");
        f.env
            .insert("TAG_API_COUNT".into(), api.display().to_string());
        executable(
            &f.bin.join("gh"),
            r#"#!/bin/sh
case "$*" in
 'auth token --hostname github.com') echo fixture-token-tester;;
 'config get user --host github.com') echo tester;;
 'api --hostname github.com user') printf 'api\n' >> "$TAG_API_COUNT"; echo '{"login":"tester","id":44,"type":"User"}';;
 *) exit 78;;
esac
"#,
        );
        let repo = f.repo("tag source");
        let manifest = f.root.join("tag manifest.json");
        Self {
            f,
            repo,
            manifest,
            api,
        }
    }
    fn raw(&self, oid: &str, kind: &str) -> Vec<u8> {
        let r = self
            .f
            .command(&self.f.git, &["cat-file", kind, oid], &self.repo, &[], None);
        assert!(r.status.success());
        r.stdout
    }
    fn oid(&self, raw: &[u8], kind: &str, write: bool) -> String {
        let args = if write {
            vec!["hash-object", "-w", "-t", kind, "--stdin"]
        } else {
            vec!["hash-object", "-t", kind, "--stdin"]
        };
        let r = self
            .f
            .command(&self.f.git, &args, &self.repo, &[], Some(raw));
        assert!(r.status.success(), "{}", String::from_utf8_lossy(&r.stderr));
        String::from_utf8(r.stdout).unwrap().trim().into()
    }
    fn tag(&self, target: &str, kind: &str, name: &str, tagger: &str, body: &str) -> String {
        self.oid(
            format!("object {target}\ntype {kind}\ntag {name}\ntagger {tagger}\n\n{body}")
                .as_bytes(),
            "tag",
            true,
        )
    }
    fn candidate(&self, source: &str, target: Option<&str>, body: Option<&str>) -> Vec<u8> {
        let raw = String::from_utf8(self.raw(source, "tag")).unwrap();
        let (headers, old_body) = raw.split_once("\n\n").unwrap();
        let mut lines = headers.lines().map(str::to_string).collect::<Vec<_>>();
        if let Some(target) = target {
            lines[0] = format!("object {target}");
        }
        let date = lines[3].rsplitn(3, ' ').take(2).collect::<Vec<_>>();
        lines[3] = format!(
            "tagger tester <44+tester@users.noreply.github.com> {} {}",
            date[1], date[0]
        );
        format!("{}\n\n{}", lines.join("\n"), body.unwrap_or(old_body)).into_bytes()
    }
    fn entry(&self, source: &str, raw: &[u8]) -> Value {
        let expected = self.oid(raw, "tag", false);
        let path = self.f.root.join(format!("candidate-{expected}.tag"));
        fs::write(&path, raw).unwrap();
        json!({"source_oid":source,"source_sha256":sha(&self.raw(source,"tag")),"expected_oid":expected,"candidate_file":path,"candidate_sha256":sha(raw)})
    }
    fn common(&self) -> PathBuf {
        self.repo.join(".git").canonicalize().unwrap()
    }
    fn save(&self, entries: Vec<Value>, receipt: Option<&Path>) -> Value {
        let m = json!({"schema_version":1,"policy_version":1,"common_dir":self.common(),"commit_receipt":receipt,"entries":entries});
        self.write(&m);
        m
    }
    fn write(&self, m: &Value) {
        fs::write(&self.manifest, serde_json::to_vec(m).unwrap()).unwrap();
    }
    fn call(&self, digest: Option<&str>, strict: bool) -> Output {
        let mut args = Vec::new();
        if strict {
            args.push("--strict");
        }
        args.extend([
            "bulk-write-tags",
            "--manifest",
            self.manifest.to_str().unwrap(),
        ]);
        if let Some(d) = digest {
            args.extend(["--confirm", d]);
        }
        self.f.canonical(&args, &self.repo, &[], None)
    }
    fn digest(&self) -> String {
        let r = self.call(None, false);
        assert!(r.status.success(), "{}", String::from_utf8_lossy(&r.stderr));
        let v: Value = serde_json::from_slice(&r.stdout).unwrap();
        v["digest"].as_str().unwrap().into()
    }
    fn count(&self) -> usize {
        fs::read_to_string(&self.api)
            .unwrap_or_default()
            .lines()
            .count()
    }
    fn exists(&self, oid: &str) -> bool {
        self.f
            .command(&self.f.git, &["cat-file", "-e", oid], &self.repo, &[], None)
            .status
            .success()
    }
}
const CANON: &str = "tester <44+tester@users.noreply.github.com> 100 +0930";
#[test]
fn exact_owned_tag_metadata_preserves_unicode_prose_dates_refs_and_skips_hooks_api() {
    let t = Tags::new();
    let target = t.f.commit(
        &t.repo,
        "foreign source wording",
        &[
            ("GIT_AUTHOR_NAME", "Other Human"),
            ("GIT_AUTHOR_EMAIL", "other@example.com"),
        ],
    );
    let body = format!("日本語のリリース説明。{}\n", "長い説明。".repeat(40));
    let old_tagger = "Old Human <old@example.com> 100 +0930";
    let source = t.tag(&target, "commit", "v1", old_tagger, &body);
    t.f.raw(&["update-ref", "refs/tags/v1", &source], &t.repo, &[]);
    let candidate = t.candidate(&source, None, None);
    let e = t.entry(&source, &candidate);
    let expected = e["expected_oid"].as_str().unwrap().to_string();
    let mut m = t.save(vec![e], None);
    refused(t.call(None, false));
    m["entries"][0]["tagger_ownership"] = json!({"old_tagger":old_tagger,"owned":true});
    t.write(&m);
    let hooks = t.f.root.join("tag hooks");
    fs::create_dir(&hooks).unwrap();
    let marker = t.f.root.join("tag hook-ran");
    executable(
        &hooks.join("reference-transaction"),
        &format!("#!/bin/sh\nprintf ran > '{}'\nexit 1\n", marker.display()),
    );
    t.f.raw(
        &["config", "core.hooksPath", hooks.to_str().unwrap()],
        &t.repo,
        &[],
    );
    let refs = t.f.raw(&["show-ref"], &t.repo, &[]);
    let index = fs::read(t.repo.join(".git/index")).unwrap_or_default();
    let digest = t.digest();
    assert!(!t.exists(&expected));
    let r = t.call(Some(&digest), false);
    assert!(r.status.success(), "{}", String::from_utf8_lossy(&r.stderr));
    let v: Value = serde_json::from_slice(&r.stdout).unwrap();
    assert_eq!(v["phase"], "complete");
    assert_eq!(t.raw(&expected, "tag"), candidate);
    assert_eq!(t.f.raw(&["show-ref"], &t.repo, &[]), refs);
    assert_eq!(
        fs::read(t.repo.join(".git/index")).unwrap_or_default(),
        index
    );
    assert!(!marker.exists());
    assert_eq!(t.count(), 0);
    accepted(t.call(Some(&digest), false));
}
#[test]
fn tag_type_name_dates_prose_hashes_and_arbitrary_retargets_are_rejected() {
    for change in [
        "name",
        "type",
        "date",
        "prose",
        "target",
        "source_hash",
        "candidate_hash",
    ] {
        let t = Tags::new();
        let target = t.f.commit(&t.repo, "base", &[]);
        let other = t.f.commit(&t.repo, "other", &[]);
        let source = t.tag(&target, "commit", "v1", CANON, "original prose\n");
        let candidate = t.candidate(&source, None, None);
        let mut text = String::from_utf8(candidate).unwrap();
        match change {
            "name" => text = text.replace("tag v1", "tag v2"),
            "type" => text = text.replace("type commit", "type tag"),
            "date" => text = text.replace("100 +0930", "101 +0930"),
            "prose" => text = text.replace("original prose", "modified prose"),
            "target" => {
                text = text.replace(&format!("object {target}"), &format!("object {other}"))
            }
            _ => {}
        }
        let e = t.entry(&source, text.as_bytes());
        let expected = e["expected_oid"].as_str().unwrap().to_string();
        let mut m = t.save(vec![e], None);
        if change == "source_hash" {
            m["entries"][0]["source_sha256"] = "0".repeat(64).into();
        }
        if change == "candidate_hash" {
            m["entries"][0]["candidate_sha256"] = "0".repeat(64).into();
        }
        t.write(&m);
        refused(t.call(None, false));
        if expected != source {
            assert!(!t.exists(&expected));
        }
    }
}
#[test]
fn recognized_pgp_and_ssh_signature_stripping_requires_exact_permission() {
    for flavor in ["PGP", "SSH"] {
        let t = Tags::new();
        let target = t.f.commit(&t.repo, "base", &[]);
        let body = format!(
            "original prose\n-----BEGIN {flavor} SIGNATURE-----\nfixture\n-----END {flavor} SIGNATURE-----\n"
        );
        let source = t.tag(&target, "commit", "v1", CANON, &body);
        let mut m = t.save(
            vec![t.entry(
                &source,
                &t.candidate(&source, None, Some("original prose\n")),
            )],
            None,
        );
        refused(t.call(None, false));
        m["entries"][0]["remove_signature"] = true.into();
        t.write(&m);
        let digest = t.digest();
        accepted(t.call(Some(&digest), false));
        let body = format!(
            "original prose\n-----BEGIN {flavor} SIGNATURE-----\nfixture\n-----END {flavor} SIGNATURE-----\nmore prose\n"
        );
        let source = t.tag(&target, "commit", "v2", CANON, &body);
        let mut m = t.save(
            vec![t.entry(
                &source,
                &t.candidate(&source, None, Some("original prose\n")),
            )],
            None,
        );
        m["entries"][0]["remove_signature"] = true.into();
        t.write(&m);
        refused(t.call(None, false));
    }
    let t = Tags::new();
    let target = t.f.commit(&t.repo, "base", &[]);
    let source = t.tag(&target, "commit", "v1", CANON, "no signature\n");
    let mut m = t.save(
        vec![t.entry(&source, &t.candidate(&source, None, None))],
        None,
    );
    m["entries"][0]["remove_signature"] = true.into();
    t.write(&m);
    refused(t.call(None, false));
}
#[test]
fn tag_credit_changes_are_explicit_and_do_not_authorize_prose_edits() {
    let t = Tags::new();
    let target = t.f.commit(&t.repo, "base", &[]);
    let old = "Co-authored-by: Claude Opus 5.5 <noreply@anthropic.com>";
    let new = "AI-credit: Claude Opus 5.5";
    let source = t.tag(
        &target,
        "commit",
        "v1",
        CANON,
        &format!("prose remains\n\n{old}\n"),
    );
    let mut m = t.save(
        vec![t.entry(
            &source,
            &t.candidate(&source, None, Some(&format!("prose remains\n\n{new}\n"))),
        )],
        None,
    );
    refused(t.call(None, false));
    m["entries"][0]["credit_changes"] = json!([{"old":old,"new":new}]);
    t.write(&m);
    let digest = t.digest();
    accepted(t.call(Some(&digest), false));
    let candidate = t.candidate(&source, None, Some(&format!("prose changed\n\n{new}\n")));
    m["entries"][0] = t.entry(&source, &candidate);
    m["entries"][0]["credit_changes"] = json!([{"old":old,"new":new}]);
    t.write(&m);
    refused(t.call(None, false));
}
#[test]
fn nested_tag_target_maps_within_batch_and_strict_queries_server_once() {
    let t = Tags::new();
    let target = t.f.commit(&t.repo, "base", &[]);
    let tagger = "Old Human <old@example.com> 100 +0930";
    let inner = t.tag(&target, "commit", "inner", tagger, "inner prose\n");
    let e1 = t.entry(&inner, &t.candidate(&inner, None, None));
    let new_inner = e1["expected_oid"].as_str().unwrap().to_string();
    let outer = t.tag(&inner, "tag", "outer", CANON, "outer prose\n");
    let e2 = t.entry(&outer, &t.candidate(&outer, Some(&new_inner), None));
    let new_outer = e2["expected_oid"].as_str().unwrap().to_string();
    let mut m = t.save(vec![e1, e2], None);
    m["entries"][0]["tagger_ownership"] = json!({"old_tagger":tagger,"owned":true});
    t.write(&m);
    accepted(t.call(None, true));
    assert_eq!(t.count(), 1);
    let digest = t.digest();
    accepted(t.call(Some(&digest), false));
    assert!(t.exists(&new_inner));
    assert!(t.exists(&new_outer));
    assert_eq!(t.count(), 1);
}
#[test]
fn receipt_retarget_uses_completed_native_commit_mapping_and_reproves_objects() {
    let t = Tags::new();
    let source_commit = t.f.commit(&t.repo, "old root", &[]);
    let raw = t.raw(&source_commit, "commit");
    let (head, _) = std::str::from_utf8(&raw)
        .unwrap()
        .split_once("\n\n")
        .unwrap();
    let candidate = format!("{head}\n\nfix: mapped root\n").into_bytes();
    let expected = t.oid(&candidate, "commit", false);
    let path = t.f.root.join("commit candidate");
    fs::write(&path, &candidate).unwrap();
    let manifest = t.f.root.join("commit manifest");
    fs::write(&manifest,serde_json::to_vec(&json!({"schema_version":1,"policy_version":1,"common_dir":t.common(),"boundaries":[],"entries":[{"source_oid":source_commit,"source_sha256":sha(&raw),"expected_oid":expected,"candidate_file":path,"candidate_sha256":sha(&candidate)}]})).unwrap()).unwrap();
    let preview = t.f.canonical(
        &["bulk-write", "--manifest", manifest.to_str().unwrap()],
        &t.repo,
        &[],
        None,
    );
    assert!(
        preview.status.success(),
        "{}",
        String::from_utf8_lossy(&preview.stderr)
    );
    let v: Value = serde_json::from_slice(&preview.stdout).unwrap();
    let digest = v["digest"].as_str().unwrap();
    accepted(t.f.canonical(
        &[
            "bulk-write",
            "--manifest",
            manifest.to_str().unwrap(),
            "--confirm",
            digest,
        ],
        &t.repo,
        &[],
        None,
    ));
    let receipt = t
        .common()
        .join("commitguard-bulk")
        .join(digest)
        .join("complete.json");
    let source = t.tag(&source_commit, "commit", "v1", CANON, "original prose\n");
    let m = t.save(
        vec![t.entry(&source, &t.candidate(&source, Some(&expected), None))],
        Some(&receipt),
    );
    let digest = t.digest();
    accepted(t.call(Some(&digest), false));
    assert_eq!(t.count(), 0);
    // The expired caller candidate is irrelevant: only retained private evidence
    // and actual object bytes are trusted for commit mapping reuse.
    fs::remove_file(path).unwrap();
    t.write(&m);
    accepted(t.call(None, false));
    // Even policy-equivalent JSON whitespace must not substitute new retained
    // inputs under the original approved digest.
    let retained = receipt.parent().unwrap().join("manifest.json");
    let original = fs::read(&retained).unwrap();
    let mut altered = original.clone();
    altered.push(b' ');
    fs::write(&retained, altered).unwrap();
    refused(t.call(None, false));
    fs::write(&retained, original).unwrap();
    accepted(t.call(None, false));
    let approval_path = receipt.parent().unwrap().join("approval.json");
    let approval_bytes = fs::read(&approval_path).unwrap();
    let mut approval: Value = serde_json::from_slice(&approval_bytes).unwrap();
    approval["context"] = json!("0".repeat(64));
    fs::write(&approval_path, serde_json::to_vec(&approval).unwrap()).unwrap();
    refused(t.call(None, false));
    fs::write(&approval_path, &approval_bytes).unwrap();
    approval["identity"]["login"] = json!("other");
    fs::write(&approval_path, serde_json::to_vec(&approval).unwrap()).unwrap();
    refused(t.call(None, false));
    fs::write(&approval_path, approval_bytes).unwrap();
    let saved_candidate = receipt.parent().unwrap().join("candidate-000000.commit");
    let candidate_bytes = fs::read(&saved_candidate).unwrap();
    fs::write(&saved_candidate, b"tampered snapshot").unwrap();
    refused(t.call(None, false));
    fs::write(&saved_candidate, candidate_bytes).unwrap();
    let receipt_bytes = fs::read(&receipt).unwrap();
    let mut incomplete: Value = serde_json::from_slice(&receipt_bytes).unwrap();
    incomplete["phase"] = json!("writing");
    fs::write(&receipt, serde_json::to_vec(&incomplete).unwrap()).unwrap();
    refused(t.call(None, false));
    fs::write(&receipt, receipt_bytes).unwrap();
    accepted(t.call(None, false));
    let copy = t.f.root.join("copied receipt.json");
    fs::copy(&receipt, &copy).unwrap();
    let mut m = m;
    m["commit_receipt"] = json!(copy);
    t.write(&m);
    refused(t.call(None, false));
}

#[test]
fn tag_preview_refuses_local_credential_change_without_an_api_request() {
    use std::os::unix::fs::PermissionsExt;
    let mut t = Tags::new();
    let target = t.f.commit(&t.repo, "base", &[]);
    let tagger = "Old Human <old@example.com> 100 +0930";
    let source = t.tag(&target, "commit", "v1", tagger, "original prose\n");
    let mut manifest = t.save(
        vec![t.entry(&source, &t.candidate(&source, None, None))],
        None,
    );
    manifest["entries"][0]["tagger_ownership"] = json!({"old_tagger":tagger,"owned":true});
    t.write(&manifest);
    let counter = t.f.root.join("tag-token-count");
    fs::write(&counter, b"").unwrap();
    fs::set_permissions(&counter, fs::Permissions::from_mode(0o600)).unwrap();
    t.f.env
        .insert("TAG_TOKEN_COUNT".into(), counter.display().to_string());
    executable(
        &t.f.bin.join("gh"),
        r#"#!/bin/sh
case "$*" in
 'auth token --hostname github.com')
   printf 'x\n' >> "$TAG_TOKEN_COUNT"
   count=0; while IFS= read -r line; do count=$((count+1)); done < "$TAG_TOKEN_COUNT"
   if [ "$count" -ge 3 ]; then echo changed-tag-token; else echo fixture-token-tester; fi;;
 'config get user --host github.com') echo tester;;
 'api --hostname github.com user') printf 'api\n' >> "$TAG_API_COUNT"; echo '{"login":"tester","id":44,"type":"User"}';;
 *) exit 78;;
esac
"#,
    );
    refused(t.call(None, false));
    assert!(fs::read_to_string(counter).unwrap().lines().count() >= 3);
    assert_eq!(t.count(), 0);
}

#[test]
fn unsupported_x509_signatures_and_nested_noncommit_targets_are_refused() {
    let t = Tags::new();
    let commit = t.f.commit(&t.repo, "base", &[]);
    let body = "release\n-----BEGIN SIGNED MESSAGE-----\ninvalid\n-----END SIGNED MESSAGE-----\n";
    let source = t.tag(&commit, "commit", "v1", CANON, body);
    let mut m = t.save(
        vec![t.entry(&source, &t.candidate(&source, None, None))],
        None,
    );
    refused(t.call(None, false));
    m["entries"][0]["remove_signature"] = true.into();
    t.write(&m);
    refused(t.call(None, false));
    let blob = t.oid(b"payload", "blob", true);
    let inner = t.tag(&blob, "blob", "inner", CANON, "prose\n");
    let outer = t.tag(&inner, "tag", "outer", CANON, "prose\n");
    t.save(
        vec![t.entry(&outer, &t.candidate(&outer, None, None))],
        None,
    );
    refused(t.call(None, false));
}

#[test]
fn exact_foreign_human_tag_credits_are_preserved_without_claiming_their_work() {
    let t = Tags::new();
    let commit = t.f.commit(&t.repo, "base", &[]);
    let body = "release prose\n\nCo-authored-by: Other Human <other@example.com>\n";
    let source = t.tag(&commit, "commit", "v1", CANON, body);
    let candidate = t.candidate(&source, None, None);
    t.save(vec![t.entry(&source, &candidate)], None);
    let digest = t.digest();
    accepted(t.call(Some(&digest), false));
    let changed = "release prose\n\nCo-authored-by: tester <44+tester@users.noreply.github.com>\n";
    t.save(
        vec![t.entry(&source, &t.candidate(&source, None, Some(changed)))],
        None,
    );
    refused(t.call(None, false));
}

#[test]
fn tag_retarget_reproves_v2_origin_profile_and_rejects_cross_domain_receipts() {
    let t = Tags::new();
    let source_commit = t.f.commit(&t.repo, "old approved source", &[]);
    let raw = t.raw(&source_commit, "commit");
    let text = String::from_utf8(raw.clone()).unwrap();
    let (headers, _) = text.split_once("\n\n").unwrap();
    let old_author = headers
        .lines()
        .find_map(|line| line.strip_prefix("author "))
        .unwrap();
    let candidate = format!(
        "{headers}\nsource-sha256 {}\n\nfix: preserve origin in tag target\n",
        sha(&raw)
    )
    .into_bytes();
    let expected = t.oid(&candidate, "commit", false);
    let candidate_path = t.f.root.join("origin candidate.commit");
    fs::write(&candidate_path, &candidate).unwrap();
    let manifest_path = t.f.root.join("origin manifest.json");
    let manifest = json!({"schema_version":1,"policy_version":2,"provenance_profile":"source-sha256-v1","common_dir":t.common(),"boundaries":[],"entries":[{"source_oid":source_commit,"source_sha256":sha(&raw),"expected_oid":expected,"candidate_file":candidate_path,"candidate_sha256":sha(&candidate),"source_provenance":"add","ownership":{"old_author":old_author,"owned":true}}]});
    fs::write(&manifest_path, serde_json::to_vec(&manifest).unwrap()).unwrap();
    let preview = t.f.canonical(
        &["bulk-write", "--manifest", manifest_path.to_str().unwrap()],
        &t.repo,
        &[],
        None,
    );
    assert!(
        preview.status.success(),
        "{}",
        String::from_utf8_lossy(&preview.stderr)
    );
    let report: Value = serde_json::from_slice(&preview.stdout).unwrap();
    let commit_digest = report["digest"].as_str().unwrap();
    accepted(t.f.canonical(
        &[
            "bulk-write",
            "--manifest",
            manifest_path.to_str().unwrap(),
            "--confirm",
            commit_digest,
        ],
        &t.repo,
        &[],
        None,
    ));
    let receipt = t
        .common()
        .join("commitguard-bulk")
        .join(commit_digest)
        .join("complete.json");
    let source_tag = t.tag(
        &source_commit,
        "commit",
        "origin-v1",
        CANON,
        "unchanged tag prose\n",
    );
    t.save(
        vec![t.entry(
            &source_tag,
            &t.candidate(&source_tag, Some(&expected), None),
        )],
        Some(&receipt),
    );
    let digest = t.digest();
    accepted(t.call(Some(&digest), false));
    assert_eq!(t.count(), 0);
    let retained = receipt.parent().unwrap().join("manifest.json");
    let saved = fs::read(&retained).unwrap();
    for policy in [1, 3] {
        let mut changed = manifest.clone();
        changed["policy_version"] = json!(policy);
        fs::write(&retained, serde_json::to_vec(&changed).unwrap()).unwrap();
        refused(t.call(None, false));
    }
    let mut changed = manifest;
    changed["provenance_profile"] = json!("source-sha256-v2");
    fs::write(&retained, serde_json::to_vec(&changed).unwrap()).unwrap();
    refused(t.call(None, false));
    fs::write(&retained, saved).unwrap();
    accepted(t.call(None, false));
}
