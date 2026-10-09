use super::*;

#[test]
fn length_rules_use_utf16_units_and_default_zero() {
    // U+1F600 is two UTF-16 units.
    let raw = "fix(😀): 😀😀";
    assert_eq!(
        ok("subject-max-length", raw, None, RuleValue::Length(4)),
        msg(true, "subject must not be longer than 4 characters")
    );
    assert!(!ok("subject-max-length", raw, None, RuleValue::Length(3)).valid);
    assert!(ok("scope-min-length", raw, None, RuleValue::Length(2)).valid);
    assert!(!ok("scope-min-length", raw, None, RuleValue::Length(3)).valid);
    assert_eq!(
        ok("scope-min-length", raw, None, RuleValue::Length(3))
            .message
            .as_deref(),
        Some("scope must not be shorter than 3 characters")
    );
    // Missing value defaults to 0: any non-empty field is too long, never too short.
    assert!(!ok("type-max-length", "fix: x", None, RuleValue::None).valid);
    assert!(ok("type-min-length", "fix: x", None, RuleValue::None).valid);
    // Empty/missing fields pass without a message.
    assert_eq!(
        ok("scope-max-length", "fix: x", None, RuleValue::Length(0)),
        bare()
    );
    assert_eq!(
        ok("body-min-length", "fix: x", None, RuleValue::Length(10)),
        bare()
    );
    assert_eq!(
        ok("footer-max-length", "fix: x", None, RuleValue::Length(0)),
        bare()
    );
    assert_eq!(
        ok(
            "body-max-length",
            "fix: x\n\nabcdef",
            None,
            RuleValue::Length(5)
        ),
        msg(false, "body must not be longer than 5 characters")
    );
    assert_eq!(
        ok(
            "footer-min-length",
            "fix: x\n\nCloses #1",
            None,
            RuleValue::Length(100)
        ),
        msg(false, "footer must not be shorter than 100 characters")
    );
}

#[test]
fn header_length_reports_current_length_and_missing_header() {
    assert_eq!(
        ok("header-max-length", "fix: 😀", None, RuleValue::Length(5)),
        msg(
            false,
            "header must not be longer than 5 characters, current length is 7"
        )
    );
    assert_eq!(
        ok("header-min-length", "fix: x", None, RuleValue::Length(6)),
        msg(
            true,
            "header must not be shorter than 6 characters, current length is 6"
        )
    );
    // An empty header parses to `undefined`: both checks are false.
    let raw = "gpg: diagnostic";
    let parsed = parse_message(raw, ParserPreset::ConventionalCommits).unwrap();
    assert_eq!(parsed.header, None);
    for (name, word) in [
        ("header-max-length", "longer"),
        ("header-min-length", "shorter"),
    ] {
        let result = evaluate_rule(name, &parsed, raw, None, &RuleValue::Length(10)).unwrap();
        assert_eq!(
            result,
            msg(
                false,
                &format!(
                    "header must not be {word} than 10 characters, current length is undefined"
                )
            )
        );
    }
}

#[test]
fn numeric_limits_handle_floats_and_nonfinite_values() {
    let max = |n: f64| ok("header-max-length", "fix: x", NEVER, RuleValue::Number(n));
    // `when` is ignored for lengths.
    assert!(max(6.0).valid);
    assert!(!max(5.5).valid);
    assert_eq!(
        max(5.5).message.as_deref(),
        Some("header must not be longer than 5.5 characters, current length is 6")
    );
    assert!(max(f64::INFINITY).valid);
    assert_eq!(
        max(f64::INFINITY).message.as_deref(),
        Some("header must not be longer than Infinity characters, current length is 6")
    );
    assert!(!max(f64::NEG_INFINITY).valid);
    assert!(!max(-1.0).valid);
    assert_eq!(
        max(-1.0).message.as_deref(),
        Some("header must not be longer than -1 characters, current length is 6")
    );
    // NaN compares false either way and prints as NaN.
    assert!(!max(f64::NAN).valid);
    assert_eq!(
        max(f64::NAN).message.as_deref(),
        Some("header must not be longer than NaN characters, current length is 6")
    );
    let min = |n: f64| ok("header-min-length", "fix: x", ALWAYS, RuleValue::Number(n));
    assert!(min(6.0).valid);
    assert!(!min(6.5).valid);
    assert!(min(-1.0).valid);
    assert!(!min(f64::NAN).valid);
    assert!(min(f64::NEG_INFINITY).valid);
    assert_eq!(
        ok(
            "body-max-length",
            "fix: x\n\nabc",
            None,
            RuleValue::Number(2.5)
        ),
        msg(false, "body must not be longer than 2.5 characters")
    );
    assert!(
        !ok(
            "scope-min-length",
            "fix(ab): x",
            None,
            RuleValue::Number(f64::NAN)
        )
        .valid
    );
    // Exact large integers stay exact.
    assert_eq!(
        ok(
            "body-max-length",
            "fix: x\n\nabc",
            None,
            RuleValue::Length(usize::MAX)
        )
        .message
        .as_deref(),
        Some(format!("body must not be longer than {} characters", usize::MAX).as_str())
    );
    // Lengths ignore the condition entirely.
    for when in [None, ALWAYS, NEVER] {
        assert!(ok("subject-max-length", "fix: x", when, RuleValue::Length(1)).valid);
        assert!(!ok("subject-max-length", "fix: x", when, RuleValue::Length(0)).valid);
    }
}

#[test]
fn line_length_rules_exempt_urls() {
    let raw = "fix: x\n\nshort\nhttps://example.com/a/very/long/path/that/exceeds\nlong line here";
    let result = ok("body-max-line-length", raw, None, RuleValue::Length(12));
    assert!(!result.valid);
    assert_eq!(
        result.message.as_deref(),
        Some("body's lines must not be longer than 12 characters")
    );
    let urls = "fix: x\n\nshort\nhttps://example.com/a/very/long/path/that/exceeds";
    assert!(ok("body-max-line-length", urls, None, RuleValue::Length(12)).valid);
    let footer = "fix: x\n\nBREAKING CHANGE: a very long footer line indeed";
    assert_eq!(
        ok(
            "footer-max-line-length",
            footer,
            None,
            RuleValue::Length(10)
        ),
        msg(
            false,
            "footer's lines must not be longer than 10 characters"
        )
    );
    assert_eq!(
        ok(
            "footer-max-line-length",
            "fix: x",
            None,
            RuleValue::Length(1)
        ),
        bare()
    );
    assert_eq!(
        ok("body-max-line-length", "fix: x", None, RuleValue::Length(1)),
        bare()
    );
    assert!(
        !ok(
            "body-max-line-length",
            "fix: x\n\nabc",
            None,
            RuleValue::Number(f64::NAN)
        )
        .valid
    );
}
