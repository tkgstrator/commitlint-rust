use commitlint_rust::{
    ParsedMessage, ParserPreset, RuleCondition, RuleValue, Severity, evaluate_rule, lint_detailed,
    lint_message, parse_message_with_comment_char,
};
use serde_json::Value;

fn compare_parsed(parsed: &ParsedMessage, expected: &Value, label: &str) {
    for (field, actual) in [
        ("header", &parsed.header),
        ("type", &parsed.r#type),
        ("scope", &parsed.scope),
        ("subject", &parsed.subject),
        ("body", &parsed.body),
        ("footer", &parsed.footer),
    ] {
        assert_eq!(
            actual.as_deref(),
            expected[field].as_str(),
            "{label} {field}"
        );
    }
}

#[test]
fn actual_upstream_rule_assertions() {
    let corpus: Value = serde_json::from_str(include_str!("fixtures/upstream-rules.json")).unwrap();
    let cases = corpus["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 86);
    for case in cases {
        let label = case["id"].as_str().unwrap();
        let raw = case["message"].as_str().unwrap();
        assert_eq!(case["parserPreset"], "angular");
        let comment = case["parserOptions"]["commentChar"].as_str().map(|s| {
            assert_eq!(s.chars().count(), 1);
            s.chars().next().unwrap()
        });
        let parsed = parse_message_with_comment_char(raw, ParserPreset::Angular, comment).unwrap();
        compare_parsed(&parsed, &case["parsed"], label);
        let when = match case["when"].as_str() {
            None => None,
            Some("always") => Some(RuleCondition::Always),
            Some("never") => Some(RuleCondition::Never),
            other => panic!("{label} {other:?}"),
        };
        let value = match &case["value"] {
            Value::Null => RuleValue::None,
            Value::Number(n) => RuleValue::Length(n.as_u64().unwrap() as usize),
            Value::String(s) => RuleValue::Text(s.clone()),
            Value::Array(a) => {
                RuleValue::List(a.iter().map(|v| v.as_str().unwrap().to_owned()).collect())
            }
            other => panic!("{label} {other:?}"),
        };
        let result =
            evaluate_rule(case["rule"].as_str().unwrap(), &parsed, raw, when, &value).unwrap();
        assert_eq!(
            result.valid,
            case["expected"]["valid"].as_bool().unwrap(),
            "{label}"
        );
        assert_eq!(
            result.message.as_deref(),
            case["expected"]["message"].as_str(),
            "{label} message"
        );
    }
}

#[test]
fn upstream_inputs_match_fixed_policy_diagnostics() {
    let corpus: Value =
        serde_json::from_str(include_str!("fixtures/upstream-input-policy.json")).unwrap();
    let cases = corpus["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 86);
    for case in cases {
        let label = case["id"].as_str().unwrap();
        let raw = case["message"].as_str().unwrap();
        let result = lint_detailed(raw.as_bytes());
        assert_eq!(result.valid, case["valid"].as_bool().unwrap(), "{label}");
        assert_eq!(
            result.valid,
            lint_message(raw.as_bytes()).is_ok(),
            "{label} legacy"
        );
        compare_parsed(result.parsed.as_ref().unwrap(), &case["parsed"], label);
        for (diagnostics, field, severity) in [
            (&result.errors, "errors", Severity::Error),
            (&result.warnings, "warnings", Severity::Warning),
        ] {
            let actual: Vec<_> = diagnostics.iter().map(|d| d.name.as_str()).collect();
            let expected: Vec<_> = case[field]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_str().unwrap())
                .collect();
            assert_eq!(actual, expected, "{label} {field}");
            assert!(diagnostics.iter().all(|d| d.severity == severity));
        }
    }
}

#[test]
fn rule_lengths_count_utf16_and_keep_url_exemptions() {
    // Independently checked against installed upstream rules with Node:
    // astral Unicode occupies two JS string code units; it remains refused by
    // our complete fixed policy, which enforces printable ASCII separately.
    for (raw, rule, limit, valid) in [
        ("test: 😀", "header-max-length", 7, false),
        ("test: 😀", "header-max-length", 8, true),
        ("test: a\n\n😀", "body-max-line-length", 1, false),
        ("test: a\n\n😀", "body-max-line-length", 2, true),
        ("test: a\n\nRefs: 😀", "footer-max-line-length", 7, false),
        ("test: a\n\nRefs: 😀", "footer-max-line-length", 8, true),
        (
            "test: a\n\nhttps://example.com/very-long",
            "body-max-line-length",
            1,
            true,
        ),
        (
            "test: a\n\nRefs: https://example.com/very-long",
            "footer-max-line-length",
            1,
            true,
        ),
    ] {
        let parsed = parse_message_with_comment_char(raw, ParserPreset::Angular, None).unwrap();
        let result = evaluate_rule(
            rule,
            &parsed,
            raw,
            Some(RuleCondition::Always),
            &RuleValue::Length(limit),
        )
        .unwrap();
        assert_eq!(result.valid, valid, "{rule} {raw:?} {limit}");
    }
}

#[test]
fn footer_warning_uses_raw_first_match_and_js_slice_semantics() {
    // Actual upstream indexOf finds the equal header at index zero, then
    // slice(-1) inspects the last raw line, including terminal line endings.
    for (raw, valid) in [
        ("fix: a\n\nfix: a", false),
        ("fix: a\n\nfix: a\n", true),
        ("fix: a\n\nfix: a\n\n", true),
        ("fix: a\n\nfix: a\n\r", false),
    ] {
        let parsed = parse_message_with_comment_char(raw, ParserPreset::Angular, None).unwrap();
        let result = evaluate_rule(
            "footer-leading-blank",
            &parsed,
            raw,
            Some(RuleCondition::Always),
            &RuleValue::None,
        )
        .unwrap();
        assert_eq!(result.valid, valid, "{raw:?}");
    }
}

#[test]
fn subject_full_stop_uses_js_final_utf16_unit() {
    let raw = "feat: hi😀";
    let parsed = parse_message_with_comment_char(raw, ParserPreset::Angular, None).unwrap();
    let result = evaluate_rule(
        "subject-full-stop",
        &parsed,
        raw,
        Some(RuleCondition::Always),
        &RuleValue::Text("😀".into()),
    )
    .unwrap();
    assert!(!result.valid);
}

#[test]
fn many_breaking_notes_parse_without_crashing() {
    let result = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--ignored", "--exact", "many_breaking_notes_child"])
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
}

#[test]
#[ignore = "subprocess fixture; exercised by many_breaking_notes_parse_without_crashing"]
fn many_breaking_notes_child() {
    let raw = format!(
        "fix: change behavior\n\n{}",
        "BREAKING CHANGE: preserve parser behavior\n".repeat(20_000)
    );
    let result = lint_detailed(raw.as_bytes());
    assert!(!result.valid);
    assert_eq!(result.valid, lint_message(raw.as_bytes()).is_ok());
    assert!(result.errors.iter().any(|d| d.name == "message-max-length"));
    assert_eq!(
        result.parsed.unwrap().footer.unwrap().lines().count(),
        20_000
    );
}
