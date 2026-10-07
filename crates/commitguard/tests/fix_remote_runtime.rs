#![cfg(unix)]
mod common;
use common::*;
use std::{fs, path::PathBuf};

struct Source {
    f: Fixture,
    repo: PathBuf,
    base: String,
    tip: String,
    remote: PathBuf,
}
impl Source {
    fn new() -> Self {
        let f = Fixture::new();
        let repo = f.repo("remote safety");
        let base = f.commit(&repo, "feat: base", &[]);
        let remote = f.bare("remote safety destination");
        f.raw(
            &["remote", "add", "origin", remote.to_str().unwrap()],
            &repo,
            &[],
        );
        f.raw(&["push", "-q", "origin", "main"], &repo, &[]);
        let tip = f.commit(&repo, "source message", &[]);
        accepted(f.canonical_install(&[]));
        Self {
            f,
            repo,
            base,
            tip,
            remote,
        }
    }
    fn export(&self, path: &std::path::Path, env: &[(&str, &str)]) -> std::process::Output {
        self.f.canonical(
            &[
                "fix",
                "--range",
                &format!("{}..HEAD", self.base),
                "--plan",
                path.to_str().unwrap(),
            ],
            &self.repo,
            env,
            None,
        )
    }
}

#[test]
fn fetch_and_push_url_credentials_are_refused_before_persisting_or_echoing_them() {
    let s = Source::new();
    let plan = s.f.root.join("remote plan.json");
    for push in [false, true] {
        let key = if push {
            "remote.origin.pushurl"
        } else {
            "remote.origin.url"
        };
        for url in [
            "ssh://git:private-pass@host/repo",
            "https://host/repo?access_token=private-pass",
            "custom://user:private-pass@host/repo",
        ] {
            s.f.raw(&["config", key, url], &s.repo, &[]);
            let output = s.export(&plan, &[]);
            assert!(!output.status.success());
            assert!(!String::from_utf8_lossy(&output.stderr).contains("private-pass"));
            assert!(!String::from_utf8_lossy(&output.stdout).contains("private-pass"));
            assert!(!plan.exists());
            assert_eq!(s.f.raw(&["rev-parse", "HEAD"], &s.repo, &[]), s.tip);
        }
        s.f.raw(&["config", key, s.remote.to_str().unwrap()], &s.repo, &[]);
    }
    accepted(s.export(&plan, &[]));
}

#[test]
fn proposal_export_refuses_the_entire_common_git_directory() {
    let s = Source::new();
    let output = s.repo.join(".git/objects/proposal.json");
    let result = s.export(&output, &[]);
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("outside the common Git directory"));
    assert!(!output.exists());
    accepted(s.export(&s.f.root.join("outside plan.json"), &[]));
}

#[test]
fn live_remote_probe_disables_prompts_without_replacing_the_ssh_command() {
    let s = Source::new();
    let script = s.f.root.join("chosen ssh transport");
    let record = s.f.root.join("transport environment");
    let payload = format!("{} refs/heads/main\n", s.base);
    let packet = format!("{:04x}{}0000", payload.len() + 4, payload);
    let quoted_record = format!("'{}'", record.to_str().unwrap().replace('\'', "'\\''"));
    executable(
        &script,
        &format!(
            "#!/bin/sh\nprintf '%s %s\\n' \"$GIT_TERMINAL_PROMPT\" \"$GCM_INTERACTIVE\" >> {quoted_record}\nprintf '%s' '{packet}'\n/bin/cat > /dev/null\n"
        ),
    );
    s.f.raw(
        &[
            "remote",
            "set-url",
            "origin",
            "ssh://git@example.invalid/repo",
        ],
        &s.repo,
        &[],
    );
    let command = format!("'{}'", script.to_str().unwrap().replace('\'', "'\\''"));
    accepted(s.export(
        &s.f.root.join("ssh plan.json"),
        &[("GIT_SSH_COMMAND", &command), ("GIT_SSH_VARIANT", "ssh")],
    ));
    let observed = fs::read_to_string(record).unwrap();
    assert!(!observed.is_empty());
    assert!(observed.lines().all(|line| line == "0 Never"), "{observed}");
}
