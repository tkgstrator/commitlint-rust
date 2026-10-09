use super::*;

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
        serde_json::from_str(include_str!("../fixtures/full-upstream-rules.json")).unwrap();
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
        serde_json::from_str(include_str!("../fixtures/full-upstream-rules.json")).unwrap();
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
