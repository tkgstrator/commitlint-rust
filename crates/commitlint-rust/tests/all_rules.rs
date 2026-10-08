use commitlint_rust::rules::{
    CaseCheck, EvaluationContext, SUPPORTED_RULES, evaluate_rule_with_context, validate_rule_value,
};
use commitlint_rust::{
    ParserPreset, RuleCondition, RuleOutcome, RuleValue, evaluate_rule, parse_message,
};
use std::path::PathBuf;
#[cfg(unix)]
mod common;

const ALWAYS: Option<RuleCondition> = Some(RuleCondition::Always);
const NEVER: Option<RuleCondition> = Some(RuleCondition::Never);

fn run(
    name: &str,
    raw: &str,
    when: Option<RuleCondition>,
    value: RuleValue,
) -> Result<RuleOutcome, String> {
    let parsed = parse_message(raw, ParserPreset::ConventionalCommits).unwrap();
    evaluate_rule(name, &parsed, raw, when, &value)
}

fn ok(name: &str, raw: &str, when: Option<RuleCondition>, value: RuleValue) -> RuleOutcome {
    run(name, raw, when, value).unwrap()
}

fn msg(valid: bool, text: &str) -> RuleOutcome {
    RuleOutcome {
        valid,
        message: Some(text.into()),
    }
}

fn bare() -> RuleOutcome {
    RuleOutcome {
        valid: true,
        message: None,
    }
}

fn strings(items: &[&str]) -> Vec<String> {
    items.iter().map(|s| (*s).to_owned()).collect()
}

fn checks(items: &[(&str, Option<RuleCondition>)]) -> Vec<CaseCheck> {
    items
        .iter()
        .map(|(target, when)| CaseCheck {
            target: (*target).into(),
            when: *when,
        })
        .collect()
}

#[test]
fn additional_body_empty_rule_is_available() {
    let raw = "fix: change behavior\n\nbody text";
    let parsed = parse_message(raw, ParserPreset::ConventionalCommits).unwrap();
    let result = evaluate_rule(
        "body-empty",
        &parsed,
        raw,
        Some(RuleCondition::Never),
        &RuleValue::None,
    )
    .expect("upstream body-empty must be supported");
    assert!(result.valid);
    assert_eq!(result.message.as_deref(), Some("body may not be empty"));
}

#[test]
fn registry_lists_exactly_the_38_upstream_names() {
    assert_eq!(SUPPORTED_RULES.len(), 38);
    let mut sorted = SUPPORTED_RULES.to_vec();
    sorted.sort();
    sorted.dedup();
    assert_eq!(sorted.len(), 38);
    // Names verified independently against @commitlint/rules src/index.ts.
    for name in [
        "body-case",
        "body-empty",
        "body-full-stop",
        "body-leading-blank",
        "body-max-length",
        "body-max-line-length",
        "body-min-length",
        "breaking-change-exclamation-mark",
        "footer-empty",
        "footer-leading-blank",
        "footer-max-length",
        "footer-max-line-length",
        "footer-min-length",
        "header-case",
        "header-full-stop",
        "header-max-length",
        "header-min-length",
        "header-trim",
        "references-empty",
        "scope-case",
        "scope-delimiter-style",
        "scope-empty",
        "scope-enum",
        "scope-max-length",
        "scope-min-length",
        "signed-off-by",
        "subject-case",
        "subject-empty",
        "subject-exclamation-mark",
        "subject-full-stop",
        "subject-max-length",
        "subject-min-length",
        "trailer-exists",
        "type-case",
        "type-empty",
        "type-enum",
        "type-max-length",
        "type-min-length",
    ] {
        assert!(SUPPORTED_RULES.contains(&name), "{name}");
    }
}

#[test]
fn every_name_evaluates_with_no_value_and_default_condition() {
    let raw = "feat(core): add thing\n\nbody text\n\nCloses #1";
    let parsed = parse_message(raw, ParserPreset::ConventionalCommits).unwrap();
    let context = EvaluationContext::default();
    for name in SUPPORTED_RULES {
        if name == "trailer-exists" {
            continue; // requires Git
        }
        evaluate_rule_with_context(name, &parsed, raw, None, &RuleValue::None, &context)
            .unwrap_or_else(|e| panic!("{name}: {e}"));
    }
}

#[test]
fn unknown_rules_and_wrong_value_types_are_errors() {
    assert!(run("no-such-rule", "feat: x", ALWAYS, RuleValue::None).is_err());
    assert!(validate_rule_value("no-such-rule", &RuleValue::None).is_err());
    assert!(run("type-empty", "feat: x", ALWAYS, RuleValue::Length(1)).is_err());
    assert!(
        run(
            "header-max-length",
            "feat: x",
            ALWAYS,
            RuleValue::Text("1".into())
        )
        .is_err()
    );
    assert!(
        run(
            "type-enum",
            "feat: x",
            ALWAYS,
            RuleValue::Text("feat".into())
        )
        .is_err()
    );
    assert!(run("signed-off-by", "feat: x", ALWAYS, RuleValue::Length(1)).is_err());
    assert!(run("scope-enum", "feat(a): x", ALWAYS, RuleValue::Length(1)).is_err());
    assert!(run("type-case", "feat: x", ALWAYS, RuleValue::Length(1)).is_err());
    // Object-based values are scope-only.
    let object = RuleValue::ScopeCases {
        cases: checks(&[("lower-case", None)]),
        delimiters: vec![],
    };
    assert!(run("type-case", "feat: x", ALWAYS, object).is_err());
    assert!(
        run(
            "type-case",
            "feat: x",
            ALWAYS,
            RuleValue::ScopeEnum {
                scopes: vec![],
                delimiters: vec![]
            }
        )
        .is_err()
    );
}

#[test]
fn unknown_case_targets_error_before_message_data_is_inspected() {
    // No body: would otherwise short-circuit to a bare pass.
    let bad = RuleValue::Text("shouty-case".into());
    for name in [
        "body-case",
        "header-case",
        "subject-case",
        "type-case",
        "scope-case",
    ] {
        assert!(run(name, "fix: x", ALWAYS, bad.clone()).is_err(), "{name}");
        assert!(validate_rule_value(name, &bad).is_err(), "{name}");
    }
    let listed = RuleValue::CaseChecks(checks(&[("lower-case", None), ("nope", None)]));
    assert!(run("type-case", "fix: x", ALWAYS, listed).is_err());
    assert!(validate_rule_value("type-case", &RuleValue::Text("kebab-case".into())).is_ok());
}

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

#[cfg(unix)]
#[test]
fn full_upstream_rule_calls_match_exact_outcomes() {
    use commitlint_rust::parser::{
        ParsedMessage, ParserOptions, Reference, parse_message_with_options,
    };
    use serde_json::Value;
    use std::collections::HashSet;
    let fixture = common::Fixture::new();
    let git = fixture.root.join("isolated oracle git");
    // Scope the reference oracle's Git configuration to this child only.
    // Never change the test process's environment or the user's configuration.
    common::executable(
        &git,
        &format!(
            "#!/bin/sh\nexec /usr/bin/env -i PATH=/usr/bin:/bin GIT_CONFIG_NOSYSTEM=1 GIT_CONFIG_GLOBAL=/dev/null '{}' \"$@\"\n",
            fixture.git.display()
        ),
    );
    let context = EvaluationContext {
        git: Some(git),
        cwd: Some(fixture.root.clone()),
        ..EvaluationContext::default()
    };
    let corpus: Value =
        serde_json::from_str(include_str!("fixtures/full-upstream-rules.json")).unwrap();
    assert_eq!(corpus["schema"], 3);
    let cases = corpus["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 532);
    fn strings(value: &Value) -> Vec<String> {
        value
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap().to_owned())
            .collect()
    }
    fn checks(value: &Value) -> Vec<CaseCheck> {
        value
            .as_array()
            .unwrap()
            .iter()
            .map(|v| {
                if let Some(target) = v.as_str() {
                    CaseCheck {
                        target: target.into(),
                        when: None,
                    }
                } else {
                    CaseCheck {
                        target: v["case"].as_str().unwrap().into(),
                        when: match v["when"].as_str() {
                            None => None,
                            Some("always") => ALWAYS,
                            Some("never") => NEVER,
                            other => panic!("unsupported inner condition {other:?}"),
                        },
                    }
                }
            })
            .collect()
    }
    fn typed(tag: &Value) -> RuleValue {
        if tag["kind"] == "undefined" {
            return RuleValue::None;
        }
        assert_eq!(tag["kind"], "json");
        match &tag["value"] {
            Value::Number(n) => RuleValue::Number(n.as_f64().unwrap()),
            Value::String(s) => RuleValue::Text(s.clone()),
            Value::Array(a) if a.iter().all(Value::is_string) => {
                RuleValue::List(strings(&tag["value"]))
            }
            Value::Array(_) => RuleValue::CaseChecks(checks(&tag["value"])),
            Value::Object(o) if o.contains_key("scopes") => RuleValue::ScopeEnum {
                scopes: strings(&o["scopes"]),
                delimiters: o.get("delimiters").map(strings).unwrap_or_default(),
            },
            Value::Object(o) if o.contains_key("cases") => RuleValue::ScopeCases {
                cases: checks(&o["cases"]),
                delimiters: o.get("delimiters").map(strings).unwrap_or_default(),
            },
            other => panic!("unsupported fixture value {other:?}"),
        }
    }
    fn optional(v: &Value) -> Option<String> {
        v.as_str().map(str::to_owned)
    }
    fn provided(v: &Value) -> ParsedMessage {
        ParsedMessage {
            header: optional(&v["header"]),
            r#type: optional(&v["type"]),
            scope: optional(&v["scope"]),
            subject: optional(&v["subject"]),
            body: optional(&v["body"]),
            footer: optional(&v["footer"]),
            references: v["references"]
                .as_array()
                .unwrap()
                .iter()
                .map(|r| Reference {
                    action: optional(&r["action"]),
                    owner: optional(&r["owner"]),
                    repository: optional(&r["repository"]),
                    issue: r["issue"].as_str().unwrap().into(),
                    prefix: r["prefix"].as_str().unwrap().into(),
                    raw: r["raw"].as_str().unwrap().into(),
                })
                .collect(),
        }
    }
    let mut names = HashSet::new();
    let mut custom_parser_cases = 0;
    let mut trailers = 0;
    for case in cases {
        let id = case["id"].as_str().unwrap();
        let name = case["rule"].as_str().unwrap();
        let raw = case["parsed"]["raw"].as_str().unwrap();
        assert_eq!(raw, case["message"].as_str().unwrap(), "{id} original raw");
        names.insert(name);
        let when = if case["when"]["kind"] == "undefined" {
            None
        } else {
            assert_eq!(case["when"]["kind"], "json");
            match case["when"]["value"].as_str() {
                Some("always") => ALWAYS,
                Some("never") => NEVER,
                other => panic!("{id} unsupported condition {other:?}"),
            }
        };
        let mut options = ParserOptions::default();
        let settings = &case["parserOptions"]["value"];
        let custom = settings["headerPattern"]["source"] == "^(.*): (.*)$";
        let parsed = if custom {
            assert_eq!(name, "type-case");
            assert_eq!(
                settings["headerCorrespondence"],
                serde_json::json!(["type", "subject"])
            );
            custom_parser_cases += 1;
            provided(&case["parsed"])
        } else {
            if let Some(c) = settings["commentChar"].as_str() {
                assert_eq!(c.chars().count(), 1);
                options.comment_char = c.chars().next();
            }
            if settings["issuePrefixes"].is_array() {
                options.issue_prefixes = strings(&settings["issuePrefixes"]);
            }
            if let Some(s) = settings["issuePrefixesCaseSensitive"].as_bool() {
                options.issue_prefixes_case_sensitive = s;
            }
            if settings["referenceActions"].is_array() {
                options.reference_actions = strings(&settings["referenceActions"]);
            }
            let actual = parse_message_with_options(raw, &options).unwrap();
            assert_eq!(
                actual,
                provided(&case["parsed"]),
                "{id} independent native parse"
            );
            actual
        };
        let result =
            evaluate_rule_with_context(name, &parsed, raw, when, &typed(&case["value"]), &context)
                .unwrap_or_else(|e| panic!("{id} {name}: {e}"));
        assert_eq!(
            result.valid,
            case["outcome"]["valid"].as_bool().unwrap(),
            "{id} {name}"
        );
        let expected_message = if case["outcome"]["message"]["kind"] == "undefined" {
            None
        } else {
            assert_eq!(case["outcome"]["message"]["kind"], "json");
            Some(case["outcome"]["message"]["value"].as_str().unwrap())
        };
        assert_eq!(
            result.message.as_deref(),
            expected_message,
            "{id} {name} exact diagnostic"
        );
        if name == "trailer-exists" {
            trailers += 1;
        }
    }
    assert_eq!(names, SUPPORTED_RULES.into_iter().collect::<HashSet<_>>());
    assert_eq!(custom_parser_cases, 7);
    assert_eq!(trailers, 11);
    println!(
        "verified 532 original calls across 38 rules, including 7 provided custom parses and 11 isolated Git trailer calls"
    );
}

#[test]
fn original_upstream_assertions_and_coverage_remain_verified() {
    use serde_json::Value;
    let corpus: Value =
        serde_json::from_str(include_str!("fixtures/full-upstream-rules.json")).unwrap();
    let tests = corpus["tests"].as_array().unwrap();
    let cases = corpus["cases"].as_array().unwrap();
    assert_eq!(tests.len(), 535);
    assert_eq!(cases.len(), 532);
    assert_eq!(corpus["coverage"]["functionalTests"], 532);
    assert_eq!(corpus["coverage"]["metaTests"], 3);
    assert_eq!(corpus["coverage"]["ruleCalls"], 532);
    assert_eq!(corpus["coverage"]["assertions"], 632);
    let mut functional = 0;
    let mut meta = 0;
    let mut assertions = 0;
    let mut rule_assertions = 0;
    for test in tests {
        let label = test["id"].as_str().unwrap();
        assert_eq!(test["passed"], true, "{label} original test failed");
        if test["callCount"] == 0 {
            meta += 1;
        } else {
            assert_eq!(test["callCount"], 1);
            functional += 1;
        }
        for assertion in test["assertions"].as_array().unwrap() {
            let actual = &assertion["actual"];
            let expected = &assertion["expected"];
            let pass = match assertion["matcher"].as_str().unwrap() {
                "toEqual" if expected["value"]["$type"] == "arrayContaining" => {
                    assert_eq!(actual["kind"], "json");
                    let actual = actual["value"].as_array().unwrap();
                    expected["value"]["value"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .all(|value| actual.contains(value))
                }
                "toEqual" | "toBe" => actual == expected,
                "toBeTruthy" | "toBeFalsy" => {
                    let truthy = actual["kind"] != "undefined"
                        && match &actual["value"] {
                            Value::Null => false,
                            Value::Bool(v) => *v,
                            Value::String(v) => !v.is_empty(),
                            Value::Number(v) => v.as_f64().unwrap() != 0.0,
                            Value::Array(_) | Value::Object(_) => true,
                        };
                    truthy == (assertion["matcher"] == "toBeTruthy")
                }
                "toContain" => {
                    assert_eq!(actual["kind"], "json");
                    assert_eq!(expected["kind"], "json");
                    match &actual["value"] {
                        Value::String(v) => v.contains(expected["value"].as_str().unwrap()),
                        Value::Array(v) => v.contains(&expected["value"]),
                        other => panic!("{label} unsupported toContain input {other:?}"),
                    }
                }
                other => panic!("{label} unsupported original matcher {other}"),
            };
            assert!(pass, "{label} original assertion {assertion}");
            assertions += 1;
            if test["callCount"] != 0 {
                rule_assertions += 1;
            }
        }
    }
    assert_eq!(
        (functional, meta, assertions, rule_assertions),
        (532, 3, 632, 629)
    );
    assert_eq!(
        cases
            .iter()
            .map(|case| case["assertions"].as_array().unwrap().len())
            .sum::<usize>(),
        629
    );
}
