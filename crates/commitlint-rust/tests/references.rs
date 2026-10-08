use commitlint_rust::parser::{ParserOptions, ParserPreset, parse_message_with_options};
use serde_json::{Value, json};

#[test]
fn supported_parser_profile_matches_upstream_fields_and_references() {
    let corpus: Value =
        serde_json::from_str(include_str!("fixtures/full-upstream-rules.json")).unwrap();
    let cases = corpus["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 532);
    let mut supported = 0;
    let mut provided_custom = 0;
    for case in cases {
        let id = case["id"].as_str().unwrap();
        let mut options = ParserOptions::default();
        assert_eq!(case["parserPreset"], "angular");
        if let Some(settings) = case["parserOptions"]["value"].as_object() {
            if let Some(pattern) = settings.get("headerPattern") {
                let source = pattern["source"].as_str().unwrap();
                if source == "^(.*): (.*)$" {
                    assert_eq!(case["rule"], "type-case");
                    assert_eq!(settings["headerCorrespondence"], json!(["type", "subject"]));
                    provided_custom += 1;
                    // These seven rule assertions use a custom parser. The
                    // all-rule runner evaluates their actual frozen records;
                    // this test claims only the supported native profile.
                    continue;
                }
                assert_eq!(
                    source, "^(\\w*)(?:\\((.*)\\))?: (.*)$",
                    "{id} unsupported custom parser"
                );
                assert_eq!(pattern["flags"], "");
            }
            if let Some(comment) = settings.get("commentChar") {
                let value = comment.as_str().unwrap();
                assert_eq!(value.chars().count(), 1);
                options.comment_char = value.chars().next();
            }
            if let Some(prefixes) = settings.get("issuePrefixes") {
                options.issue_prefixes = prefixes
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|p| p.as_str().unwrap().to_owned())
                    .collect();
            }
            if let Some(flag) = settings.get("issuePrefixesCaseSensitive") {
                options.issue_prefixes_case_sensitive = flag.as_bool().unwrap();
            }
            if let Some(actions) = settings.get("referenceActions") {
                options.reference_actions = actions
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|p| p.as_str().unwrap().to_owned())
                    .collect();
            }
        }
        let raw = case["parsed"]["raw"].as_str().unwrap();
        let parsed = parse_message_with_options(raw, &options).unwrap();
        for (field, value) in [
            ("header", &parsed.header),
            ("type", &parsed.r#type),
            ("scope", &parsed.scope),
            ("subject", &parsed.subject),
            ("body", &parsed.body),
            ("footer", &parsed.footer),
        ] {
            assert_eq!(
                value.as_deref(),
                case["parsed"][field].as_str(),
                "{id} {field}"
            );
        }
        let refs: Vec<_> = parsed.references.iter().map(|r| json!({"raw":r.raw,"action":r.action,"owner":r.owner,"repository":r.repository,"prefix":r.prefix,"issue":r.issue})).collect();
        assert_eq!(
            Value::Array(refs),
            case["parsed"]["references"],
            "{id} references"
        );
        supported += 1;
    }
    assert_eq!(supported, 525);
    assert_eq!(provided_custom, 7);
}

#[test]
fn issue_prefixes_change_footer_and_respect_reference_case_flag() {
    let raw = "fix: change\n\nCloses ref-1";
    let defaults = parse_message_with_options(raw, &ParserOptions::default()).unwrap();
    assert_eq!(defaults.body.as_deref(), Some("Closes ref-1"));
    assert!(defaults.footer.is_none());
    let mut opts = ParserOptions {
        issue_prefixes: vec!["REF-".into()],
        ..ParserOptions::default()
    };
    let parsed = parse_message_with_options(raw, &opts).unwrap();
    assert_eq!(parsed.body.as_deref(), Some(""));
    assert_eq!(parsed.footer.as_deref(), Some("Closes ref-1"));
    assert_eq!(parsed.references.len(), 1);
    assert_eq!(parsed.references[0].action.as_deref(), Some("Closes"));
    assert_eq!(parsed.references[0].prefix, "ref-");
    opts.issue_prefixes_case_sensitive = true;
    let sensitive = parse_message_with_options(raw, &opts).unwrap();
    assert_eq!(sensitive.footer, parsed.footer); // footer token regex is always /i
    assert!(sensitive.references.is_empty());
}

#[test]
fn custom_actions_empty_actions_and_url_sentences_match_upstream() {
    let raw = "fix: change\n\ncustom #1 and #2";
    let mut opts = ParserOptions {
        reference_actions: vec!["custom".into()],
        ..ParserOptions::default()
    };
    let parsed = parse_message_with_options(raw, &opts).unwrap();
    assert_eq!(parsed.references.len(), 2);
    assert!(
        parsed
            .references
            .iter()
            .all(|r| r.action.as_deref() == Some("custom"))
    );
    opts.reference_actions.clear();
    assert!(
        parse_message_with_options("fix: change #1", &opts)
            .unwrap()
            .references
            .is_empty()
    );
    assert_eq!(
        parse_message_with_options("fix: change\n\nCloses #1", &opts)
            .unwrap()
            .references
            .len(),
        1
    );
    assert!(
        parse_message_with_options(
            "fix: link https://example.com/#1",
            &ParserOptions::default()
        )
        .unwrap()
        .references
        .is_empty()
    );
}

#[test]
fn filter_scissor_and_gpg_preserve_raw_reference_order() {
    let raw = "# ignored #0\ngpg: diagnostic #9\nfix: change #1\n\nCloses #2\n# ------------------------ >8 ------------------------\nCloses #3";
    let parsed = parse_message_with_options(
        raw,
        &ParserOptions {
            preset: ParserPreset::ConventionalCommits,
            comment_char: Some('#'),
            ..ParserOptions::default()
        },
    )
    .unwrap();
    assert_eq!(
        parsed
            .references
            .iter()
            .map(|r| r.issue.as_str())
            .collect::<Vec<_>>(),
        ["1", "2"]
    );
}

#[test]
fn failed_issue_boundary_preserves_prose_before_later_reference() {
    let raw = "fix: x\n\nSee #1! and #2";
    let parsed = parse_message_with_options(raw, &ParserOptions::default()).unwrap();
    assert_eq!(parsed.references.len(), 1);
    assert_eq!(parsed.references[0].raw, "See #1! and #2");
    assert_eq!(parsed.references[0].issue, "2");
}

#[test]
fn overlapping_issue_prefixes_backtrack_through_complete_footer_match() {
    use commitlint_rust::{RuleCondition, RuleValue, evaluate_rule};
    let raw = "fix: a\n\nCloses REF-1";
    for prefixes in [["REF-1", "REF-"], ["REF-", "REF-1"]] {
        let options = ParserOptions {
            issue_prefixes: prefixes.into_iter().map(str::to_owned).collect(),
            ..ParserOptions::default()
        };
        let parsed = parse_message_with_options(raw, &options).unwrap();
        assert_eq!(parsed.body.as_deref(), Some(""), "{prefixes:?}");
        assert_eq!(
            parsed.footer.as_deref(),
            Some("Closes REF-1"),
            "{prefixes:?}"
        );
        for (condition, valid, message) in [
            (RuleCondition::Never, true, "footer may not be empty"),
            (RuleCondition::Always, false, "footer must be empty"),
        ] {
            let result = evaluate_rule(
                "footer-empty",
                &parsed,
                raw,
                Some(condition),
                &RuleValue::None,
            )
            .unwrap();
            assert_eq!(result.valid, valid, "{prefixes:?} {condition:?}");
            assert_eq!(result.message.as_deref(), Some(message));
        }
    }
}

#[test]
fn unicode_boundary_and_repository_raw_records_match_upstream() {
    let parsed =
        parse_message_with_options("fix: x\n\nsee #1é and #2", &ParserOptions::default()).unwrap();
    assert_eq!(parsed.references.len(), 1);
    assert_eq!(parsed.references[0].raw, "see #1é and #2");
    assert_eq!(parsed.references[0].issue, "2");
    let parsed =
        parse_message_with_options("fix: x\n\nsee #1a, x#2", &ParserOptions::default()).unwrap();
    assert_eq!(parsed.references.len(), 2);
    assert_eq!(parsed.references[0].raw, "see #1a");
    assert_eq!(parsed.references[0].issue, "1a");
    assert_eq!(parsed.references[1].raw, ", x#2");
    assert_eq!(parsed.references[1].issue, "2");
    assert_eq!(parsed.references[1].repository.as_deref(), Some("x"));
}

#[test]
fn empty_reference_actions_have_different_header_and_footer_fallbacks() {
    let options = ParserOptions {
        reference_actions: Vec::new(),
        ..ParserOptions::default()
    };
    for header in ["fix: change #1", "FIX: change #1"] {
        let parsed = parse_message_with_options(header, &options).unwrap();
        assert!(parsed.references.is_empty(), "{header}");
    }
    for footer in ["Closes #1", "closes #1"] {
        let parsed =
            parse_message_with_options(&format!("fix: change\n\n{footer}"), &options).unwrap();
        assert_eq!(parsed.references.len(), 1);
        assert_eq!(parsed.references[0].raw, footer);
        assert_eq!(parsed.references[0].issue, "1");
        assert_eq!(parsed.references[0].action, None);
    }
}

#[test]
fn overlapping_reference_actions_backtrack_after_separator_failure() {
    for actions in [["fix", "fix-up"], ["fix-up", "fix"]] {
        let options = ParserOptions {
            reference_actions: actions.into_iter().map(str::to_owned).collect(),
            ..ParserOptions::default()
        };
        for (raw, expected_raw, expected_action, body, footer) in [
            ("fix-up #1", "#1", Some("fix-up"), None, None),
            ("fix-up: #1", "fix-up: #1", None, None, None),
            (
                "fix: x\n\ncontext fix-up #1",
                "#1",
                Some("fix-up"),
                Some("context fix-up #1"),
                None,
            ),
            (
                "fix: x\n\ncontext fix-up: #1",
                "context fix-up: #1",
                None,
                Some("context fix-up: #1"),
                None,
            ),
            (
                "fix: x\n\nfix-up #1",
                "#1",
                Some("fix-up"),
                Some(""),
                Some("fix-up #1"),
            ),
            (
                "fix: x\n\nfix-up: #1",
                "#1",
                Some("fix-up"),
                Some(""),
                Some("fix-up: #1"),
            ),
            (
                "fix: x\n\nFIX-UP: #1",
                "#1",
                Some("FIX-UP"),
                Some(""),
                Some("FIX-UP: #1"),
            ),
        ] {
            let parsed = parse_message_with_options(raw, &options).unwrap();
            assert_eq!(parsed.body.as_deref(), body, "{actions:?} {raw:?}");
            assert_eq!(parsed.footer.as_deref(), footer, "{actions:?} {raw:?}");
            assert_eq!(parsed.references.len(), 1, "{actions:?} {raw:?}");
            let reference = &parsed.references[0];
            assert_eq!(
                reference.action.as_deref(),
                expected_action,
                "{actions:?} {raw:?}"
            );
            assert_eq!(reference.raw, expected_raw, "{actions:?} {raw:?}");
            assert_eq!(reference.issue, "1");
            assert_eq!(reference.prefix, "#");
            assert_eq!(reference.owner, None);
            assert_eq!(reference.repository, None);
        }
    }
}

#[test]
fn lexical_keyword_boundaries_stop_spans_even_without_full_action_match() {
    for (actions, expected_count) in [
        (vec!["fixes", "fix"], 1),
        (vec!["fixes", "fix", "fix-up"], 2),
    ] {
        let options = ParserOptions {
            reference_actions: actions.into_iter().map(str::to_owned).collect(),
            ..ParserOptions::default()
        };
        for (raw, first, second) in [
            ("fixes #1 fix-up #2", "fixes", "fix-up"),
            ("FIXES #1 FIX-UP #2", "FIXES", "FIX-UP"),
        ] {
            let parsed = parse_message_with_options(raw, &options).unwrap();
            assert_eq!(
                parsed.references.len(),
                expected_count,
                "{raw} {:?}",
                options.reference_actions
            );
            assert_eq!(parsed.references[0].action.as_deref(), Some(first));
            assert_eq!(parsed.references[0].raw, "#1");
            assert_eq!(parsed.references[0].issue, "1");
            if expected_count == 2 {
                assert_eq!(parsed.references[1].action.as_deref(), Some(second));
                assert_eq!(parsed.references[1].raw, "#2");
                assert_eq!(parsed.references[1].issue, "2");
            }
        }
    }
}
