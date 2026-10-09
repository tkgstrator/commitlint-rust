use super::*;

#[test]
fn header_trim_reports_each_form() {
    let parsed_with = |header: &str| {
        let mut parsed = parse_message("fix: x", ParserPreset::ConventionalCommits).unwrap();
        parsed.header = Some(header.to_owned());
        parsed
    };
    let run_trim = |header: &str| {
        evaluate_rule(
            "header-trim",
            &parsed_with(header),
            "fix: x",
            None,
            &RuleValue::None,
        )
        .unwrap()
    };
    assert_eq!(run_trim("fix: x"), bare());
    assert_eq!(
        run_trim(" fix: x"),
        msg(false, "header must not start with whitespace")
    );
    assert_eq!(
        run_trim("fix: x\u{3000}"),
        msg(false, "header must not end with whitespace")
    );
    assert_eq!(
        run_trim("\u{feff}fix: x "),
        msg(false, "header must not be surrounded by whitespace")
    );
    assert!(run("header-trim", "fix: x", ALWAYS, RuleValue::Length(1)).is_err());
}

#[test]
fn breaking_change_markers() {
    let message = "breaking changes must have both an exclamation mark in the header and BREAKING CHANGE in the footer to identify a breaking change";
    let neg_message = message.replace("must have", "must not have");
    assert_eq!(
        ok(
            "breaking-change-exclamation-mark",
            "feat!: x\n\nBREAKING CHANGE: y",
            None,
            RuleValue::None
        ),
        msg(true, message)
    );
    assert_eq!(
        ok(
            "breaking-change-exclamation-mark",
            "feat!: x\n\nBREAKING-CHANGE: y",
            ALWAYS,
            RuleValue::None
        ),
        msg(true, message)
    );
    assert!(
        !ok(
            "breaking-change-exclamation-mark",
            "feat!: x",
            ALWAYS,
            RuleValue::None
        )
        .valid
    );
    assert!(
        !ok(
            "breaking-change-exclamation-mark",
            "feat: x\n\nBREAKING CHANGE: y",
            ALWAYS,
            RuleValue::None
        )
        .valid
    );
    // Neither marker present: equivalent, so `always` passes and `never` fails.
    assert!(
        ok(
            "breaking-change-exclamation-mark",
            "feat: x",
            ALWAYS,
            RuleValue::None
        )
        .valid
    );
    let never = ok(
        "breaking-change-exclamation-mark",
        "feat: x",
        NEVER,
        RuleValue::None,
    );
    assert!(!never.valid);
    assert_eq!(never.message.unwrap(), neg_message);
    // Both markers present with `never` fails as well: negation of equivalence, not XOR of presence.
    assert!(
        !ok(
            "breaking-change-exclamation-mark",
            "feat!: x\n\nBREAKING CHANGE: y",
            NEVER,
            RuleValue::None
        )
        .valid
    );
    assert!(
        ok(
            "breaking-change-exclamation-mark",
            "feat!: x",
            NEVER,
            RuleValue::None
        )
        .valid
    );
    // The footer marker is case sensitive and requires a colon.
    assert!(
        !ok(
            "breaking-change-exclamation-mark",
            "feat!: x\n\nbreaking change: y",
            ALWAYS,
            RuleValue::None
        )
        .valid
    );
    // A scoped bang header is recognized.
    assert!(
        ok(
            "breaking-change-exclamation-mark",
            "feat(core)!: x\n\nBREAKING CHANGE: y",
            ALWAYS,
            RuleValue::None
        )
        .valid
    );
}
