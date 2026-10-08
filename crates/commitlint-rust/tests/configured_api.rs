use commitlint_rust::configured::is_default_ignored;
use commitlint_rust::rules::EvaluationContext;
use commitlint_rust::{lint_configured, lint_message, parse_json_config};

#[test]
fn default_ignore_matches_upstream_merge_and_version_examples() {
    for input in [
        "Merge pull request #369",
        "Merge branch 'main'\r\n # comment",
        "Merge tag 'v1.0.0'\n",
        "Revert \"fix: a\"",
        "reapply previous changes",
        "fixup! fix: a",
        "Merged in feature/test (pull request #8)",
        "Merged PR 123: description",
        "Auto-merged develop into master",
        "chore(release): v3.0.0 [skip ci]",
        "1.0.0-alpha.0\n\nSigned-off-by: user",
    ] {
        assert!(is_default_ignored(input), "{input:?}");
    }
    for input in [
        "initial commit",
        "foo Merge branch main",
        "01.0.0",
        "1.0.0-01",
        "9007199254740992.0.0",
    ] {
        assert!(!is_default_ignored(input), "{input:?}");
    }
}

#[test]
fn conventional_preset_has_upstream_limits_and_complete_tuple_overrides() {
    let default = parse_json_config(r#"{"extends":["@commitlint/config-conventional"]}"#).unwrap();
    let raw = format!("fix: {}", "x".repeat(96));
    assert!(lint_message(raw.as_bytes()).is_ok());
    let result = lint_configured(&raw, &default, &EvaluationContext::default()).unwrap();
    assert!(!result.valid);
    assert_eq!(result.errors[0].name, "header-max-length");
    let relaxed = parse_json_config(
        r#"{"extends":"@commitlint/config-conventional","rules":{"header-max-length":[2,"always",128]}}"#,
    )
    .unwrap();
    assert!(
        lint_configured(&raw, &relaxed, &EvaluationContext::default())
            .unwrap()
            .valid
    );
    assert_eq!(default.rules.len(), relaxed.rules.len());
    assert!(
        lint_configured("fix: café", &default, &EvaluationContext::default())
            .unwrap()
            .valid
    );
}

#[test]
fn diagnostics_preserve_configuration_order_and_severity() {
    let config = parse_json_config(r#"{"rules":{"subject-empty":[2,"never"],"type-empty":[2,"never"],"header-full-stop":[1,"always","."]}}"#).unwrap();
    let result = lint_configured("invalid header", &config, &EvaluationContext::default()).unwrap();
    assert!(!result.valid);
    assert_eq!(
        result
            .errors
            .iter()
            .map(|d| d.name.as_str())
            .collect::<Vec<_>>(),
        ["subject-empty", "type-empty"]
    );
    assert_eq!(result.warnings[0].name, "header-full-stop");
}

#[test]
fn configuration_validation_precedes_empty_or_ignored_shortcuts() {
    for text in [
        r#"{"rules":{"unknown-rule":[0]}}"#,
        r#"{"rules":{"subject-case":[2,"always","unknown-case"]}}"#,
        r#"{"rules":{"type-enum":[2]}}"#,
        r#"{"rules":{"type-empty":[1.5,"never"]}}"#,
        r#"{"rules":{"type-enum":[2,"always",null]}}"#,
        r#"{"plugins":[]}"#,
        r#"{"parserPreset":{"parserOpts":{"headerPattern":".*"}}}"#,
        r#"{"extends":"conventional"}"#,
        r#"{"extends":"commitlint-config-conventional"}"#,
        r#"{"parserPreset":"angular"}"#,
        r#"{"parserPreset":"conventionalcommits"}"#,
    ] {
        assert!(parse_json_config(text).is_err(), "{text}");
    }
    let config = parse_json_config(r#"{"rules":{"trailer-exists":[0]}}"#).unwrap();
    assert!(
        lint_configured("", &config, &EvaluationContext::default())
            .unwrap()
            .valid
    );
    let strict =
        parse_json_config(r#"{"defaultIgnores":false,"rules":{"type-empty":[2,"never"]}}"#)
            .unwrap();
    assert!(
        !lint_configured(
            "Merge pull request #369",
            &strict,
            &EvaluationContext::default()
        )
        .unwrap()
        .valid
    );
}

#[test]
fn duplicate_rule_keys_keep_first_position_and_last_value() {
    let config = parse_json_config(r#"{"rules":{"subject-empty":[2,"always"],"type-empty":[2,"never"],"subject-empty":[1,"never"]}}"#).unwrap();
    assert_eq!(
        config
            .rules
            .iter()
            .map(|r| r.name.as_str())
            .collect::<Vec<_>>(),
        ["subject-empty", "type-empty"]
    );
    assert_eq!(
        config.rules[0].severity,
        commitlint_rust::ConfigSeverity::Warning
    );
    assert_eq!(
        config.rules[0].condition,
        Some(commitlint_rust::RuleCondition::Never)
    );
}

#[test]
fn scope_objects_nested_case_conditions_and_fractional_limits_are_usable() {
    let config = parse_json_config(r#"{"rules":{"scope-enum":[2,"always",{"scopes":["api","parser"],"delimiters":["."]}],"scope-case":[2,"always",{"cases":[{"case":"lower-case","when":"always"}],"delimiters":["."]}],"subject-min-length":[2,"never",1.5]}}"#).unwrap();
    assert!(
        lint_configured(
            "fix(api.parser): ab",
            &config,
            &EvaluationContext::default()
        )
        .unwrap()
        .valid
    );
    let result =
        lint_configured("fix(api.parser): a", &config, &EvaluationContext::default()).unwrap();
    assert!(!result.valid);
    assert_eq!(
        result.errors[0].message,
        "subject must not be shorter than 1.5 characters"
    );
}

#[test]
fn configured_trailer_tool_failure_is_fatal_even_for_warning_or_never() {
    let context = EvaluationContext {
        git: Some(std::env::temp_dir().join("commitlint-missing-tool-for-configured-test")),
        ..EvaluationContext::default()
    };
    for severity in [1, 2] {
        for condition in ["always", "never"] {
            let config = parse_json_config(&format!(
                r#"{{"rules":{{"trailer-exists":[{severity},"{condition}","Signed-off-by:"]}}}}"#
            ))
            .unwrap();
            assert!(lint_configured("fix: x", &config, &context).is_err());
        }
    }
    let disabled = parse_json_config(r#"{"rules":{"trailer-exists":[0]}}"#).unwrap();
    assert!(
        lint_configured("fix: x", &disabled, &context)
            .unwrap()
            .valid
    );
}

#[test]
fn configured_parser_prefixes_control_footer_and_references_rules() {
    let config = parse_json_config(r#"{"parserPreset":{"parserOpts":{"issuePrefixes":["GH-"],"referenceActions":["delivers"]}},"rules":{"references-empty":[2,"never"],"body-empty":[2,"always"]}}"#).unwrap();
    let result = lint_configured(
        "fix: x\n\ndelivers GH-12",
        &config,
        &EvaluationContext::default(),
    )
    .unwrap();
    assert!(result.valid);
    let parsed = result.parsed.unwrap();
    assert_eq!(parsed.body.as_deref(), Some(""));
    assert_eq!(parsed.footer.as_deref(), Some("delivers GH-12"));
    assert_eq!(parsed.references[0].action.as_deref(), Some("delivers"));
    assert_eq!(parsed.references[0].issue, "12");
}

#[test]
fn numeric_severity_accepts_equivalent_json_spellings() {
    for severity in ["2", "2.0", "2e0", "2.00e+0"] {
        let config = parse_json_config(&format!(
            r#"{{"defaultIgnores":false,"rules":{{"type-empty":[{severity},"never"]}}}}"#
        ))
        .unwrap();
        assert!(
            lint_configured("fix: a", &config, &EvaluationContext::default())
                .unwrap()
                .valid
        );
    }
    for severity in ["0.0", "0e0", "-0.0"] {
        let config =
            parse_json_config(&format!(r#"{{"rules":{{"trailer-exists":[{severity}]}}}}"#))
                .unwrap();
        assert_eq!(
            config.rules[0].severity,
            commitlint_rust::ConfigSeverity::Disabled
        );
    }
    for severity in ["1.5", "-1", "3", "\"2\"", "true", "null"] {
        assert!(
            parse_json_config(&format!(
                r#"{{"rules":{{"type-empty":[{severity},"never"]}}}}"#
            ))
            .is_err()
        );
    }
}

#[test]
fn default_ignores_obey_js_line_terminators_and_semver_input_limit() {
    let config = parse_json_config(r#"{"rules":{"type-empty":[2,"never"]}}"#).unwrap();
    for raw in [
        "Merge x\r into main",
        "Merge x\u{2028} into main",
        "Merge x\u{2029} into main",
    ] {
        assert!(!is_default_ignored(raw), "{raw:?}");
        assert!(
            !lint_configured(raw, &config, &EvaluationContext::default())
                .unwrap()
                .valid
        );
    }
    let too_long = format!("v1.2.3+{}", "a".repeat(250));
    assert!(!is_default_ignored(&too_long));
    assert!(
        !lint_configured(&too_long, &config, &EvaluationContext::default())
            .unwrap()
            .valid
    );
    let longest = format!("v1.2.3+{}", "a".repeat(249));
    assert!(is_default_ignored(&longest));
    for separator in ['\r', '\u{2028}', '\u{2029}'] {
        assert!(is_default_ignored(&format!(
            "other{separator}Merge tag v1.2.3{separator}other"
        )));
    }
    assert!(!is_default_ignored("1.2.3 [skip\u{85}ci]"));
    assert!(is_default_ignored("1.2.3 [skip\u{a0}ci]"));
    assert!(!is_default_ignored("1.2.3 [sKip ci]"));
}
