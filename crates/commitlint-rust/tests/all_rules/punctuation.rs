use super::*;

#[test]
fn full_stop_rules() {
    // body-full-stop checks the final UTF-16 unit only.
    assert_eq!(
        ok("body-full-stop", "fix: x\n\nbody.", ALWAYS, RuleValue::None),
        msg(true, "body must end with full stop")
    );
    assert_eq!(
        ok("body-full-stop", "fix: x\n\nbody.", NEVER, RuleValue::None),
        msg(false, "body may not end with full stop")
    );
    assert_eq!(
        ok("body-full-stop", "fix: x", ALWAYS, RuleValue::None),
        bare()
    );
    assert!(
        ok(
            "body-full-stop",
            "fix: x\n\nbody!",
            ALWAYS,
            RuleValue::Text("!".into())
        )
        .valid
    );
    assert!(
        !ok(
            "body-full-stop",
            "fix: x\n\nbody.",
            ALWAYS,
            RuleValue::Text("..".into())
        )
        .valid
    );
    // A multi-unit value never equals a single unit; so does an empty value.
    assert!(
        !ok(
            "body-full-stop",
            "fix: x\n\nbody",
            ALWAYS,
            RuleValue::Text(String::new())
        )
        .valid
    );
    // Astral last character: the final unit is a low surrogate, never a char.
    assert!(
        !ok(
            "body-full-stop",
            "fix: x\n\nbody😀",
            ALWAYS,
            RuleValue::Text("😀".into())
        )
        .valid
    );

    assert_eq!(
        ok("header-full-stop", "fix: x.", ALWAYS, RuleValue::None),
        msg(true, "header must end with full stop")
    );
    assert_eq!(
        ok("header-full-stop", "fix: x", NEVER, RuleValue::None),
        msg(true, "header may not end with full stop")
    );
    // Ellipsis exception is subject-full-stop only.
    assert!(ok("header-full-stop", "fix: x...", ALWAYS, RuleValue::None).valid);

    assert_eq!(
        ok("subject-full-stop", "fix: x.", ALWAYS, RuleValue::None),
        msg(true, "subject must end with full stop")
    );
    assert!(!ok("subject-full-stop", "fix: x...", ALWAYS, RuleValue::None).valid);
    assert!(ok("subject-full-stop", "fix: x...", NEVER, RuleValue::None).valid);
    // Colon as final header character passes early with a bare outcome.
    let parsed = parse_message("fix:", ParserPreset::ConventionalCommits).unwrap();
    assert_eq!(
        evaluate_rule(
            "subject-full-stop",
            &parsed,
            "fix:",
            NEVER,
            &RuleValue::None
        )
        .unwrap(),
        bare()
    );
    assert!(
        ok(
            "subject-full-stop",
            "fix: x!",
            ALWAYS,
            RuleValue::Text("!".into())
        )
        .valid
    );
}

#[test]
fn leading_blank_rules() {
    assert_eq!(
        ok(
            "body-leading-blank",
            "fix: x\n\nbody",
            ALWAYS,
            RuleValue::None
        ),
        msg(true, "body must have leading blank line")
    );
    assert_eq!(
        ok("body-leading-blank", "fix: x\nbody", None, RuleValue::None),
        msg(false, "body must have leading blank line")
    );
    assert_eq!(
        ok("body-leading-blank", "fix: x\nbody", NEVER, RuleValue::None),
        msg(true, "body may not have leading blank line")
    );
    assert_eq!(
        ok("body-leading-blank", "fix: x", NEVER, RuleValue::None),
        bare()
    );
    assert_eq!(
        ok(
            "footer-leading-blank",
            "fix: x\n\nCloses #1",
            ALWAYS,
            RuleValue::None
        ),
        msg(true, "footer must have leading blank line")
    );
    assert_eq!(
        ok(
            "footer-leading-blank",
            "fix: x\nCloses #1",
            ALWAYS,
            RuleValue::None
        ),
        msg(false, "footer must have leading blank line")
    );
    assert_eq!(
        ok(
            "footer-leading-blank",
            "fix: x\n\nbody\n\nCloses #1",
            NEVER,
            RuleValue::None
        ),
        msg(false, "footer may not have leading blank line")
    );
    assert_eq!(
        ok("footer-leading-blank", "fix: x", ALWAYS, RuleValue::None),
        bare()
    );
}

#[test]
fn subject_exclamation_mark_rule() {
    let tail = "have an exclamation mark in the subject to identify a breaking change";
    assert_eq!(
        ok(
            "subject-exclamation-mark",
            "feat!: x",
            None,
            RuleValue::None
        ),
        msg(true, &format!("subject must {tail}"))
    );
    assert_eq!(
        ok(
            "subject-exclamation-mark",
            "feat(a)!: x",
            NEVER,
            RuleValue::None
        ),
        msg(false, &format!("subject must not {tail}"))
    );
    assert!(
        !ok(
            "subject-exclamation-mark",
            "feat: x",
            ALWAYS,
            RuleValue::None
        )
        .valid
    );
    assert!(
        ok(
            "subject-exclamation-mark",
            "feat: x",
            NEVER,
            RuleValue::None
        )
        .valid
    );
    // An empty header passes with an empty (not absent) message.
    let parsed = parse_message("gpg: diagnostic", ParserPreset::ConventionalCommits).unwrap();
    assert_eq!(
        evaluate_rule(
            "subject-exclamation-mark",
            &parsed,
            "gpg: diagnostic",
            ALWAYS,
            &RuleValue::None
        )
        .unwrap(),
        msg(true, "")
    );
}
