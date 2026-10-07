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
struct Bulk {
    f: Fixture,
    repo: PathBuf,
    manifest: PathBuf,
    api: PathBuf,
    tokens: PathBuf,
}
impl Bulk {
    fn new() -> Self {
        let mut f = Fixture::new();
        let api = f.root.join("bulk-api-count");
        let tokens = f.root.join("bulk-token-count");
        f.env
            .insert("BULK_API_COUNT".into(), api.display().to_string());
        f.env
            .insert("BULK_TOKEN_COUNT".into(), tokens.display().to_string());
        executable(
            &f.bin.join("gh"),
            r#"#!/bin/sh
case "$*" in
 'auth token --hostname github.com')
   printf 'x\n' >> "$BULK_TOKEN_COUNT"
   if [ "${BULK_SWITCH:-0}" = 1 ]; then
     count=0; while IFS= read -r line; do count=$((count+1)); done < "$BULK_TOKEN_COUNT"
     if [ "$count" -ge 3 ]; then echo fixture-changed-token; exit 0; fi
   fi
   echo fixture-token-tester;;
 'config get user --host github.com') echo tester;;
 'api --hostname github.com user') printf 'api\n' >> "$BULK_API_COUNT"; echo '{"login":"tester","id":44,"type":"User"}';;
 *) exit 78;;
esac
"#,
        );
        let repo = f.repo("bulk original");
        let manifest = f.root.join("bulk manifest.json");
        Self {
            f,
            repo,
            manifest,
            api,
            tokens,
        }
    }
    fn raw(&self, oid: &str) -> Vec<u8> {
        let r = self.f.command(
            &self.f.git,
            &["cat-file", "commit", oid],
            &self.repo,
            &[],
            None,
        );
        assert!(r.status.success());
        r.stdout
    }
    fn oid(&self, bytes: &[u8], write: bool) -> String {
        let args = if write {
            vec!["hash-object", "-w", "-t", "commit", "--stdin"]
        } else {
            vec!["hash-object", "-t", "commit", "--stdin"]
        };
        let r = self
            .f
            .command(&self.f.git, &args, &self.repo, &[], Some(bytes));
        assert!(r.status.success(), "{}", String::from_utf8_lossy(&r.stderr));
        String::from_utf8(r.stdout).unwrap().trim().into()
    }
    fn exists(&self, oid: &str) -> bool {
        self.f
            .command(&self.f.git, &["cat-file", "-e", oid], &self.repo, &[], None)
            .status
            .success()
    }
    fn entry(&self, source: &str, candidate: &[u8]) -> Value {
        let expected = self.oid(candidate, false);
        let path = self.f.root.join(format!("candidate-{expected}"));
        fs::write(&path, candidate).unwrap();
        json!({"source_oid":source,"source_sha256":sha(&self.raw(source)),"expected_oid":expected,"candidate_file":path,"candidate_sha256":sha(candidate)})
    }
    fn message(&self, source: &str, message: &str, mapping: &[(&str, &str)]) -> Vec<u8> {
        let raw = self.raw(source);
        let raw = String::from_utf8(raw).unwrap();
        let (headers, _) = raw.split_once("\n\n").unwrap();
        let mut headers = headers.to_string();
        for (old, new) in mapping {
            headers = headers.replace(&format!("parent {old}\n"), &format!("parent {new}\n"));
        }
        format!("{headers}\n\n{message}\n").into_bytes()
    }
    fn save(&self, entries: Vec<Value>, boundaries: &[String]) -> Value {
        let common = self
            .f
            .raw(&["rev-parse", "--git-common-dir"], &self.repo, &[]);
        let common = if Path::new(&common).is_absolute() {
            PathBuf::from(common)
        } else {
            self.repo.join(common)
        }
        .canonicalize()
        .unwrap();
        let m = json!({"schema_version":1,"policy_version":1,"common_dir":common,"boundaries":boundaries,"entries":entries});
        self.write(&m);
        m
    }
    fn write(&self, m: &Value) {
        fs::write(&self.manifest, serde_json::to_vec(m).unwrap()).unwrap();
    }
    fn call(&self, confirm: Option<&str>, strict: bool, extra: &[(&str, &str)]) -> Output {
        let mut args = Vec::new();
        if strict {
            args.push("--strict");
        }
        args.extend(["bulk-write", "--manifest", self.manifest.to_str().unwrap()]);
        if let Some(d) = confirm {
            args.extend(["--confirm", d]);
        }
        self.f.canonical(&args, &self.repo, extra, None)
    }
    fn preview(&self) -> Value {
        let r = self.call(None, false, &[]);
        assert!(r.status.success(), "{}", String::from_utf8_lossy(&r.stderr));
        serde_json::from_slice(&r.stdout).unwrap()
    }
    fn digest(&self) -> String {
        self.preview()["digest"]
            .as_str()
            .expect("preview digest")
            .into()
    }
    fn count(&self) -> usize {
        fs::read_to_string(&self.api)
            .unwrap_or_default()
            .lines()
            .count()
    }
    fn snapshot(&self) -> (String, Vec<u8>, Vec<u8>) {
        (
            self.f.raw(
                &["for-each-ref", "--format=%(refname) %(objectname)"],
                &self.repo,
                &[],
            ),
            fs::read(self.repo.join(".git/index")).unwrap_or_default(),
            self.f
                .command(
                    &self.f.git,
                    &["status", "--porcelain=v2", "-z"],
                    &self.repo,
                    &[],
                    None,
                )
                .stdout,
        )
    }
}
#[test]
fn root_and_ordered_merge_batch_preserves_dates_trees_refs_and_skips_api_hooks() {
    let b = Bulk::new();
    let root = b.f.commit(
        &b.repo,
        "old root",
        &[
            ("GIT_AUTHOR_DATE", "@100 +0930"),
            ("GIT_COMMITTER_DATE", "@200 -0500"),
        ],
    );
    b.f.raw(&["checkout", "-q", "-b", "side"], &b.repo, &[]);
    let side = b.f.commit(&b.repo, "old side", &[]);
    b.f.raw(&["checkout", "-q", "main"], &b.repo, &[]);
    let left = b.f.commit(&b.repo, "old left", &[]);
    b.f.raw(
        &[
            "-c",
            "core.hooksPath=/dev/null",
            "-c",
            "commit.gpgsign=false",
            "merge",
            "--no-ff",
            "-m",
            "old merge",
            "side",
        ],
        &b.repo,
        &[],
    );
    let merge = b.f.raw(&["rev-parse", "HEAD"], &b.repo, &[]);
    let e0 = b.entry(&root, &b.message(&root, "fix: root", &[]));
    let n0 = e0["expected_oid"].as_str().unwrap().to_string();
    let e1 = b.entry(&side, &b.message(&side, "fix: side", &[(&root, &n0)]));
    let n1 = e1["expected_oid"].as_str().unwrap().to_string();
    let e2 = b.entry(&left, &b.message(&left, "fix: left", &[(&root, &n0)]));
    let n2 = e2["expected_oid"].as_str().unwrap().to_string();
    let e3 = b.entry(
        &merge,
        &b.message(&merge, "fix: merge", &[(&left, &n2), (&side, &n1)]),
    );
    let n3 = e3["expected_oid"].as_str().unwrap().to_string();
    b.save(vec![e0, e1, e2, e3], &[]);
    let hooks = b.f.root.join("bulk hooks");
    fs::create_dir(&hooks).unwrap();
    let marker = b.f.root.join("hook-ran");
    executable(
        &hooks.join("pre-commit"),
        &format!("#!/bin/sh\nprintf ran > '{}'\nexit 1\n", marker.display()),
    );
    b.f.raw(
        &["config", "core.hooksPath", hooks.to_str().unwrap()],
        &b.repo,
        &[],
    );
    let before = b.snapshot();
    let digest = b.digest();
    assert!(!b.exists(&n3), "preview must not write candidate objects");
    let r = b.call(Some(&digest), false, &[]);
    assert!(r.status.success(), "{}", String::from_utf8_lossy(&r.stderr));
    let value: Value = serde_json::from_slice(&r.stdout).unwrap();
    assert_eq!(value["phase"], "complete");
    assert!(
        value["mapping"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["source_oid"] == merge && item["new_oid"] == n3)
    );
    assert!(b.exists(&n3));
    assert_eq!(b.snapshot(), before);
    assert!(!marker.exists());
    assert_eq!(b.count(), 0);
    assert_eq!(b.raw(&n0), b.message(&root, "fix: root", &[]));
    accepted(b.call(Some(&digest), false, &[]));
    assert_eq!(b.snapshot(), before);
}
#[test]
fn explicit_strict_is_one_api_call_for_the_entire_preview() {
    let b = Bulk::new();
    let old = b.f.commit(&b.repo, "old root", &[]);
    b.save(
        vec![b.entry(&old, &b.message(&old, "fix: batch", &[]))],
        &[],
    );
    accepted(b.call(None, true, &[]));
    assert_eq!(b.count(), 1);
}
#[test]
fn source_candidate_hash_and_confirmation_mismatches_never_write() {
    for key in ["source_sha256", "candidate_sha256", "expected_oid"] {
        let b = Bulk::new();
        let old = b.f.commit(&b.repo, "old root", &[]);
        let e = b.entry(&old, &b.message(&old, "fix: batch", &[]));
        let expected = e["expected_oid"].as_str().unwrap().to_string();
        let mut m = b.save(vec![e], &[]);
        m["entries"][0][key] = if key == "expected_oid" {
            "1".repeat(expected.len())
        } else {
            "0".repeat(64)
        }
        .into();
        b.write(&m);
        refused(b.call(None, false, &[]));
        assert!(!b.exists(&expected));
        assert_eq!(b.count(), 0);
    }
    let b = Bulk::new();
    let old = b.f.commit(&b.repo, "old root", &[]);
    let e = b.entry(&old, &b.message(&old, "fix: batch", &[]));
    let expected = e["expected_oid"].as_str().unwrap().to_string();
    b.save(vec![e], &[]);
    refused(b.call(Some(&"0".repeat(64)), false, &[]));
    assert!(!b.exists(&expected));
}
#[test]
fn changed_tree_parent_order_dates_and_foreign_author_are_rejected() {
    for change in ["tree", "date", "author", "parent"] {
        let b = Bulk::new();
        let base = b.f.commit(&b.repo, "old base", &[]);
        let old = b.f.commit(&b.repo, "old child", &[]);
        let mut bytes = String::from_utf8(b.message(&old, "fix: batch", &[])).unwrap();
        match change {
            "tree" => {
                let other = b.f.repo("other");
                fs::write(other.join("content"), "arbitrary new content").unwrap();
                b.f.raw(&["add", "content"], &other, &[]);
                let tree = b.f.raw(&["write-tree"], &other, &[]);
                let oldtree = bytes.lines().next().unwrap().to_string();
                bytes = bytes.replacen(&oldtree, &format!("tree {tree}"), 1);
            }
            "date" => {
                let author = bytes
                    .lines()
                    .find(|l| l.starts_with("author "))
                    .unwrap()
                    .to_string();
                bytes = bytes.replacen(
                    &author,
                    "author tester <44+tester@users.noreply.github.com> 1 +0000",
                    1,
                );
            }
            "author" => {
                bytes = bytes.replace(
                    "author tester <44+tester@users.noreply.github.com>",
                    "author foreign <foreign@example.com>",
                )
            }
            "parent" => bytes = bytes.replace(&format!("parent {base}"), &format!("parent {old}")),
            _ => unreachable!(),
        }
        let e = b.entry(&old, bytes.as_bytes());
        let expected = e["expected_oid"].as_str().unwrap().to_string();
        b.save(vec![e], &[base]);
        refused(b.call(None, false, &[]));
        assert!(!b.exists(&expected));
    }
}
#[test]
fn exact_owned_author_declaration_and_boundary_are_required() {
    let b = Bulk::new();
    let base = b.f.commit(&b.repo, "base", &[]);
    let old = b.f.commit(
        &b.repo,
        "old alias",
        &[
            ("GIT_AUTHOR_NAME", "Old Human"),
            ("GIT_AUTHOR_EMAIL", "old@example.com"),
        ],
    );
    let raw = String::from_utf8(b.raw(&old)).unwrap();
    let old_author = raw.lines().find_map(|l| l.strip_prefix("author ")).unwrap();
    let bytes = String::from_utf8(b.message(&old, "fix: owned migration", &[]))
        .unwrap()
        .replace(
            "author Old Human <old@example.com>",
            "author tester <44+tester@users.noreply.github.com>",
        );
    let e = b.entry(&old, bytes.as_bytes());
    let mut m = b.save(vec![e], &[base]);
    refused(b.call(None, false, &[]));
    m["entries"][0]["ownership"] = json!({"old_author":old_author,"owned":true});
    b.write(&m);
    let digest = b.digest();
    accepted(b.call(Some(&digest), false, &[]));
    m["entries"][0]["ownership"]["old_author"] = "Old Human <other@example.com> 1 +0000".into();
    b.write(&m);
    refused(b.call(None, false, &[]));
}
#[test]
fn ai_credit_cannot_be_removed_and_signature_header_removal_is_explicit() {
    let b = Bulk::new();
    let old =
        b.f.commit(&b.repo, "old wording\n\nCo-authored-by: Codex", &[]);
    let e = b.entry(&old, &b.message(&old, "fix: remove credit", &[]));
    b.save(vec![e], &[]);
    refused(b.call(None, false, &[]));
    let raw = b.raw(&old);
    let s = String::from_utf8(raw).unwrap().replacen(
        "\n\n",
        "\ngpgsig fixture signature\n continuation\n\n",
        1,
    );
    let signed = b.oid(s.as_bytes(), true);
    let mut candidate = b.message(
        &signed,
        "fix: preserve credit\n\nCo-authored-by: Codex",
        &[],
    );
    let text = String::from_utf8(candidate).unwrap();
    candidate = text
        .replace("\ngpgsig fixture signature\n continuation", "")
        .into_bytes();
    let mut m = b.save(vec![b.entry(&signed, &candidate)], &[]);
    refused(b.call(None, false, &[]));
    m["entries"][0]["remove_headers"] = json!(["gpgsig"]);
    b.write(&m);
    let digest = b.digest();
    accepted(b.call(Some(&digest), false, &[]));
}
#[test]
fn duplicates_unknown_fields_and_symlink_candidates_are_rejected() {
    use std::os::unix::fs::symlink;
    let b = Bulk::new();
    let old = b.f.commit(&b.repo, "old", &[]);
    let e = b.entry(&old, &b.message(&old, "fix: batch", &[]));
    let m = b.save(vec![e.clone()], &[]);
    let mut duplicate = m.clone();
    duplicate["entries"] = json!([e.clone(), e.clone()]);
    b.write(&duplicate);
    refused(b.call(None, false, &[]));
    let mut unknown = m.clone();
    unknown["unexpected"] = true.into();
    b.write(&unknown);
    refused(b.call(None, false, &[]));
    let mut collapse = m.clone();
    let second = b.f.commit(&b.repo, "second", &[]);
    let mut e2 = e.clone();
    e2["source_oid"] = second.clone().into();
    e2["source_sha256"] = sha(&b.raw(&second)).into();
    collapse["entries"] = json!([e.clone(), e2]);
    b.write(&collapse);
    refused(b.call(None, false, &[]));
    b.write(&m);
    let path = PathBuf::from(e["candidate_file"].as_str().unwrap());
    let target = path.with_extension("real");
    fs::rename(&path, &target).unwrap();
    symlink(&target, &path).unwrap();
    refused(b.call(None, false, &[]));
}
#[test]
fn frozen_digest_rejects_changed_candidates_and_changed_auth_context() {
    let b = Bulk::new();
    let old = b.f.commit(&b.repo, "old", &[]);
    let e = b.entry(&old, &b.message(&old, "fix: batch", &[]));
    let m = b.save(vec![e.clone()], &[]);
    let digest = b.digest();
    fs::write(
        e["candidate_file"].as_str().unwrap(),
        b.message(&old, "fix: later mutation", &[]),
    )
    .unwrap();
    refused(b.call(Some(&digest), false, &[]));
    assert!(!b.exists(e["expected_oid"].as_str().unwrap()));
    fs::write(
        e["candidate_file"].as_str().unwrap(),
        b.message(&old, "fix: batch", &[]),
    )
    .unwrap();
    b.write(&m);
    fs::remove_file(&b.tokens).unwrap();
    refused(b.call(Some(&digest), false, &[("BULK_SWITCH", "1")]));
    assert_eq!(b.count(), 0);
}

#[test]
fn nested_gitlink_substitution_requires_exact_path_old_and_new_declaration() {
    let b = Bulk::new();
    let old_child = b.f.commit(&b.repo, "child old", &[]);
    let new_child = b.f.commit(&b.repo, "child new", &[]);
    b.f.raw(
        &[
            "update-index",
            "--add",
            "--cacheinfo",
            "160000",
            &old_child,
            "vendor/lib",
        ],
        &b.repo,
        &[],
    );
    fs::write(b.repo.join("kept"), "keep this blob\n").unwrap();
    b.f.raw(&["add", "kept"], &b.repo, &[]);
    let source = b.f.commit(&b.repo, "old gitlink", &[]);
    b.f.raw(
        &[
            "update-index",
            "--cacheinfo",
            "160000",
            &new_child,
            "vendor/lib",
        ],
        &b.repo,
        &[],
    );
    let new_tree = b.f.raw(&["write-tree"], &b.repo, &[]);
    let raw = String::from_utf8(b.message(&source, "fix: mapped gitlink", &[])).unwrap();
    let tree_line = raw.lines().next().unwrap();
    let candidate = raw.replacen(tree_line, &format!("tree {new_tree}"), 1);
    let mut m = b.save(
        vec![b.entry(&source, candidate.as_bytes())],
        &[new_child.clone()],
    );
    refused(b.call(None, false, &[]));
    let path_hex = b"vendor/lib"
        .iter()
        .map(|v| format!("{v:02x}"))
        .collect::<String>();
    m["entries"][0]["gitlinks"] =
        json!([{"path_hex":path_hex,"old_oid":old_child,"new_oid":new_child}]);
    b.write(&m);
    let before = b.snapshot();
    let digest = b.digest();
    accepted(b.call(Some(&digest), false, &[]));
    assert_eq!(b.snapshot(), before);
    m["entries"][0]["gitlinks"][0]["path_hex"] = "6b657074".into();
    b.write(&m);
    refused(b.call(None, false, &[]));
}

#[test]
fn sha256_repository_hashes_and_complete_readback_are_supported() {
    let mut b = Bulk::new();
    let repo = b.f.root.join("sha256 repository");
    fs::create_dir(&repo).unwrap();
    let r = b.f.command(
        &b.f.git,
        &["init", "-q", "-b", "main", "--object-format=sha256"],
        &repo,
        &[],
        None,
    );
    assert!(
        r.status.success(),
        "test Git must support SHA256: {}",
        String::from_utf8_lossy(&r.stderr)
    );
    b.repo = repo;
    let source = b.f.commit(&b.repo, "old sha256 root", &[]);
    assert_eq!(source.len(), 64);
    let candidate = b.message(&source, "fix: sha256 batch", &[]);
    let entry = b.entry(&source, &candidate);
    let expected = entry["expected_oid"].as_str().unwrap().to_string();
    assert_eq!(expected.len(), 64);
    b.save(vec![entry], &[]);
    let digest = b.digest();
    accepted(b.call(Some(&digest), false, &[]));
    assert_eq!(b.raw(&expected), candidate);
    assert_eq!(b.count(), 0);
}

#[test]
fn credit_normalization_cannot_discard_model_version_or_context() {
    let b = Bulk::new();
    let old_credit = "Co-authored-by: Claude Opus 5.5 (1m context) <noreply@anthropic.com>";
    let source = b.f.commit(&b.repo, &format!("old\n\n{old_credit}"), &[]);
    let new_credit = "Co-authored-by: Claude <noreply@anthropic.com>";
    let candidate = b.message(&source, &format!("fix: batch\n\n{new_credit}"), &[]);
    let mut m = b.save(vec![b.entry(&source, &candidate)], &[]);
    m["entries"][0]["credit_changes"] = json!([{"old":old_credit,"new":new_credit}]);
    b.write(&m);
    refused(b.call(None, false, &[]));
}

fn ai_main_case(
    role: &str,
    name: &str,
    email: &str,
    credit: Option<&str>,
) -> (Bulk, Value, String) {
    let b = Bulk::new();
    let vars = if role == "author" {
        [("GIT_AUTHOR_NAME", name), ("GIT_AUTHOR_EMAIL", email)]
    } else {
        [("GIT_COMMITTER_NAME", name), ("GIT_COMMITTER_EMAIL", email)]
    };
    let source = b.f.commit(&b.repo, "old main attribution", &vars);
    let raw = String::from_utf8(b.raw(&source)).unwrap();
    let old_identity = raw
        .lines()
        .find_map(|line| line.strip_prefix(&format!("{role} ")))
        .unwrap()
        .to_string();
    let message = credit
        .map(|v| format!("fix: retain provenance\n\n{v}"))
        .unwrap_or_else(|| "fix: retain provenance".into());
    let candidate = String::from_utf8(b.message(&source, &message, &[]))
        .unwrap()
        .replace(
            &format!("{role} {name} <{email}>"),
            &format!("{role} tester <44+tester@users.noreply.github.com>"),
        );
    let mut entry = b.entry(&source, candidate.as_bytes());
    if role == "author" {
        entry["ownership"] = json!({"old_author":old_identity,"owned":true});
    } else {
        entry["committer_ownership"] = json!({"old_committer":old_identity,"owned":true});
    }
    let manifest = b.save(vec![entry], &[]);
    (b, manifest, old_identity)
}

#[test]
fn recognized_ai_main_role_canonicalization_requires_equivalent_credit() {
    let exact = "Co-authored-by: Claude Opus 5.5 (1m context) <noreply@anthropic.com>";
    for role in ["author", "committer"] {
        let (b, m, _) = ai_main_case(
            role,
            "Claude Opus 5.5 (1m context)",
            "noreply@anthropic.com",
            None,
        );
        refused(b.call(None, false, &[]));
        assert!(!b.exists(m["entries"][0]["expected_oid"].as_str().unwrap()));
        let (b, _, _) = ai_main_case(
            role,
            "Claude Opus 5.5 (1m context)",
            "noreply@anthropic.com",
            Some(exact),
        );
        let digest = b.digest();
        accepted(b.call(Some(&digest), false, &[]));
        let changed = "Co-authored-by: Claude Sonnet 5.5 (1m context) <noreply@anthropic.com>";
        let (b, _, _) = ai_main_case(
            role,
            "Claude Opus 5.5 (1m context)",
            "noreply@anthropic.com",
            Some(changed),
        );
        refused(b.call(None, false, &[]));
    }
}

#[test]
fn exact_legacy_ai_main_declaration_preserves_credit_and_rejects_ambiguous_roles() {
    let new = "Co-authored-by: Codex <noreply@openai.com>";
    for role in ["author", "committer"] {
        let (b, mut m, old) = ai_main_case(
            role,
            "Legacy Assistant",
            "legacy-ai@example.invalid",
            Some(new),
        );
        // Unknown main names cannot be inferred as AI merely because a new
        // recognized credit was added. An exact approved migration is required.
        refused(b.call(None, false, &[]));
        let declaration = json!({"role":role,"old_identity":old,"new":new});
        m["entries"][0]["main_credit_changes"] = json!([declaration.clone()]);
        b.write(&m);
        let digest = b.digest();
        accepted(b.call(Some(&digest), false, &[]));
        for invalid in [
            json!([declaration.clone(), declaration.clone()]),
            json!([{"role":role,"old_identity":"Legacy Assistant <wrong@example.invalid> 1 +0000","new":new}]),
            json!([{"role":"tree","old_identity":old,"new":new}]),
            json!([{"role":if role=="author" {"committer"} else {"author"},"old_identity":old,"new":new}]),
            json!([{"role":role,"old_identity":old,"new":"Signed-off-by: Codex <noreply@openai.com>"}]),
            json!([{"role":role,"old_identity":old,"new":"Co-authored-by: Claude <noreply@anthropic.com>"}]),
        ] {
            m["entries"][0]["main_credit_changes"] = invalid;
            b.write(&m);
            refused(b.call(None, false, &[]));
        }
    }
}

#[test]
fn recognized_ai_main_declaration_cannot_change_model_or_drop_candidate_credit() {
    let credit = "Co-authored-by: Claude Opus 5.5 <noreply@anthropic.com>";
    for role in ["author", "committer"] {
        let (b, mut m, old) = ai_main_case(
            role,
            "Claude Opus 5.5",
            "noreply@anthropic.com",
            Some(credit),
        );
        m["entries"][0]["main_credit_changes"] = json!([{"role":role,"old_identity":old,"new":"Co-authored-by: Claude Sonnet 5.5 <noreply@anthropic.com>"}]);
        b.write(&m);
        refused(b.call(None, false, &[]));
        let (b, mut m, old) = ai_main_case(role, "Claude Opus 5.5", "noreply@anthropic.com", None);
        m["entries"][0]["main_credit_changes"] =
            json!([{"role":role,"old_identity":old,"new":credit}]);
        b.write(&m);
        refused(b.call(None, false, &[]));
    }
}

fn compact_credit_case(old_blocks: &str, new_blocks: &str) -> (Bulk, Value) {
    let b = Bulk::new();
    let source = b.f.commit(
        &b.repo,
        &format!("old approved wording\n\n{old_blocks}"),
        &[],
    );
    let candidate = b.message(
        &source,
        &format!("fix: compact credit\n\n{new_blocks}"),
        &[],
    );
    let mut manifest = b.save(vec![b.entry(&source, &candidate)], &[]);
    manifest["entries"][0]["credit_changes"] = json!([{"old":old_blocks,"new":new_blocks}]);
    b.write(&manifest);
    (b, manifest)
}

#[test]
fn explicit_standard_ai_to_compact_credit_preserves_exact_semantics() {
    for model in ["Opus 4.7", "Opus 5", "Opus 5.5", "Sonnet 5"] {
        let old = format!("Co-Authored-By: Claude {model} <noreply@anthropic.com>");
        let new = format!("AI-credit: Claude {model}");
        let (b, manifest) = compact_credit_case(&old, &new);
        let digest = b.digest();
        accepted(b.call(Some(&digest), false, &[]));
        let expected = manifest["entries"][0]["expected_oid"].as_str().unwrap();
        assert_eq!(
            b.raw(expected),
            fs::read(manifest["entries"][0]["candidate_file"].as_str().unwrap()).unwrap()
        );
        assert_eq!(b.count(), 0);
    }
    let old = "Co-authored-by: Claude Opus 4.8 (1m context) <noreply@anthropic.com>";
    let (b, _) = compact_credit_case(old, "AI-credit: Claude Opus 4.8-1M");
    let digest = b.digest();
    accepted(b.call(Some(&digest), false, &[]));
}

#[test]
fn compact_shared_prefix_rewrites_multiple_long_standard_credits_without_loss() {
    let old = [
        "Co-authored-by: Claude Opus 4.8 (1m context) <noreply@anthropic.com>",
        "Co-authored-by: Claude Opus 5 <noreply@anthropic.com>",
        "Co-authored-by: Claude Opus 4.7 <noreply@anthropic.com>",
        "Co-authored-by: Claude Fable 5 <noreply@anthropic.com>",
    ]
    .join("\n");
    assert!(
        old.len() > 128,
        "historical source may exceed current message policy"
    );
    let new = "AI-credit: Claude Opus 4.8-1M/5/4.7; Fable 5";
    assert!("fix: compact credit\n\n".len() + new.len() <= 128);
    let (b, m) = compact_credit_case(&old, new);
    let digest = b.digest();
    accepted(b.call(Some(&digest), false, &[]));
    assert!(b.exists(m["entries"][0]["expected_oid"].as_str().unwrap()));
    let (b, _) = compact_credit_case(&old, "AI-credit: Claude Opus 4.8/5/4.7; Fable 5");
    refused(b.call(None, false, &[]));
    let (b, _) = compact_credit_case(&old, "AI-credit: Claude Opus 4.8-1M/5/4.7; Fable 5-1M");
    refused(b.call(None, false, &[]));
}

#[test]
fn compact_credit_cannot_change_provider_model_version_context_or_multiplicity() {
    let old = "Co-authored-by: Claude Opus 4.7 <noreply@anthropic.com>";
    for new in [
        "AI-credit: GPT 4.7",
        "AI-credit: Claude Sonnet 4.7",
        "AI-credit: Claude Opus 5",
        "AI-credit: Claude Opus",
        "AI-credit: Claude Opus 4.7-1M",
        "AI-credit: Claude Opus bananas",
        "AI-credit: Opus 4.7",
        "AI-credit: Claude Opus 4.7/4.7",
        "AI-credit: Claude Opus 4.7\nAI-credit: Claude Opus 4.7",
    ] {
        let (b, _) = compact_credit_case(old, new);
        refused(b.call(None, false, &[]));
    }
    let doubled = format!("{old}\n{old}");
    let (b, _) = compact_credit_case(&doubled, "AI-credit: Claude Opus 4.7");
    refused(b.call(None, false, &[]));
    let context = "Co-authored-by: Claude Opus 4.7 (1m context) <noreply@anthropic.com>";
    let (b, _) = compact_credit_case(context, "AI-credit: Claude Opus 4.7");
    refused(b.call(None, false, &[]));
}

#[test]
fn compact_credit_cannot_replace_human_signoff_or_bypass_explicit_declarations() {
    let human = "Signed-off-by: tester <44+tester@users.noreply.github.com>";
    let (b, _) = compact_credit_case(human, "AI-credit: Claude Opus 5.5");
    refused(b.call(None, false, &[]));
    let old = "Co-authored-by: Claude Opus 5.5 <noreply@anthropic.com>";
    let (b, mut m) = compact_credit_case(old, "AI-credit: Claude Opus 5.5");
    m["entries"][0]["credit_changes"] = json!([]);
    b.write(&m);
    refused(b.call(None, false, &[]));
    for role in ["author", "committer"] {
        let new = "AI-credit: Codex";
        let (b, mut m, old_identity) = ai_main_case(
            role,
            "Legacy Assistant",
            "legacy-ai@example.invalid",
            Some(new),
        );
        m["entries"][0]["main_credit_changes"] =
            json!([{"role":role,"old_identity":old_identity,"new":new}]);
        b.write(&m);
        let digest = b.digest();
        accepted(b.call(Some(&digest), false, &[]));
        let new = "AI-credit: Codex; Claude";
        let (b, mut m, old_identity) = ai_main_case(
            role,
            "Legacy Assistant",
            "legacy-ai@example.invalid",
            Some(new),
        );
        m["entries"][0]["main_credit_changes"] =
            json!([{"role":role,"old_identity":old_identity,"new":new}]);
        b.write(&m);
        refused(b.call(None, false, &[]));
    }
    let new = "AI-credit: Codex";
    let (b, mut m, old_identity) = ai_main_case(
        "author",
        "tester",
        "44+tester@users.noreply.github.com",
        Some(new),
    );
    m["entries"][0]["main_credit_changes"] =
        json!([{"role":"author","old_identity":old_identity,"new":new}]);
    b.write(&m);
    refused(b.call(None, false, &[]));
}

#[test]
fn foreign_committer_requires_its_own_exact_positive_ownership_claim() {
    let (b, mut m, old) = ai_main_case("committer", "Old Human", "old@example.com", None);
    let claim = m["entries"][0]["committer_ownership"].take();
    b.write(&m);
    refused(b.call(None, false, &[]));
    for invalid in [
        json!({"old_committer":old,"owned":false}),
        json!({"old_committer":"Other Human <other@example.com> 1 +0000","owned":true}),
        json!({"old_author":old,"owned":true}),
    ] {
        m["entries"][0]["committer_ownership"] = invalid;
        b.write(&m);
        refused(b.call(None, false, &[]));
    }
    m["entries"][0]["committer_ownership"] = claim;
    b.write(&m);
    let digest = b.digest();
    accepted(b.call(Some(&digest), false, &[]));
    // The author is already authenticated; its identity cannot stand in for a
    // separate declaration about an unrelated committer.
    assert_eq!(m["entries"][0]["ownership"], Value::Null);
}

#[test]
fn legacy_human_trailer_normalization_requires_explicit_ownership_and_same_role() {
    for key in ["Co-authored-by", "Signed-off-by"] {
        let old = format!("{key}: Old Human <old@example.com>");
        let new = format!("{key}: tester <44+tester@users.noreply.github.com>");
        let (b, mut m) = compact_credit_case(&old, &new);
        refused(b.call(None, false, &[]));
        m["entries"][0]["credit_changes"][0]["owned"] = false.into();
        b.write(&m);
        refused(b.call(None, false, &[]));
        m["entries"][0]["credit_changes"][0]["owned"] = true.into();
        b.write(&m);
        let digest = b.digest();
        accepted(b.call(Some(&digest), false, &[]));
        let ai = "Co-authored-by: Claude <noreply@anthropic.com>";
        let (b, mut m) = compact_credit_case(&old, ai);
        m["entries"][0]["credit_changes"][0]["owned"] = true.into();
        b.write(&m);
        refused(b.call(None, false, &[]));
        let opposite = if key == "Co-authored-by" {
            "Signed-off-by"
        } else {
            "Co-authored-by"
        };
        let alternate = format!("{opposite}: tester <44+tester@users.noreply.github.com>");
        let (b, mut m) = compact_credit_case(&old, &alternate);
        m["entries"][0]["credit_changes"][0]["owned"] = true.into();
        b.write(&m);
        refused(b.call(None, false, &[]));
    }
}

#[test]
fn ai_main_migration_cannot_invent_signedoff_certification_but_keeps_existing_one() {
    let credit = "Signed-off-by: Claude Opus 5.5 <noreply@anthropic.com>";
    for role in ["author", "committer"] {
        let (b, _, _) = ai_main_case(
            role,
            "Claude Opus 5.5",
            "noreply@anthropic.com",
            Some(credit),
        );
        refused(b.call(None, false, &[]));
        let b = Bulk::new();
        let variables = if role == "author" {
            [
                ("GIT_AUTHOR_NAME", "Claude Opus 5.5"),
                ("GIT_AUTHOR_EMAIL", "noreply@anthropic.com"),
            ]
        } else {
            [
                ("GIT_COMMITTER_NAME", "Claude Opus 5.5"),
                ("GIT_COMMITTER_EMAIL", "noreply@anthropic.com"),
            ]
        };
        let source = b.f.commit(
            &b.repo,
            &format!("old certification\n\n{credit}"),
            &variables,
        );
        let raw = String::from_utf8(b.raw(&source)).unwrap();
        let old_identity = raw
            .lines()
            .find_map(|line| line.strip_prefix(&format!("{role} ")))
            .unwrap();
        let candidate = String::from_utf8(b.message(
            &source,
            &format!("fix: retain certification\n\n{credit}"),
            &[],
        ))
        .unwrap()
        .replace(
            &format!("{role} Claude Opus 5.5 <noreply@anthropic.com>"),
            &format!("{role} tester <44+tester@users.noreply.github.com>"),
        );
        let mut entry = b.entry(&source, candidate.as_bytes());
        if role == "author" {
            entry["ownership"] = json!({"old_author":old_identity,"owned":true});
        } else {
            entry["committer_ownership"] = json!({"old_committer":old_identity,"owned":true});
        }
        b.save(vec![entry], &[]);
        let digest = b.digest();
        accepted(b.call(Some(&digest), false, &[]));
    }
}

#[test]
fn preview_cannot_bind_a_cached_identity_to_a_later_different_credential() {
    use std::os::unix::fs::PermissionsExt;
    let b = Bulk::new();
    let source = b.f.commit(&b.repo, "old credential race", &[]);
    b.save(
        vec![b.entry(&source, &b.message(&source, "fix: race boundary", &[]))],
        &[],
    );
    // Fixture::new seeded the same selected account and the first credential.
    // Reads one and two validate that cached identity; a later independent
    // fingerprint must never bind it to a different token for the same login.
    fs::write(&b.tokens, b"").unwrap();
    fs::set_permissions(&b.tokens, fs::Permissions::from_mode(0o600)).unwrap();
    let result = b.call(None, false, &[("BULK_SWITCH", "1")]);
    let reads = fs::read_to_string(&b.tokens).unwrap().lines().count();
    assert!(
        reads >= 3,
        "fixture must reach the local credential transition"
    );
    assert_eq!(
        b.count(),
        0,
        "credential race must not make any API request"
    );
    refused(result);
}

#[test]
fn exact_generated_tool_footer_maps_without_guessing_prose_or_urls() {
    let old = "🤖 Generated with [Claude Code](https://claude.com/claude-code)";
    let (b, _) = compact_credit_case(old, "AI-credit: Claude Code");
    let digest = b.digest();
    accepted(b.call(Some(&digest), false, &[]));
    for invalid in [
        "Generated with Claude Code",
        "🤖 Generated with [Claude Code](https://example.com/claude-code)",
    ] {
        let (b, _) = compact_credit_case(invalid, "AI-credit: Claude Code");
        refused(b.call(None, false, &[]));
    }
    let (b, _) = compact_credit_case(old, "AI-credit: Codex");
    refused(b.call(None, false, &[]));
}

#[test]
fn compact_actor_counts_preserve_repeated_source_attribution() {
    for count in [7, 141] {
        let old = vec!["Co-authored-by: Claude Opus 5.5 <noreply@anthropic.com>"; count].join("\n");
        let new = format!("AI-credit: Claude Opus 5.5 x{count}");
        let (b, _) = compact_credit_case(&old, &new);
        let digest = b.digest();
        accepted(b.call(Some(&digest), false, &[]));
        for wrong in [count - 1, count + 1] {
            let (b, _) = compact_credit_case(&old, &format!("AI-credit: Claude Opus 5.5 x{wrong}"));
            refused(b.call(None, false, &[]));
        }
    }
}

#[test]
fn noncontiguous_source_credit_groups_consume_each_actual_block_once() {
    let credit = "Co-authored-by: Claude Opus 5.5 <noreply@anthropic.com>";
    let old = format!("{credit}\n\nintervening source prose\n\n{credit}\n\nmore prose\n\n{credit}");
    let (b, mut m) = compact_credit_case(&old, "AI-credit: Claude Opus 5.5 x3");
    refused(b.call(None, false, &[]));
    m["entries"][0]["credit_changes"] =
        json!([{"source_blocks":[credit,credit,credit],"new":"AI-credit: Claude Opus 5.5 x3"}]);
    b.write(&m);
    let digest = b.digest();
    accepted(b.call(Some(&digest), false, &[]));
    for blocks in [
        vec![credit, credit],
        vec![credit, credit, credit, credit],
        vec![credit, "Co-authored-by: Codex", credit],
    ] {
        m["entries"][0]["credit_changes"][0]["source_blocks"] = json!(blocks);
        b.write(&m);
        refused(b.call(None, false, &[]));
    }
    m["entries"][0]["credit_changes"] = json!([{"old":credit,"source_blocks":[credit,credit,credit],"new":"AI-credit: Claude Opus 5.5 x3"}]);
    b.write(&m);
    refused(b.call(None, false, &[]));
}

#[test]
fn rewrite_cannot_invent_or_duplicate_human_attribution_or_signoff() {
    for key in ["Co-authored-by", "Signed-off-by"] {
        let human = format!("{key}: tester <44+tester@users.noreply.github.com>");
        let b = Bulk::new();
        let source = b.f.commit(&b.repo, "source without certification", &[]);
        let candidate = b.message(&source, &format!("fix: preserve meaning\n\n{human}"), &[]);
        b.save(vec![b.entry(&source, &candidate)], &[]);
        refused(b.call(None, false, &[]));
        let (b, mut m) = compact_credit_case(&human, &format!("{human}\n{human}"));
        m["entries"][0]["credit_changes"] = json!([]);
        b.write(&m);
        refused(b.call(None, false, &[]));
    }
}
