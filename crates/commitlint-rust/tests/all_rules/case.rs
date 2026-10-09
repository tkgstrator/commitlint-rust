use super::*;

#[test]
fn type_enum_and_aliases() {
    let list = RuleValue::List(strings(&["feat", "fix"]));
    assert_eq!(
        ok("type-enum", "feat: x", None, list.clone()),
        msg(true, "type must be one of [feat, fix]")
    );
    assert_eq!(
        ok("type-enum", "docs: x", ALWAYS, list.clone()),
        msg(false, "type must be one of [feat, fix]")
    );
    assert_eq!(
        ok("type-enum", "feat: x", NEVER, list.clone()),
        msg(false, "type must not be one of [feat, fix]")
    );
    assert_eq!(ok("type-enum", "missing", NEVER, list), bare());
    // Missing value is an empty list.
    assert!(!ok("type-enum", "feat: x", None, RuleValue::None).valid);
}

#[test]
fn case_rules_aliases_conditions_and_reporting() {
    let alias = |t: &str| RuleValue::Text(t.into());
    assert_eq!(
        ok("type-case", "feat: x", None, alias("lower-case")),
        msg(true, "type must be lower-case")
    );
    assert_eq!(
        ok("type-case", "feat: x", None, alias("lowercase")),
        msg(true, "type must be lowercase")
    );
    assert_eq!(
        ok("type-case", "feat: x", None, alias("lowerCase")),
        msg(true, "type must be lowerCase")
    );
    assert!(!ok("type-case", "FEAT: x", None, alias("lower-case")).valid);
    assert!(ok("type-case", "FEAT: x", None, alias("uppercase")).valid);
    assert!(ok("type-case", "feat: x", NEVER, alias("upper-case")).valid);
    assert_eq!(
        ok("type-case", "FEAT: x", NEVER, alias("upper-case")),
        msg(false, "type must not be upper-case")
    );
    // No cases configured: no check matches, so `always` fails and `never` passes.
    assert_eq!(
        ok("type-case", "feat: x", ALWAYS, RuleValue::None),
        msg(false, "type must be ")
    );
    assert!(ok("type-case", "feat: x", NEVER, RuleValue::None).valid);
    // Missing type passes bare.
    assert_eq!(
        ok("type-case", "no header format", ALWAYS, alias("lower-case")),
        bare()
    );

    // Per-check `when` inverts the check before the outer condition.
    let per_check = RuleValue::CaseChecks(checks(&[("upper-case", NEVER), ("kebab-case", None)]));
    assert_eq!(
        ok("type-case", "feat: x", ALWAYS, per_check.clone()),
        msg(true, "type must be upper-case, kebab-case")
    );
    // `never` reports only the matched checks.
    assert_eq!(
        ok("type-case", "feat: x", NEVER, per_check),
        msg(false, "type must not be upper-case, kebab-case")
    );
    let list = RuleValue::List(strings(&["sentence-case", "upper-case"]));
    assert_eq!(
        ok("subject-case", "feat: Hello world", NEVER, list.clone()),
        msg(false, "subject must not be sentence-case")
    );
    assert_eq!(
        ok("subject-case", "feat: hello world", ALWAYS, list),
        msg(false, "subject must be sentence-case, upper-case")
    );
}

#[test]
fn per_field_case_gates() {
    // body-case has no initial-letter gate; empty body passes bare.
    assert_eq!(
        ok(
            "body-case",
            "fix: x\n\n1 body",
            ALWAYS,
            RuleValue::Text("lower-case".into())
        ),
        msg(true, "body must be lower-case")
    );
    assert!(
        !ok(
            "body-case",
            "fix: x\n\nBody",
            ALWAYS,
            RuleValue::Text("lower-case".into())
        )
        .valid
    );
    assert_eq!(
        ok(
            "body-case",
            "fix: x",
            ALWAYS,
            RuleValue::Text("lower-case".into())
        ),
        bare()
    );
    // header-case only evaluates when the header starts with an ASCII letter.
    assert_eq!(
        ok(
            "header-case",
            "1fix: x",
            ALWAYS,
            RuleValue::Text("lower-case".into())
        ),
        bare()
    );
    assert_eq!(
        ok(
            "header-case",
            "Éfix: x",
            ALWAYS,
            RuleValue::Text("lower-case".into())
        ),
        bare()
    );
    assert!(
        !ok(
            "header-case",
            "Fix: x",
            ALWAYS,
            RuleValue::Text("lower-case".into())
        )
        .valid
    );
    assert_eq!(
        ok(
            "header-case",
            "fix: x",
            ALWAYS,
            RuleValue::Text("lower-case".into())
        ),
        msg(true, "header must be lower-case")
    );
    // subject-case gate is a cased Unicode letter, not just ASCII.
    assert!(
        !ok(
            "subject-case",
            "fix: élan",
            ALWAYS,
            RuleValue::Text("upper-case".into())
        )
        .valid
    );
    assert_eq!(
        ok(
            "subject-case",
            "fix: 1 thing",
            ALWAYS,
            RuleValue::Text("upper-case".into())
        ),
        bare()
    );
    assert_eq!(
        ok(
            "subject-case",
            "fix: 漢字",
            ALWAYS,
            RuleValue::Text("upper-case".into())
        ),
        bare()
    );
    assert_eq!(
        ok(
            "subject-case",
            "fix: _x",
            ALWAYS,
            RuleValue::Text("upper-case".into())
        ),
        bare()
    );
}
