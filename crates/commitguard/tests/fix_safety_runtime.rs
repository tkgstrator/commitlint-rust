#![cfg(unix)]
mod common;
use common::*;
use std::{
    fs,
    process::{Command, Stdio},
    time::{Duration, Instant},
};

#[test]
fn original_hook_large_error_is_bounded_redacted_and_preserves_the_source() {
    let f = Fixture::new();
    let repo = f.repo("diagnostic source");
    let base = f.commit(&repo, "feat: base", &[]);
    let remote = f.bare("diagnostic remote");
    f.raw(
        &["remote", "add", "origin", remote.to_str().unwrap()],
        &repo,
        &[],
    );
    f.raw(&["push", "-q", "origin", "main"], &repo, &[]);
    let source = f.commit(&repo, "old message needs correction", &[]);
    let hooks = repo.join(".git/original-hooks");
    fs::create_dir_all(&hooks).unwrap();
    let hook_marker = f.root.join("hook started marker");
    assert!(!hook_marker.exists());
    executable(
        &hooks.join("pre-commit"),
        "#!/bin/sh\n: > \"$FIXTURE_HOOK_STARTED\" || true\nprintf 'safety-hook rejected ghp_testHookToken https://user:password@github.com/repo\\n' >&2\ni=0\nwhile [ \"$i\" -lt 20000 ]; do\n printf 'large hook diagnostic line for continuous drain verification\\n' >&2\n i=$((i+1))\ndone\nexit 1\n",
    );
    f.raw(
        &["config", "core.hooksPath", hooks.to_str().unwrap()],
        &repo,
        &[],
    );
    accepted(f.canonical_install(&[]));
    let plan = f.root.join("diagnostic plan.json");
    accepted(f.canonical(
        &[
            "fix",
            "--range",
            &format!("{base}..HEAD"),
            "--plan",
            plan.to_str().unwrap(),
        ],
        &repo,
        &[],
        None,
    ));
    let mut proposal: serde_json::Value =
        serde_json::from_slice(&fs::read(&plan).unwrap()).unwrap();
    proposal["candidates"][0]["message"] = "fix: corrected message".into();
    fs::write(&plan, serde_json::to_vec(&proposal).unwrap()).unwrap();
    let error_path = f.root.join("captured error.txt");
    let mut child = Command::new(canonical_exe())
        .args(["fix", "--apply", plan.to_str().unwrap()])
        .current_dir(&repo)
        .env_clear()
        .envs(&f.env)
        .env("FIXTURE_HOOK_STARTED", &hook_marker)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(fs::File::create(&error_path).unwrap())
        .spawn()
        .unwrap();
    // The pipe-drain watchdog begins when the hook starts, not during the
    // preceding guarded Git/fsync preparation on a variable-speed CI runner.
    let preparation_started = Instant::now();
    let mut hook_started = None;
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        if hook_started.is_none() && hook_marker.exists() {
            hook_started = Some(Instant::now());
        }
        let timeout = match hook_started {
            Some(started) if started.elapsed() > Duration::from_secs(30) => {
                Some("large hook stderr did not drain within 30 seconds after hook start")
            }
            None if preparation_started.elapsed() > Duration::from_secs(120) => {
                Some("apply preparation did not reach the original hook within 120 seconds")
            }
            _ => None,
        };
        if let Some(reason) = timeout {
            let _ = child.kill();
            let _ = child.wait();
            panic!("{reason}");
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    assert!(
        hook_marker.exists(),
        "apply failed before the original hook started"
    );
    assert!(!status.success());
    let error = fs::read_to_string(error_path).unwrap();
    assert!(error.contains("safety-hook rejected"), "{error}");
    assert!(error.contains("diagnostic truncated"));
    assert!(!error.contains("ghp_testHookToken"));
    assert!(!error.contains("password"));
    assert!(error.len() < 32 * 1024);
    assert_eq!(f.raw(&["rev-parse", "HEAD"], &repo, &[]), source);
    let id = proposal["plan_id"].as_str().unwrap();
    assert_eq!(
        f.raw(
            &["rev-parse", &format!("refs/commitguard/backups/{id}")],
            &repo,
            &[]
        ),
        source
    );
    let journal: serde_json::Value = serde_json::from_slice(
        &fs::read(repo.join(format!(".git/commitguard-fix/operation-{id}/journal.json"))).unwrap(),
    )
    .unwrap();
    assert!(journal["failure"].as_str().unwrap().len() <= 16 * 1024);
}
