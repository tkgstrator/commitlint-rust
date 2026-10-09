use super::*;

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
