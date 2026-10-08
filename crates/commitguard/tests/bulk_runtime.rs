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
 '--version') echo 'gh version 2.90.0 (isolated fixture)';;
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
            vec![
                "hash-object",
                "--literally",
                "-w",
                "-t",
                "commit",
                "--stdin",
            ]
        } else {
            vec!["hash-object", "--literally", "-t", "commit", "--stdin"]
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
        std::slice::from_ref(&new_child),
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

fn with_origin(candidate: &[u8], origin: &str) -> Vec<u8> {
    let raw = std::str::from_utf8(candidate).unwrap();
    let (headers, message) = raw.split_once("\n\n").unwrap();
    format!("{headers}\nsource-sha256 {origin}\n\n{message}").into_bytes()
}
fn origin_entry(b: &Bulk, source: &str, candidate: &[u8], mode: &str) -> Value {
    let raw = b.raw(source);
    let raw = std::str::from_utf8(&raw).unwrap();
    let author = raw
        .lines()
        .find_map(|line| line.strip_prefix("author "))
        .unwrap();
    let mut entry = b.entry(source, candidate);
    entry["source_provenance"] = mode.into();
    entry["ownership"] = json!({"old_author":author,"owned":true});
    entry
}
fn origin_manifest(b: &Bulk, entries: Vec<Value>, boundaries: &[String]) -> Value {
    let mut m = b.save(entries, boundaries);
    m["policy_version"] = 2.into();
    m["provenance_profile"] = "source-sha256-v1".into();
    b.write(&m);
    m
}

#[test]
fn origin_profile_distinguishes_colliding_roots_without_changing_words_dates_or_tree() {
    let b = Bulk::new();
    let first = b.f.commit(
        &b.repo,
        "different old message one",
        &[
            ("GIT_AUTHOR_DATE", "@100 +0930"),
            ("GIT_COMMITTER_DATE", "@200 -0500"),
        ],
    );
    let first_raw = String::from_utf8(b.raw(&first)).unwrap();
    let second = b.oid(
        first_raw
            .replace("different old message one", "different old message two")
            .as_bytes(),
        true,
    );
    let common_candidate = b.message(&first, "fix: retain meaning", &[]);
    assert_eq!(
        common_candidate,
        b.message(&second, "fix: retain meaning", &[])
    );
    let one = with_origin(&common_candidate, &sha(&b.raw(&first)));
    let two = with_origin(&common_candidate, &sha(&b.raw(&second)));
    let e1 = origin_entry(&b, &first, &one, "add");
    let n1 = e1["expected_oid"].as_str().unwrap().to_string();
    let e2 = origin_entry(&b, &second, &two, "add");
    let n2 = e2["expected_oid"].as_str().unwrap().to_string();
    assert_ne!(n1, n2);
    origin_manifest(&b, vec![e1, e2], &[]);
    let hooks = b.f.root.join("origin hooks");
    fs::create_dir(&hooks).unwrap();
    let marker = b.f.root.join("origin hook-ran");
    executable(
        &hooks.join("reference-transaction"),
        &format!("#!/bin/sh\nprintf ran > '{}'\nexit 1\n", marker.display()),
    );
    b.f.raw(
        &["config", "core.hooksPath", hooks.to_str().unwrap()],
        &b.repo,
        &[],
    );
    let before = b.snapshot();
    let digest = b.digest();
    assert!(!b.exists(&n1) && !b.exists(&n2));
    let output = b.call(Some(&digest), false, &[]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["mapping"].as_array().unwrap().len(), 2);
    assert_eq!(b.raw(&n1), one);
    assert_eq!(b.raw(&n2), two);
    assert_eq!(b.snapshot(), before);
    assert!(!marker.exists());
    assert_eq!(b.count(), 0);
    accepted(b.call(Some(&digest), false, &[]));
    for new in [&n1, &n2] {
        let raw = String::from_utf8(b.raw(new)).unwrap();
        assert!(raw.ends_with("\n\nfix: retain meaning\n"));
        assert!(raw.contains("author tester <44+tester@users.noreply.github.com> 100 +0930\n"));
        assert!(raw.contains(
            "committer tester <44+tester@users.noreply.github.com> 200 -0500\nsource-sha256 "
        ));
    }
}

#[test]
fn origin_default_policy_and_explicit_nulls_never_opt_in_implicitly() {
    let b = Bulk::new();
    let source = b.f.commit(&b.repo, "old root", &[]);
    let plain = b.message(&source, "fix: ordinary", &[]);
    let ordinary = b.save(vec![b.entry(&source, &plain)], &[]);
    let digest = b.digest();
    accepted(b.call(Some(&digest), false, &[]));
    let origin = with_origin(
        &b.message(&source, "fix: origin", &[]),
        &sha(&b.raw(&source)),
    );
    b.save(vec![b.entry(&source, &origin)], &[]);
    refused(b.call(None, false, &[]));
    for field in ["provenance_profile", "source_provenance"] {
        for explicit_null in [false, true] {
            let mut m = ordinary.clone();
            if field == "provenance_profile" {
                m[field] = if explicit_null {
                    Value::Null
                } else {
                    "source-sha256-v1".into()
                };
            } else {
                m["entries"][0][field] = if explicit_null {
                    Value::Null
                } else {
                    "add".into()
                };
            }
            b.write(&m);
            refused(b.call(None, false, &[]));
        }
    }
    let opt = origin_manifest(&b, vec![origin_entry(&b, &source, &origin, "add")], &[]);
    for (field, in_entry) in [("provenance_profile", false), ("source_provenance", true)] {
        let mut m = opt.clone();
        if in_entry {
            m["entries"][0][field] = Value::Null;
        } else {
            m[field] = Value::Null;
        }
        b.write(&m);
        refused(b.call(None, false, &[]));
    }
    let mut m = opt.clone();
    m.as_object_mut().unwrap().remove("provenance_profile");
    b.write(&m);
    refused(b.call(None, false, &[]));
    let mut m = opt.clone();
    m["provenance_profile"] = "different-profile".into();
    b.write(&m);
    refused(b.call(None, false, &[]));
    let mut m = opt.clone();
    m["entries"][0]
        .as_object_mut()
        .unwrap()
        .remove("source_provenance");
    b.write(&m);
    refused(b.call(None, false, &[]));
    let body = b.message(
        &source,
        &format!("fix: body\n\nsource-sha256 {}", "a".repeat(64)),
        &[],
    );
    origin_manifest(&b, vec![b.entry(&source, &body)], &[]);
    let digest = b.digest();
    accepted(b.call(Some(&digest), false, &[]));
}

#[test]
fn origin_add_requires_exact_positive_source_ownership_even_for_canonical_authors() {
    for foreign in [false, true] {
        let b = Bulk::new();
        let vars = if foreign {
            vec![
                ("GIT_AUTHOR_NAME", "Old Human"),
                ("GIT_AUTHOR_EMAIL", "old@example.com"),
            ]
        } else {
            Vec::new()
        };
        let source = b.f.commit(&b.repo, "old source", &vars);
        let mut plain = String::from_utf8(b.message(&source, "fix: origin", &[])).unwrap();
        if foreign {
            plain = plain.replace(
                "author Old Human <old@example.com>",
                "author tester <44+tester@users.noreply.github.com>",
            );
        }
        let candidate = with_origin(plain.as_bytes(), &sha(&b.raw(&source)));
        let mut m = origin_manifest(&b, vec![origin_entry(&b, &source, &candidate, "add")], &[]);
        let exact = m["entries"][0]["ownership"].clone();
        for claim in [
            Value::Null,
            json!({"old_author":exact["old_author"],"owned":false}),
            json!({"old_author":"wrong <wrong@example.com> 1 +0000","owned":true}),
        ] {
            m["entries"][0]["ownership"] = claim;
            b.write(&m);
            refused(b.call(None, false, &[]));
        }
        m["entries"][0]["ownership"] = exact;
        b.write(&m);
        let digest = b.digest();
        accepted(b.call(Some(&digest), false, &[]));
    }
}

#[test]
fn origin_headers_reject_wrong_hash_uppercase_duplicates_folding_and_wrong_slots() {
    let b = Bulk::new();
    let source = b.f.commit(&b.repo, "old root", &[]);
    let raw = b.raw(&source);
    let origin = sha(&raw);
    let plain = b.message(&source, "fix: origin", &[]);
    let valid = String::from_utf8(with_origin(&plain, &origin)).unwrap();
    let line = format!("source-sha256 {origin}");
    let committer = valid
        .lines()
        .find(|l| l.starts_with("committer "))
        .unwrap()
        .to_string();
    let variants = [
        valid.replace(&line, &format!("source-sha256 {}", "0".repeat(64))),
        valid.replace(&line, &format!("source-sha256 {}", origin.to_uppercase())),
        valid.replace(&line, &format!("{line}\n{line}")),
        valid.replace(&line, &format!("{line}\n continuation")),
        valid.replace(
            &format!("{committer}\n{line}"),
            &format!("{line}\n{committer}"),
        ),
        valid.replace(&line, &format!("source-sha256 {}", &origin[..63])),
        valid.replace(&line, &format!("{line} ")),
        valid.replace(&line, &format!("source-sha256\t{origin}")),
        valid.replace(&line, &format!("SOURCE-SHA256 {origin}")),
    ];
    for candidate in variants {
        let e = origin_entry(&b, &source, candidate.as_bytes(), "add");
        let expected = e["expected_oid"].as_str().unwrap().to_string();
        origin_manifest(&b, vec![e], &[]);
        refused(b.call(None, false, &[]));
        assert!(!b.exists(&expected));
    }
    origin_manifest(&b, vec![origin_entry(&b, &source, &plain, "add")], &[]);
    refused(b.call(None, false, &[]));
}

#[test]
fn origin_preserve_keeps_first_raw_hash_and_never_refreshes_removes_or_invents_it() {
    let b = Bulk::new();
    let source = b.f.commit(&b.repo, "original old root", &[]);
    let origin = sha(&b.raw(&source));
    let candidate = with_origin(&b.message(&source, "fix: first rewrite", &[]), &origin);
    let entry = origin_entry(&b, &source, &candidate, "add");
    let first = entry["expected_oid"].as_str().unwrap().to_string();
    origin_manifest(&b, vec![entry], &[]);
    let digest = b.digest();
    accepted(b.call(Some(&digest), false, &[]));
    assert_ne!(sha(&b.raw(&first)), origin);
    let second = b.message(&first, "fix: later rewrite", &[]);
    let e = origin_entry(&b, &first, &second, "preserve");
    let new = e["expected_oid"].as_str().unwrap().to_string();
    let valid = origin_manifest(&b, vec![e], &[]);
    let digest = b.digest();
    accepted(b.call(Some(&digest), false, &[]));
    assert!(
        String::from_utf8(b.raw(&new))
            .unwrap()
            .contains(&format!("source-sha256 {origin}\n\n"))
    );
    accepted(b.call(Some(&digest), false, &[]));
    let mut add = valid.clone();
    add["entries"][0]["source_provenance"] = "add".into();
    b.write(&add);
    refused(b.call(None, false, &[]));
    let mut undeclared = valid.clone();
    undeclared["entries"][0]
        .as_object_mut()
        .unwrap()
        .remove("source_provenance");
    b.write(&undeclared);
    refused(b.call(None, false, &[]));
    for claim in [
        Value::Null,
        json!({"old_author":valid["entries"][0]["ownership"]["old_author"],"owned":false}),
        json!({"old_author":"Foreign <foreign@example.com> 1 +0000","owned":true}),
    ] {
        let mut invalid = valid.clone();
        invalid["entries"][0]["ownership"] = claim;
        b.write(&invalid);
        refused(b.call(None, false, &[]));
    }
    for updated in ["0".repeat(64), sha(&b.raw(&first))] {
        let changed = String::from_utf8(second.clone()).unwrap().replace(
            &format!("source-sha256 {origin}"),
            &format!("source-sha256 {updated}"),
        );
        origin_manifest(
            &b,
            vec![origin_entry(&b, &first, changed.as_bytes(), "preserve")],
            &[],
        );
        refused(b.call(None, false, &[]));
    }
    let deleted = String::from_utf8(second)
        .unwrap()
        .replace(&format!("\nsource-sha256 {origin}"), "");
    let mut m = origin_manifest(
        &b,
        vec![origin_entry(&b, &first, deleted.as_bytes(), "preserve")],
        &[],
    );
    refused(b.call(None, false, &[]));
    m["entries"][0]["remove_headers"] = json!(["source-sha256"]);
    b.write(&m);
    refused(b.call(None, false, &[]));
    origin_manifest(
        &b,
        vec![origin_entry(
            &b,
            &source,
            &b.message(&source, "fix: missing origin", &[]),
            "preserve",
        )],
        &[],
    );
    refused(b.call(None, false, &[]));
}

#[test]
fn origin_hash_covers_signatures_and_profile_boundaries_keep_ordered_merge_parents() {
    let b = Bulk::new();
    let root = b.f.commit(&b.repo, "original root", &[]);
    let signed = String::from_utf8(b.raw(&root)).unwrap().replacen(
        "\n\n",
        "\ngpgsig original signature\n continuation\n\n",
        1,
    );
    let source = b.oid(signed.as_bytes(), true);
    let origin = sha(&b.raw(&source));
    let unsigned = String::from_utf8(b.message(&source, "fix: signed origin", &[]))
        .unwrap()
        .replace("\ngpgsig original signature\n continuation", "");
    let candidate = with_origin(unsigned.as_bytes(), &origin);
    let mut e = origin_entry(&b, &source, &candidate, "add");
    e["remove_headers"] = json!(["gpgsig"]);
    let first = e["expected_oid"].as_str().unwrap().to_string();
    origin_manifest(&b, vec![e], &[]);
    let digest = b.digest();
    accepted(b.call(Some(&digest), false, &[]));
    let tree = b.f.raw(&["rev-parse", "HEAD^{tree}"], &b.repo, &[]);
    let other = b.f.commit(&b.repo, "other source", &[]);
    let merge_raw = format!(
        "tree {tree}\nparent {first}\nparent {other}\nauthor tester <44+tester@users.noreply.github.com> 100 +0930\ncommitter tester <44+tester@users.noreply.github.com> 200 -0500\n\nold merge\n"
    );
    let merge = b.oid(merge_raw.as_bytes(), true);
    let candidate = with_origin(
        &b.message(&merge, "fix: ordered merge", &[]),
        &sha(&b.raw(&merge)),
    );
    let e = origin_entry(&b, &merge, &candidate, "add");
    let new = e["expected_oid"].as_str().unwrap().to_string();
    let v2 = origin_manifest(&b, vec![e], &[first.clone(), other.clone()]);
    let digest = b.digest();
    accepted(b.call(Some(&digest), false, &[]));
    let actual = String::from_utf8(b.raw(&new)).unwrap();
    assert!(actual.contains(&format!("parent {first}\nparent {other}\n")));
    let mut v1 = v2;
    v1["policy_version"] = 1.into();
    v1.as_object_mut().unwrap().remove("provenance_profile");
    v1["entries"][0]
        .as_object_mut()
        .unwrap()
        .remove("source_provenance");
    b.write(&v1);
    refused(b.call(None, false, &[]));
    let malformed = String::from_utf8(b.raw(&first)).unwrap().replace(
        &format!("source-sha256 {origin}"),
        &format!("source-sha256 {}", origin.to_uppercase()),
    );
    let bad_boundary = b.oid(malformed.as_bytes(), true);
    let changed_merge = merge_raw.replace(
        &format!("parent {first}"),
        &format!("parent {bad_boundary}"),
    );
    let source = b.oid(changed_merge.as_bytes(), true);
    let c = with_origin(
        &b.message(&source, "fix: malformed boundary", &[]),
        &sha(&b.raw(&source)),
    );
    origin_manifest(
        &b,
        vec![origin_entry(&b, &source, &c, "add")],
        &[bad_boundary, other],
    );
    refused(b.call(None, false, &[]));
}

#[test]
fn origin_sha256_objects_roundtrip_and_shared_actual_header_checks_are_strict() {
    let mut b = Bulk::new();
    let repo = b.f.root.join("origin sha256");
    fs::create_dir(&repo).unwrap();
    accepted(b.f.command(
        &b.f.git,
        &["init", "-q", "-b", "main", "--object-format=sha256"],
        &repo,
        &[],
        None,
    ));
    b.repo = repo;
    let source = b.f.commit(&b.repo, "old sha256 root", &[]);
    assert_eq!(source.len(), 64);
    let candidate = with_origin(
        &b.message(&source, "fix: sha256 origin", &[]),
        &sha(&b.raw(&source)),
    );
    let entry = origin_entry(&b, &source, &candidate, "add");
    let expected = entry["expected_oid"].as_str().unwrap().to_string();
    origin_manifest(&b, vec![entry], &[]);
    let digest = b.digest();
    accepted(b.call(Some(&digest), false, &[]));
    assert_eq!(b.raw(&expected), candidate);
    accepted(b.f.canonical(&["commits", &expected], &b.repo, &[], None));
    assert_eq!(b.count(), 0);
    let raw = String::from_utf8(candidate).unwrap();
    let header = raw
        .lines()
        .find(|l| l.starts_with("source-sha256 "))
        .unwrap();
    let malformed = raw.replace(header, &format!("{header}\n{header}"));
    let oid = b.oid(malformed.as_bytes(), true);
    refused(b.f.canonical(&["commits", &oid], &b.repo, &[], None));
    let line = header.to_string();
    let committer = raw.lines().find(|l| l.starts_with("committer ")).unwrap();
    let wrong_slot = raw.replace(
        &format!("{committer}\n{line}"),
        &format!("{line}\n{committer}"),
    );
    let oid = b.oid(wrong_slot.as_bytes(), true);
    refused(b.f.canonical(&["commits", &oid], &b.repo, &[], None));
}

#[test]
fn origin_v1_fix_refuses_header_sources_before_creating_repair_state() {
    let b = Bulk::new();
    let base = b.f.commit(&b.repo, "feat: published base", &[]);
    let remote = b.f.bare("origin fix remote");
    b.f.raw(
        &["remote", "add", "origin", remote.to_str().unwrap()],
        &b.repo,
        &[],
    );
    b.f.raw(&["push", "-q", "origin", "main"], &b.repo, &[]);
    let source = b.f.commit(&b.repo, "fix: header source", &[]);
    let raw = b.raw(&source);
    let text = String::from_utf8(raw.clone()).unwrap();
    let with_header = text.replacen("\n\n", &format!("\nsource-sha256 {}\n\n", sha(&raw)), 1);
    let source = b.oid(with_header.as_bytes(), true);
    b.f.raw(&["update-ref", "refs/heads/main", &source], &b.repo, &[]);
    accepted(b.f.canonical_install(&[]));
    let before = b.snapshot();
    let plan = b.f.root.join("unsupported origin plan.json");
    let range = format!("{base}..HEAD");
    let result = b.f.canonical(
        &["fix", "--range", &range, "--plan", plan.to_str().unwrap()],
        &b.repo,
        &[],
        None,
    );
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("unsupported source commit header"));
    assert!(!plan.exists());
    assert!(!b.repo.join(".git/commitguard-fix").exists());
    assert_eq!(b.snapshot(), before);
}

#[test]
fn origin_interrupted_object_write_resumes_exactly_and_refuses_changed_input() {
    use std::os::unix::fs::symlink;
    let b = Bulk::new();
    let source = b.f.commit(&b.repo, "approved old source", &[]);
    let plain = b.message(&source, "fix: retry approved source", &[]);
    b.save(vec![b.entry(&source, &plain)], &[]);
    let legacy_digest = b.digest();
    let candidate = with_origin(&plain, &sha(&b.raw(&source)));
    let e = origin_entry(&b, &source, &candidate, "add");
    let expected = e["expected_oid"].as_str().unwrap().to_string();
    let candidate_path = PathBuf::from(e["candidate_file"].as_str().unwrap());
    origin_manifest(&b, vec![e], &[]);
    let digest = b.digest();
    refused(b.call(Some(&legacy_digest), false, &[]));
    fs::write(&candidate_path, b"changed input").unwrap();
    refused(b.call(Some(&digest), false, &[]));
    fs::write(&candidate_path, &candidate).unwrap();
    let before = b.snapshot();
    let git_link = b.f.bin.join("git");
    fs::remove_file(&git_link).unwrap();
    let native = b.f.git.to_string_lossy().replace('\'', "'\\''");
    executable(
        &git_link,
        &format!(
            "#!/bin/sh\ncase \" $* \" in *\" hash-object \"*\" -w \"*) '{native}' \"$@\"; exit 99;; esac\nexec '{native}' \"$@\"\n"
        ),
    );
    refused(b.call(Some(&digest), false, &[]));
    let operation = b.repo.join(".git/commitguard-bulk").join(&digest);
    assert!(operation.join("intent.json").exists());
    assert!(!operation.join("complete.json").exists());
    assert!(
        b.exists(&expected),
        "the fault occurs after writing immutable objects"
    );
    assert_eq!(b.snapshot(), before);
    fs::remove_file(&git_link).unwrap();
    symlink(&b.f.git, &git_link).unwrap();
    accepted(b.call(Some(&digest), false, &[]));
    accepted(b.call(Some(&digest), false, &[]));
    assert_eq!(b.raw(&expected), candidate);
    assert_eq!(b.snapshot(), before);
    assert_eq!(b.count(), 0);
}
