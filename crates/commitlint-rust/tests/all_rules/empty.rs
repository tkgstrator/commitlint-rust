use super::*;

#[test]
fn empty_rules_defaults_and_messages() {
    assert_eq!(
        ok("body-empty", "fix: x", None, RuleValue::None),
        msg(true, "body must be empty")
    );
    assert_eq!(
        ok("body-empty", "fix: x\n\nbody", ALWAYS, RuleValue::None),
        msg(false, "body must be empty")
    );
    assert_eq!(
        ok("footer-empty", "fix: x", NEVER, RuleValue::None),
        msg(false, "footer may not be empty")
    );
    assert_eq!(
        ok(
            "footer-empty",
            "fix: x\n\nCloses #1",
            NEVER,
            RuleValue::None
        ),
        msg(true, "footer may not be empty")
    );
    // scope-empty defaults to `never`.
    assert_eq!(
        ok("scope-empty", "fix: x", None, RuleValue::None),
        msg(false, "scope may not be empty")
    );
    assert_eq!(
        ok("scope-empty", "fix(a): x", None, RuleValue::None),
        msg(true, "scope may not be empty")
    );
    assert_eq!(
        ok("scope-empty", "fix: x", ALWAYS, RuleValue::None),
        msg(true, "scope must be empty")
    );
    assert_eq!(
        ok("scope-empty", "fix(a): x", ALWAYS, RuleValue::None),
        msg(false, "scope must be empty")
    );
    assert_eq!(
        ok("scope-empty", "fix(a): x", NEVER, RuleValue::None),
        msg(true, "scope may not be empty")
    );
    // references-empty defaults to `never` as well.
    assert_eq!(
        ok("references-empty", "fix: x", None, RuleValue::None),
        msg(false, "references may not be empty")
    );
    assert_eq!(
        ok(
            "references-empty",
            "fix: x\n\nCloses #1",
            None,
            RuleValue::None
        ),
        msg(true, "references may not be empty")
    );
    assert_eq!(
        ok("references-empty", "fix: x", ALWAYS, RuleValue::None),
        msg(true, "references must be empty")
    );
    assert_eq!(
        ok(
            "references-empty",
            "fix: x\n\nCloses #1",
            ALWAYS,
            RuleValue::None
        ),
        msg(false, "references must be empty")
    );
    assert_eq!(
        ok("subject-empty", "fix: x", None, RuleValue::None),
        msg(false, "subject must be empty")
    );
    assert_eq!(
        ok("type-empty", "fix: x", NEVER, RuleValue::None),
        msg(true, "type may not be empty")
    );
}
