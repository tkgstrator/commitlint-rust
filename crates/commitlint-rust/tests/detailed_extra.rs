use commitlint_rust::{
    ParsedMessage, ParserPreset, RuleCondition, RuleValue, evaluate_rule, lint_detailed,
    lint_message, parse_message,
};

fn parsed(raw: &str) -> ParsedMessage {
    parse_message(raw, ParserPreset::Angular).unwrap()
}

fn error_names(input: &[u8]) -> Vec<String> {
    lint_detailed(input)
        .errors
        .into_iter()
        .map(|d| d.name)
        .collect()
}

#[test]
fn ordinal_subject_diagnostics_match_upstream_case_lists() {
    for (subject, expected) in [
        (
            "Add 4th Item",
            "subject must not be sentence-case, start-case",
        ),
        (
            "Fix4thItem",
            "subject must not be sentence-case, pascal-case",
        ),
        (
            "Add 1ST Item",
            "subject must not be sentence-case, start-case",
        ),
    ] {
        let message = format!("fix: {subject}");
        let outcome = lint_detailed(message.as_bytes());
        let case = outcome
            .errors
            .iter()
            .find(|d| d.name == "subject-case")
            .unwrap();
        assert_eq!(case.message, expected, "{subject}");
        assert!(!outcome.valid);
        assert_eq!(outcome.valid, lint_message(message.as_bytes()).is_ok());
    }
}

#[test]
fn invalid_utf8_has_no_parse_and_one_ascii_error() {
    let input = b"fix: caf\xff";
    let outcome = lint_detailed(input);
    assert!(!outcome.valid);
    assert!(outcome.parsed.is_none());
    assert_eq!(error_names(input), ["ascii-message"]);
    assert!(lint_message(input).is_err());
}

#[test]
fn non_ascii_and_control_bytes_are_ascii_errors_not_parser_changes() {
    for input in [
        "fix: caf\u{e9}",
        "fix: tab\there",
        "fix: a\r\n\r\nb",
        "fix: nul\0",
    ] {
        let outcome = lint_detailed(input.as_bytes());
        assert!(!outcome.valid, "{input:?}");
        assert!(error_names(input.as_bytes()).contains(&"ascii-message".to_owned()));
        assert_eq!(outcome.valid, lint_message(input.as_bytes()).is_ok());
    }
}

#[test]
fn whole_message_limit_is_not_exempted_by_urls() {
    let url = format!("https://example.com/{}", "x".repeat(140));
    let input = format!("fix: change\n\n{url}");
    let outcome = lint_detailed(input.as_bytes());
    // Per-line rules exempt the URL; the whole 128-character rule does not.
    assert_eq!(error_names(input.as_bytes()), ["message-max-length"]);
    assert!(!outcome.valid);
    assert!(lint_message(input.as_bytes()).is_err());
    // Exactly 128 characters (trailing LFs excluded) is accepted.
    let ok = format!("fix: {}\n\n", "x".repeat(123));
    assert_eq!(ok.trim_end_matches('\n').len(), 128);
    assert!(lint_detailed(ok.as_bytes()).valid);
    assert!(lint_message(ok.as_bytes()).is_ok());
    let long = format!("fix: {}", "x".repeat(124));
    assert_eq!(
        error_names(long.as_bytes()),
        ["header-max-length", "message-max-length"]
    );
}

#[test]
fn leading_blank_warnings_use_raw_lines() {
    // Body directly after header and footer directly after body.
    let raw = "fix: a\nbody\nCloses #1";
    let outcome = lint_detailed(raw.as_bytes());
    assert!(outcome.valid);
    let warned: Vec<_> = outcome.warnings.iter().map(|d| d.name.as_str()).collect();
    assert_eq!(warned, ["body-leading-blank", "footer-leading-blank"]);
    // Blank line before an immediate footer yields an empty (Some("")) body.
    let p = parse_message("fix: a\n\nCloses #1", ParserPreset::ConventionalCommits).unwrap();
    assert_eq!(p.body.as_deref(), Some(""));
    assert_eq!(p.footer.as_deref(), Some("Closes #1"));
    let p = parse_message("fix: a\nCloses #1", ParserPreset::ConventionalCommits).unwrap();
    assert_eq!(p.body, None);
    assert_eq!(p.footer.as_deref(), Some("Closes #1"));
    assert!(lint_detailed(b"fix: a\n\nCloses #1").warnings.is_empty());
}

#[test]
fn parser_trims_outer_newlines_and_drops_gpg_lines() {
    let p = parse_message(
        "\n\ngpg: note\nfix(api)!: change\r\n\r\nbody\n\n",
        ParserPreset::ConventionalCommits,
    )
    .unwrap();
    assert_eq!(p.header.as_deref(), Some("fix(api)!: change"));
    assert_eq!(p.scope.as_deref(), Some("api"));
    assert_eq!(p.body.as_deref(), Some("body"));
    // Angular has no optional bang.
    let a = parse_message("fix!: change", ParserPreset::Angular).unwrap();
    assert_eq!((a.r#type, a.subject), (None, None));
    assert!(parse_message("", ParserPreset::Angular).is_err());
    assert!(parse_message("\n \n", ParserPreset::Angular).is_err());
}

#[test]
fn unknown_and_incompatible_rules_fail_explicitly() {
    let p = parsed("fix: a");
    let none = RuleValue::None;
    assert!(evaluate_rule("unknown-rule", &p, "fix: a", None, &none).is_err());
    assert!(evaluate_rule("", &p, "fix: a", None, &none).is_err());
    assert!(evaluate_rule("type-enum", &p, "fix: a", None, &RuleValue::Length(1)).is_err());
    assert!(
        evaluate_rule(
            "header-max-length",
            &p,
            "fix: a",
            None,
            &RuleValue::Text("1".into())
        )
        .is_err()
    );
    assert!(
        evaluate_rule(
            "subject-full-stop",
            &p,
            "fix: a",
            None,
            &RuleValue::List(vec![])
        )
        .is_err()
    );
    assert!(evaluate_rule("type-empty", &p, "fix: a", None, &RuleValue::Length(1)).is_err());
}

#[test]
fn defaults_and_when_semantics() {
    let p = parsed("fix: a");
    let always = Some(RuleCondition::Always);
    let never = Some(RuleCondition::Never);
    // Default `when` is "always" for the empty rules; maxline ignores it.
    assert!(
        !evaluate_rule("type-empty", &p, "fix: a", None, &RuleValue::None)
            .unwrap()
            .valid
    );
    assert!(
        evaluate_rule("type-empty", &p, "fix: a", never, &RuleValue::None)
            .unwrap()
            .valid
    );
    let max = RuleValue::Length(5);
    let a = evaluate_rule("header-max-length", &p, "fix: a", always, &max).unwrap();
    let n = evaluate_rule("header-max-length", &p, "fix: a", never, &max).unwrap();
    assert_eq!(a, n);
    assert!(!a.valid);
    // Missing value defaults to 0 / [] / ".".
    assert!(
        !evaluate_rule("header-max-length", &p, "fix: a", None, &RuleValue::None)
            .unwrap()
            .valid
    );
    assert!(
        !evaluate_rule("type-enum", &p, "fix: a", None, &RuleValue::None)
            .unwrap()
            .valid
    );
    let dotted = parsed("fix: a.");
    assert!(
        evaluate_rule(
            "subject-full-stop",
            &dotted,
            "fix: a.",
            always,
            &RuleValue::None
        )
        .unwrap()
        .valid
    );
}
