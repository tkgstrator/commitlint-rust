#![cfg(unix)]
mod common;
use commitlint_rust::{
    git::interpret_trailers,
    parser::{ParserPreset, parse_message},
    rules::{EvaluationContext, RuleCondition, RuleValue, evaluate_rule_with_context},
};
use common::{Fixture, executable};
use std::{
    fs,
    path::Path,
    process::Command,
    time::{Duration, Instant},
};

fn run(
    f: &Fixture,
    script: &str,
    input: &str,
    timeout: Duration,
    limit: usize,
) -> Result<String, String> {
    let exe = f.root.join("fake git");
    executable(&exe, script);
    interpret_trailers(input, Some(&exe), Some(&f.root), timeout, limit)
}

#[test]
fn trailer_capture_uses_exact_args_raw_input_and_cwd() {
    let f = Fixture::new();
    let script = "#!/bin/sh\n[ \"$1\" = interpret-trailers ] && [ \"$2\" = --parse ] && [ \"$#\" = 2 ] || exit 18\n/bin/cat\n";
    let raw = "fix: x\r\n\nSigned-off-by: person\n";
    assert_eq!(
        run(&f, script, raw, Duration::from_secs(2), 4096).unwrap(),
        raw
    );
    let output = run(
        &f,
        "#!/bin/sh\n/bin/pwd\n",
        "",
        Duration::from_secs(2),
        4096,
    )
    .unwrap();
    assert_eq!(
        Path::new(output.trim_end()).canonicalize().unwrap(),
        f.root.canonicalize().unwrap()
    );
}

#[test]
fn missing_nonzero_invalid_utf8_and_broken_pipe_are_errors() {
    let f = Fixture::new();
    assert!(
        interpret_trailers(
            "x",
            Some(&f.root.join("missing")),
            Some(&f.root),
            Duration::from_secs(1),
            4096
        )
        .is_err()
    );
    assert!(
        run(
            &f,
            "#!/bin/sh\nexit 17\n",
            "x",
            Duration::from_secs(1),
            4096
        )
        .is_err()
    );
    assert!(
        run(
            &f,
            "#!/bin/sh\nprintf '\\377'\n",
            "",
            Duration::from_secs(1),
            4096
        )
        .unwrap_err()
        .contains("UTF-8")
    );
    assert!(
        run(
            &f,
            "#!/bin/sh\nexec 0<&-\n/bin/sleep 60\n",
            &"x".repeat(2_000_000),
            Duration::from_millis(100),
            4096
        )
        .is_err()
    );
}

#[test]
fn blocked_stdin_and_held_pipes_obey_deadline() {
    let f = Fixture::new();
    for (script, input) in [
        ("#!/bin/sh\n/bin/sleep 60 &\nwait\n", "x".repeat(2_000_000)),
        ("#!/bin/sh\n/bin/sleep 60 &\nexit 0\n", "x".into()),
    ] {
        let start = Instant::now();
        let result = run(&f, script, &input, Duration::from_millis(100), 4096);
        assert!(result.unwrap_err().contains("timed out"));
        assert!(start.elapsed() < Duration::from_secs(2));
    }
}

#[test]
fn stdout_and_stderr_overflow_fail_without_hanging() {
    let f = Fixture::new();
    for script in [
        "#!/bin/sh\nexec /usr/bin/yes x\n",
        "#!/bin/sh\nexec /usr/bin/yes x >&2\n",
    ] {
        let start = Instant::now();
        assert!(
            run(&f, script, "", Duration::from_secs(1), 32)
                .unwrap_err()
                .contains("exceeds limit")
        );
        assert!(start.elapsed() < Duration::from_secs(2));
    }
}

#[test]
fn timeout_cleans_owned_process_group() {
    let f = Fixture::new();
    let pidfile = f.root.join("owned child pid");
    let script = format!(
        "#!/bin/sh\n/bin/sleep 60 &\necho \"$!\" > '{}'\nexit 0\n",
        pidfile.display()
    );
    assert!(
        run(&f, &script, "", Duration::from_millis(150), 4096)
            .unwrap_err()
            .contains("timed out")
    );
    let pid = fs::read_to_string(&pidfile).unwrap();
    // The child can briefly remain a zombie awaiting its OS reaper, but it
    // must not remain runnable. Check only the exact PID created by this test.
    let output = Command::new("/bin/ps")
        .args(["-p", pid.trim(), "-o", "stat="])
        .output()
        .unwrap();
    let state = String::from_utf8(output.stdout).unwrap();
    assert!(
        state.trim().is_empty() || state.trim_start().starts_with('Z'),
        "owned child remains running: {pid} {state}"
    );
}

#[test]
fn trailer_failures_propagate_for_never_and_empty_value_matches_empty_output() {
    let f = Fixture::new();
    let exe = f.root.join("trailer tool");
    executable(&exe, "#!/bin/sh\nexit 0\n");
    let raw = "fix: x";
    let parsed = parse_message(raw, ParserPreset::ConventionalCommits).unwrap();
    let mut ctx = EvaluationContext {
        git: Some(exe),
        cwd: Some(f.root.clone()),
        timeout: Duration::from_secs(1),
        output_limit: 4096,
    };
    let result =
        evaluate_rule_with_context("trailer-exists", &parsed, raw, None, &RuleValue::None, &ctx)
            .unwrap();
    assert!(result.valid);
    ctx.git = Some(f.root.join("missing"));
    assert!(
        evaluate_rule_with_context(
            "trailer-exists",
            &parsed,
            raw,
            Some(RuleCondition::Never),
            &RuleValue::Text("Signed-off-by:".into()),
            &ctx
        )
        .is_err()
    );
}

#[test]
fn actual_git_reads_trailers_without_identity_or_network_tools() {
    let f = Fixture::new();
    let output = interpret_trailers(
        "fix: x\n\nSigned-off-by: person <person@example.com>\n",
        Some(Path::new(&f.git)),
        Some(&f.root),
        Duration::from_secs(2),
        4096,
    )
    .unwrap();
    assert_eq!(output, "Signed-off-by: person <person@example.com>\n");
}
