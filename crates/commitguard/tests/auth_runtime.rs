mod common;
use common::{Fixture, accepted, executable, refused};
use std::{fs, path::Path, process::Output};

// Deliberately separate from the shared fixture: distinguish local gh commands
// from API requests, and make unexpected gh commands fail rather than returning
// a plausible identity for every command.
struct AuthFixture {
    f: Fixture,
    calls: std::path::PathBuf,
}
impl AuthFixture {
    fn new() -> Self {
        let mut f = Fixture::new();
        let calls = f.root.join("api-calls");
        f.env
            .insert("AUTH_API_CALLS".into(), calls.display().to_string());
        executable(
            &f.bin.join("gh"),
            r#"#!/bin/sh
case "$*" in
 '--version') echo 'gh fixture';;
 'auth token --hostname github.com')
   printf '%s\n' "${GH_TOKEN:-${GITHUB_TOKEN:-${AUTH_TOKEN:-private-fixture-secret-a}}}";;
 'config get user --host github.com')
   if [ "${AUTH_NO_ACCOUNT:-0}" = 1 ]; then exit 1; fi
   printf '%s\n' "${AUTH_ACCOUNT:-tester}";;
 'api --hostname github.com user')
   printf 'api\n' >> "$AUTH_API_CALLS"
   if [ "${AUTH_OFFLINE:-0}" = 1 ]; then exit 1; fi
   if [ -n "${AUTH_RESPONSE:-}" ]; then printf '%s\n' "$AUTH_RESPONSE";
   else echo '{"login":"tester","id":44,"type":"User"}'; fi;;
 *) printf 'unexpected gh command\n' >&2; exit 78;;
esac
"#,
        );
        Self { f, calls }
    }
    fn call(&self, args: &[&str], extra: &[(&str, &str)]) -> Output {
        self.f.canonical(args, &self.f.root, extra, None)
    }
    fn api_count(&self) -> usize {
        fs::read_to_string(&self.calls)
            .unwrap_or_default()
            .lines()
            .count()
    }
    fn seed(&self) {
        accepted(self.call(&["--strict", "account"], &[]));
        assert_eq!(self.api_count(), 1, "strict must verify against the server");
    }
}

#[test]
fn strict_seed_then_warm_account_message_and_commits_are_api_free() {
    let a = AuthFixture::new();
    a.seed();
    accepted(a.call(&["account"], &[("AUTH_OFFLINE", "1")]));
    let message = a.f.root.join("message");
    fs::write(&message, "fix: preserve cached identity\n").unwrap();
    accepted(a.call(
        &["message", message.to_str().unwrap()],
        &[("AUTH_OFFLINE", "1")],
    ));
    let repo = a.f.repo("repo");
    let oid = a.f.commit(&repo, "fix: preserve cached identity", &[]);
    accepted(a.f.canonical(&["commits", &oid], &repo, &[("AUTH_OFFLINE", "1")], None));
    assert_eq!(
        a.api_count(),
        1,
        "normal commands must never make an API request"
    );
}

#[test]
fn cold_default_refuses_without_any_api_request() {
    let a = AuthFixture::new();
    let output = a.call(&["account"], &[]);
    assert_eq!(a.api_count(), 0, "cold cache must not implicitly refresh");
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("strict"));
}

#[test]
fn changed_credential_account_or_config_context_never_implicitly_refreshes() {
    let a = AuthFixture::new();
    a.seed();
    for extra in [
        vec![("AUTH_TOKEN", "private-fixture-secret-b")],
        vec![("AUTH_ACCOUNT", "changed")],
        vec![("GH_TOKEN", "private-env-token")],
        vec![("GITHUB_TOKEN", "private-env-token")],
        vec![("GH_CONFIG_DIR", "/tmp/fixture-new-gh-context")],
    ] {
        refused(a.call(&["account"], &extra));
        assert_eq!(
            a.api_count(),
            1,
            "changed local context must refuse without API"
        );
    }
    accepted(a.call(&["account"], &[]));
}

#[test]
fn environment_only_token_precedence_is_supported() {
    let a = AuthFixture::new();
    let extra = [
        ("AUTH_NO_ACCOUNT", "1"),
        ("GH_TOKEN", "private-primary-token"),
        ("GITHUB_TOKEN", "private-shadowed-token"),
    ];
    accepted(a.call(&["--strict", "account"], &extra));
    accepted(a.call(&["account"], &extra));
    accepted(a.call(
        &["account"],
        &[
            ("AUTH_NO_ACCOUNT", "1"),
            ("GH_TOKEN", "private-primary-token"),
            ("GITHUB_TOKEN", "private-other-shadowed-token"),
        ],
    ));
    assert_eq!(a.api_count(), 1);
    refused(a.call(
        &["account"],
        &[
            ("AUTH_NO_ACCOUNT", "1"),
            ("GITHUB_TOKEN", "private-primary-token"),
        ],
    ));
    assert_eq!(
        a.api_count(),
        1,
        "auth source change must not implicitly refresh"
    );
}

#[test]
fn strict_invalid_human_identity_or_online_failure_never_falls_back() {
    let a = AuthFixture::new();
    a.seed();
    let responses = [
        r#"{"login":"tester","id":44,"type":"Bot"}"#,
        r#"{"login":"tester","id":44}"#,
        r#"{"login":"tester","id":0,"type":"User"}"#,
        r#"{"login":"tester","id":"44","type":"User"}"#,
        r#"{"login":"tester","id":9007199254740992,"type":"User"}"#,
    ];
    for (index, response) in responses.iter().enumerate() {
        refused(a.call(&["--strict", "account"], &[("AUTH_RESPONSE", response)]));
        assert_eq!(a.api_count(), index + 2);
    }
    refused(a.call(&["--strict", "account"], &[("AUTH_OFFLINE", "1")]));
    assert_eq!(a.api_count(), responses.len() + 2);
}

#[test]
fn push_always_requires_online_identity_even_with_warm_cache() {
    let a = AuthFixture::new();
    a.seed();
    let repo = a.f.repo("repo");
    a.f.commit(&repo, "fix: validate outgoing identity", &[]);
    let remote = a.f.bare("remote.git");
    refused(a.f.canonical(
        &["push", remote.to_str().unwrap(), "HEAD"],
        &repo,
        &[("AUTH_OFFLINE", "1")],
        None,
    ));
    assert_eq!(
        a.api_count(),
        2,
        "push must online-verify even without --strict"
    );
    let oid = a.f.raw(&["rev-parse", "HEAD"], &repo, &[]);
    let payload = format!("refs/heads/main {oid} refs/heads/main {}\n", "0".repeat(40));
    refused(a.f.canonical(
        &["pre-push", remote.to_str().unwrap()],
        &repo,
        &[("AUTH_OFFLINE", "1")],
        Some(payload.as_bytes()),
    ));
    assert_eq!(a.api_count(), 3, "pre-push also must verify online");
}

fn assert_private_files(path: &Path) {
    use std::os::unix::fs::PermissionsExt;
    for entry in fs::read_dir(path).unwrap() {
        let path = entry.unwrap().path();
        let meta = fs::symlink_metadata(&path).unwrap();
        if meta.is_dir() {
            assert_private_files(&path);
        } else if meta.is_file() && path.file_name().unwrap() != ".gitconfig" {
            let bytes = fs::read(&path).unwrap();
            assert!(
                !String::from_utf8_lossy(&bytes).contains("private-fixture-secret"),
                "token leaked in {}",
                path.display()
            );
            assert_eq!(
                meta.permissions().mode() & 0o077,
                0,
                "cache file must be private: {}",
                path.display()
            );
        }
    }
}
#[test]
fn verified_cache_and_errors_never_expose_token_bytes() {
    let a = AuthFixture::new();
    let output = a.call(&["--strict", "account"], &[]);
    assert!(!String::from_utf8_lossy(&output.stdout).contains("private-fixture-secret"));
    assert!(!String::from_utf8_lossy(&output.stderr).contains("private-fixture-secret"));
    accepted(output);
    assert_private_files(&a.f.home);
    let output = a.call(&["account"], &[("AUTH_TOKEN", "private-fixture-secret-b")]);
    assert!(!String::from_utf8_lossy(&output.stderr).contains("private-fixture-secret"));
    refused(output);
}

#[test]
fn malformed_public_and_symlink_cache_refuse_without_api() {
    use std::os::unix::fs::{PermissionsExt, symlink};
    let a = AuthFixture::new();
    a.seed();
    let dir = a.f.home.join(".local/state/commitguard");
    let records = || {
        fs::read_dir(&dir)
            .unwrap()
            .map(|e| e.unwrap().path())
            .filter(|p| p.extension().is_some_and(|x| x == "json"))
            .collect::<Vec<_>>()
    };
    for path in records() {
        fs::write(path, b"{}").unwrap();
    }
    refused(a.call(&["account"], &[]));
    assert_eq!(a.api_count(), 1);
    accepted(a.call(&["--strict", "account"], &[]));
    for path in records() {
        fs::set_permissions(path, fs::Permissions::from_mode(0o644)).unwrap();
    }
    refused(a.call(&["account"], &[]));
    assert_eq!(a.api_count(), 2);
    // Strict renewal replaces each destination atomically, never opens its target.
    accepted(a.call(&["--strict", "account"], &[]));
    let target = a.f.root.join("unrelated");
    fs::write(&target, b"unchanged").unwrap();
    for path in records() {
        fs::remove_file(&path).unwrap();
        symlink(&target, path).unwrap();
    }
    refused(a.call(&["account"], &[]));
    assert_eq!(a.api_count(), 3);
    accepted(a.call(&["--strict", "account"], &[]));
    assert_eq!(fs::read(&target).unwrap(), b"unchanged");
    fs::set_permissions(&dir, fs::Permissions::from_mode(0o755)).unwrap();
    refused(a.call(&["account"], &[]));
    assert_eq!(a.api_count(), 4);
}

#[test]
fn strict_api_is_pinned_and_local_credential_race_refuses_cache_update() {
    let a = AuthFixture::new();
    let state = a.f.root.join("credential-state");
    fs::write(&state, "private-start-token\n").unwrap();
    executable(
        &a.f.bin.join("gh"),
        r#"#!/bin/sh
case "$*" in
 'auth token --hostname github.com') /bin/cat "$AUTH_TOKEN_FILE";;
 'config get user --host github.com') echo tester;;
 'api --hostname github.com user')
   printf 'api\n' >> "$AUTH_API_CALLS"
   if [ "$GH_TOKEN" != private-start-token ]; then exit 79; fi
   printf 'private-replaced-token\n' > "$AUTH_TOKEN_FILE"
   echo '{"login":"tester","id":44,"type":"User"}';;
 *) exit 78;;
esac
"#,
    );
    let extra = [
        ("AUTH_TOKEN_FILE", state.to_str().unwrap()),
        ("GITHUB_TOKEN", "private-shadowed-env"),
    ];
    refused(a.call(&["--strict", "account"], &extra));
    assert_eq!(
        a.api_count(),
        1,
        "API must use captured effective token despite inherited env"
    );
    refused(a.call(&["account"], &extra));
    assert_eq!(
        a.api_count(),
        1,
        "raced strict verification must not create valid cache"
    );
}

#[test]
fn strict_flag_does_not_consume_literal_message_filename() {
    let a = AuthFixture::new();
    a.seed();
    fs::write(a.f.root.join("--strict"), "fix: retain literal filename\n").unwrap();
    accepted(a.call(&["message", "--strict"], &[]));
    assert_eq!(a.api_count(), 1);
    accepted(a.call(&["account", "--strict"], &[]));
    assert_eq!(a.api_count(), 2);
}

#[test]
fn installed_creation_hooks_are_api_free_and_explicit_strict_git_is_stronger() {
    let a = AuthFixture::new();
    accepted(a.f.canonical_install(&[]));
    assert_eq!(a.api_count(), 1, "successful setup seeds verified cache");
    let repo = a.f.repo("guarded");
    accepted(a.f.guarded(
        &["commit", "--allow-empty", "-m", "fix: cached creation"],
        &repo,
        &[("AUTH_OFFLINE", "1")],
    ));
    assert_eq!(a.api_count(), 1, "creation hooks must use cache");
    refused(a.f.command(
        &a.f.guard_root().join("bin/commitguard"),
        &[
            "--strict",
            "git",
            "commit",
            "--allow-empty",
            "-m",
            "fix: strict creation",
        ],
        &repo,
        &[("AUTH_OFFLINE", "1")],
        None,
    ));
    assert!(
        a.api_count() > 1,
        "explicit strict proxy must strengthen hook checks"
    );
    let before = a.api_count();
    refused(a.f.guarded(&["commit", "--allow-empty", "-m", "--strict"], &repo, &[]));
    assert_eq!(
        a.api_count(),
        before,
        "Git message argument must not become strict flag"
    );
}

#[test]
fn container_cache_cannot_enter_protected_host_agent_configuration() {
    let a = AuthFixture::new();
    let repo = a.f.repo("container");
    let state = a.f.home.join(".codex/host-state");
    let codex = repo.join(".codex");
    let before = fs::read(&a.f.global).unwrap();
    refused(a.f.canonical(
        &["install", "--container", "--repo", repo.to_str().unwrap()],
        &repo,
        &[
            ("XDG_STATE_HOME", state.to_str().unwrap()),
            ("CODEX_HOME", codex.to_str().unwrap()),
        ],
        None,
    ));
    assert!(!state.exists());
    assert_eq!(fs::read(&a.f.global).unwrap(), before);
    assert!(!a.f.guard_root().exists());
    assert_eq!(a.api_count(), 0);
}

#[test]
fn oversized_gh_output_is_terminated_before_full_capture_or_deadline() {
    for mode in ["token", "config", "api"] {
        let a = AuthFixture::new();
        executable(
            &a.f.bin.join("gh"),
            r#"#!/bin/sh
huge() { while :; do printf 'xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx\n'; done; }
case "$*" in
 'auth token --hostname github.com') if [ "$AUTH_HUGE" = token ]; then huge; else echo fixture-token; fi;;
 'config get user --host github.com') if [ "$AUTH_HUGE" = config ]; then huge; else echo tester; fi;;
 'api --hostname github.com user') huge;;
 *) exit 78;;
esac
"#,
        );
        let start = std::time::Instant::now();
        let result = a.call(&["--strict", "account"], &[("AUTH_HUGE", mode)]);
        assert!(String::from_utf8_lossy(&result.stderr).contains("output exceeded safe limit"));
        assert!(
            start.elapsed() < std::time::Duration::from_secs(3),
            "oversized child must be terminated promptly"
        );
        refused(result);
    }
}
