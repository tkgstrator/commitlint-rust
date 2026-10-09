use super::*;

#[test]
fn signed_off_by_raw_lines() {
    let sign = |text: &str| RuleValue::Text(text.into());
    let msg_ok = "feat: x\n\nSigned-off-by: A <a@example.com>";
    assert_eq!(
        ok("signed-off-by", msg_ok, None, sign("Signed-off-by:")),
        msg(true, "message must be signed off")
    );
    assert_eq!(
        ok("signed-off-by", msg_ok, NEVER, sign("Signed-off-by:")),
        msg(false, "message must not be signed off")
    );
    assert!(
        !ok(
            "signed-off-by",
            "feat: x\n\nbody",
            ALWAYS,
            sign("Signed-off-by:")
        )
        .valid
    );
    assert!(
        ok(
            "signed-off-by",
            "feat: x\n\nbody",
            NEVER,
            sign("Signed-off-by:")
        )
        .valid
    );
    // Only the last significant line counts.
    assert!(
        !ok(
            "signed-off-by",
            "feat: x\n\nSigned-off-by: A\n\ntrailing",
            ALWAYS,
            sign("Signed-off-by:")
        )
        .valid
    );
    // Comments, blank lines and cherry-pick lines are skipped.
    let skipped =
        "feat: x\n\nSigned-off-by: A\n\n# comment\n(cherry picked from commit abcdef1234567)\n\n";
    assert!(ok("signed-off-by", skipped, ALWAYS, sign("Signed-off-by:")).valid);
    let upper = "feat: x\n\nSigned-off-by: A\n(Cherry Picked From Commit ABCDEF1234567890)\n";
    assert!(ok("signed-off-by", upper, ALWAYS, sign("Signed-off-by:")).valid);
    // Six hex digits is too short, so the line counts as the last line.
    let short = "feat: x\n\nSigned-off-by: A\n(cherry picked from commit abcdef)";
    assert!(!ok("signed-off-by", short, ALWAYS, sign("Signed-off-by:")).valid);
    // Surrounding whitespace is trimmed only for the cherry-pick test.
    let padded = "feat: x\n\nSigned-off-by: A\n  (cherry picked from commit abcdef1234567)  ";
    assert!(ok("signed-off-by", padded, ALWAYS, sign("Signed-off-by:")).valid);
    // The default prefix is empty: any non-empty last line starts with it.
    assert!(ok("signed-off-by", "feat: x", None, RuleValue::None).valid);
    assert!(
        !ok(
            "signed-off-by",
            "feat: x\n\n# only comment",
            None,
            sign("x")
        )
        .valid
    );
}

#[test]
fn trailer_exists_requires_git_and_fails_closed() {
    let raw = "feat: x\n\nSigned-off-by: A <a@example.com>";
    let parsed = parse_message(raw, ParserPreset::ConventionalCommits).unwrap();
    let missing = EvaluationContext {
        git: Some(PathBuf::from("/definitely/not/a/git")),
        ..EvaluationContext::default()
    };
    // Operational failure is fatal for every condition: no RuleOutcome.
    for when in [None, ALWAYS, NEVER] {
        let result = evaluate_rule_with_context(
            "trailer-exists",
            &parsed,
            raw,
            when,
            &RuleValue::Text("Signed-off-by:".into()),
            &missing,
        );
        assert!(result.is_err(), "{when:?}");
    }
    // Wrong value types are rejected before Git is consulted.
    let result = evaluate_rule_with_context(
        "trailer-exists",
        &parsed,
        raw,
        ALWAYS,
        &RuleValue::Length(1),
        &missing,
    );
    assert!(result.unwrap_err().contains("text"));
}

#[test]
fn trailer_exists_with_real_git_when_available() {
    let Ok(git) = commitlint_rust::git::find_git() else {
        return; // Git is only needed by this rule; nothing to verify without it.
    };
    let context = EvaluationContext {
        git: Some(git),
        ..EvaluationContext::default()
    };
    let raw = "feat: x\n\nbody\n\nSigned-off-by: A <a@example.com>\n";
    let parsed = parse_message(raw, ParserPreset::ConventionalCommits).unwrap();
    let eval = |when, prefix: &str| {
        evaluate_rule_with_context(
            "trailer-exists",
            &parsed,
            raw,
            when,
            &RuleValue::Text(prefix.into()),
            &context,
        )
        .unwrap()
    };
    assert_eq!(
        eval(ALWAYS, "Signed-off-by:"),
        msg(true, "message must have `Signed-off-by:` trailer")
    );
    assert_eq!(
        eval(NEVER, "Signed-off-by:"),
        msg(false, "message must not have `Signed-off-by:` trailer")
    );
    assert!(!eval(None, "Reviewed-by:").valid);
    assert!(eval(NEVER, "Reviewed-by:").valid);
}
