use super::*;

fn scope_cases(items: &[(&str, Option<RuleCondition>)], delimiters: &[&str]) -> RuleValue {
    RuleValue::ScopeCases {
        cases: checks(items),
        delimiters: strings(delimiters),
    }
}

#[test]
fn scope_case_segments_and_delimiters() {
    let lower = RuleValue::Text("lower-case".into());
    assert_eq!(
        ok("scope-case", "fix(a/b): x", None, lower.clone()),
        msg(true, "scope must be lower-case")
    );
    assert!(!ok("scope-case", "fix(a/B): x", None, lower.clone()).valid);
    assert!(!ok("scope-case", "fix(a\\B): x", None, lower.clone()).valid);
    assert!(!ok("scope-case", "fix(a,B): x", None, lower.clone()).valid);
    // `, ?` consumes one optional ASCII space.
    assert!(!ok("scope-case", "fix(a, B): x", None, lower.clone()).valid);
    assert!(ok("scope-case", "fix(a, b): x", None, lower.clone()).valid);
    // A second space stays in the next segment, which then is not lower-case-trimmed-equal.
    assert!(ok("scope-case", "fix(a,  b): x", None, lower.clone()).valid);
    assert_eq!(ok("scope-case", "fix: x", None, lower.clone()), bare());
    // Explicit custom delimiters replace the defaults.
    let custom = scope_cases(&[("lower-case", None)], &["|"]);
    assert!(!ok("scope-case", "fix(a|B): x", None, custom.clone()).valid);
    assert!(!ok("scope-case", "fix(a/B): x", None, custom.clone()).valid);
    // Metacharacters are literal; multi-character delimiters match in order.
    let multi = scope_cases(&[("lower-case", None)], &["::", "."]);
    assert!(ok("scope-case", "fix(a::b.c): x", None, multi.clone()).valid);
    assert!(!ok("scope-case", "fix(a::B.c): x", None, multi).valid);
    // Empty delimiter list means the defaults.
    let defaults = scope_cases(&[("lower-case", None)], &[]);
    assert!(!ok("scope-case", "fix(a/B): x", None, defaults).valid);
    // Object checks may carry per-check conditions; never reports matches.
    let never = scope_cases(&[("upper-case", None), ("lower-case", None)], &[]);
    assert_eq!(
        ok("scope-case", "fix(a/b): x", NEVER, never),
        msg(false, "scope must not be lower-case")
    );
}

#[test]
fn scope_case_empty_delimiter_splits_utf16_units() {
    // JS "ab".split(/(?:)/) splits every UTF-16 unit; astral letters split into
    // surrogate halves; the empty pattern also matches each segment (skipped).
    let value = scope_cases(&[("lower-case", None)], &[""]);
    assert!(ok("scope-case", "fix(ab): x", None, value.clone()).valid);
    // An empty delimiter matches every segment, so each segment is skipped.
    assert!(ok("scope-case", "fix(aB): x", None, value.clone()).valid);
    let astral = ok("scope-case", "fix(a😀): x", None, value);
    assert!(astral.valid);
}

#[test]
fn scope_enum_whole_string_or_all_segments() {
    let list = |items: &[&str]| RuleValue::List(strings(items));
    let enumerated = list(&["a", "b", "a/b"]);
    assert_eq!(
        ok("scope-enum", "fix(a): x", None, enumerated.clone()),
        msg(true, "scope must be one of [a, b, a/b]")
    );
    // All segments allowed.
    assert!(ok("scope-enum", "fix(a,b): x", ALWAYS, list(&["a", "b"])).valid);
    assert!(ok("scope-enum", "fix(a, b): x", ALWAYS, list(&["a", "b"])).valid);
    assert!(!ok("scope-enum", "fix(a/c): x", ALWAYS, list(&["a", "b"])).valid);
    // The whole string alone is enough.
    assert!(ok("scope-enum", "fix(a/c): x", ALWAYS, list(&["a/c"])).valid);
    // never: no segment and not the whole string may be allowed.
    assert_eq!(
        ok("scope-enum", "fix(a/c): x", NEVER, list(&["a", "b"])),
        msg(false, "scope must not be one of [a, b]")
    );
    assert!(ok("scope-enum", "fix(c/d): x", NEVER, list(&["a", "b"])).valid);
    // Whole string allowed also fails `never` even when no segment is.
    assert!(!ok("scope-enum", "fix(a/c): x", NEVER, list(&["a/c"])).valid);
    // Empty scope or empty enum pass with an empty message.
    assert_eq!(
        ok("scope-enum", "fix: x", ALWAYS, list(&["a"])),
        msg(true, "")
    );
    assert_eq!(
        ok("scope-enum", "fix(a): x", ALWAYS, RuleValue::None),
        msg(true, "")
    );
    assert_eq!(
        ok("scope-enum", "fix(a): x", NEVER, list(&[])),
        msg(true, "")
    );
    // Object form with custom delimiters.
    let object = RuleValue::ScopeEnum {
        scopes: strings(&["a", "b"]),
        delimiters: strings(&["|"]),
    };
    assert!(ok("scope-enum", "fix(a|b): x", ALWAYS, object.clone()).valid);
    assert!(!ok("scope-enum", "fix(a/b): x", ALWAYS, object).valid);
    let defaults = RuleValue::ScopeEnum {
        scopes: strings(&["a", "b"]),
        delimiters: vec![],
    };
    assert!(ok("scope-enum", "fix(a/b): x", ALWAYS, defaults).valid);
}

#[test]
fn scope_delimiter_style_rule() {
    let style = |items: &[&str]| RuleValue::List(strings(items));
    assert_eq!(
        ok(
            "scope-delimiter-style",
            "fix(a/b): x",
            None,
            RuleValue::None
        ),
        msg(true, "scope delimiters must be one of [/, \\, ,]")
    );
    assert!(
        ok(
            "scope-delimiter-style",
            "fix(a, b): x",
            ALWAYS,
            RuleValue::None
        )
        .valid
    );
    assert!(
        !ok(
            "scope-delimiter-style",
            "fix(a|b): x",
            ALWAYS,
            RuleValue::None
        )
        .valid
    );
    assert!(
        !ok(
            "scope-delimiter-style",
            "fix(a; b): x",
            ALWAYS,
            RuleValue::None
        )
        .valid
    );
    // `-` and `_` are word characters; underscores/hyphens are not delimiters.
    assert!(
        ok(
            "scope-delimiter-style",
            "fix(a-b_c): x",
            ALWAYS,
            RuleValue::None
        )
        .valid
    );
    // never inverts "all allowed", so no delimiters at all fails it.
    assert_eq!(
        ok(
            "scope-delimiter-style",
            "fix(ab): x",
            NEVER,
            RuleValue::None
        ),
        msg(false, "scope delimiters must not be one of [/, \\, ,]")
    );
    assert!(
        ok(
            "scope-delimiter-style",
            "fix(a|b): x",
            NEVER,
            RuleValue::None
        )
        .valid
    );
    assert!(
        ok(
            "scope-delimiter-style",
            "fix(a|b|c): x",
            ALWAYS,
            style(&["|"])
        )
        .valid
    );
    assert_eq!(
        ok(
            "scope-delimiter-style",
            "fix(a/b): x",
            ALWAYS,
            style(&["|", ":"])
        ),
        msg(false, "scope delimiters must be one of [|, :]")
    );
    // A run is a single delimiter: ", " normalizes to ",", but "//" does not equal "/".
    assert!(
        !ok(
            "scope-delimiter-style",
            "fix(a//b): x",
            ALWAYS,
            RuleValue::None
        )
        .valid
    );
    assert!(
        ok(
            "scope-delimiter-style",
            "fix(a ,b): x",
            ALWAYS,
            RuleValue::None
        )
        .valid
    );
    assert_eq!(
        ok("scope-delimiter-style", "fix: x", ALWAYS, RuleValue::None),
        bare()
    );
}
