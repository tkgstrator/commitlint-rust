use commitlint_rust::rules::EvaluationContext;
use commitlint_rust::{Diagnostic, ParsedMessage, Severity, lint_configured, parse_json_config};
use serde_json::{Value, json};

fn diagnostics(actual: &[Diagnostic]) -> Value {
    Value::Array(
        actual
            .iter()
            .map(|d| {
                json!({
                    "name":d.name,"level":if d.severity==Severity::Error {2} else {1},
                    "message":d.message,"valid":false
                })
            })
            .collect(),
    )
}
fn parsed(actual: &ParsedMessage, raw: &str) -> Value {
    json!({"header":actual.header,"type":actual.r#type,"scope":actual.scope,
        "subject":actual.subject,"body":actual.body,"footer":actual.footer,"raw":raw,
        "references":actual.references.iter().map(|r|json!({
            "action":r.action,"owner":r.owner,"repository":r.repository,
            "issue":r.issue,"prefix":r.prefix,"raw":r.raw
        })).collect::<Vec<_>>()})
}

#[test]
fn actual_json_loader_and_configured_lint_oracle() {
    let corpus: Value =
        serde_json::from_str(include_str!("fixtures/configured-upstream.json")).unwrap();
    assert_eq!(corpus["schema"], 1);
    assert_eq!(corpus["execution"]["node"], "v26.8.2");
    let cases = corpus["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 111);
    let mut match_count = 0;
    let mut failures = Vec::new();
    for case in cases {
        let label = case["id"].as_str().unwrap();
        let configuration = parse_json_config(case["configText"].as_str().unwrap());
        let checked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            match case["nativeExpected"].as_str().unwrap() {
                "config-error" => {
                    assert!(
                        configuration.is_err(),
                        "{label}: strict configuration should refuse: {configuration:?}"
                    );
                    if case["upstream"]["loadError"].is_null()
                        && case["upstream"]["lintError"].is_null()
                    {
                        assert!(
                            case["deviation"].is_string(),
                            "{label}: no silent upstream acceptance difference"
                        );
                    }
                }
                "lint-error" => {
                    let configuration = configuration.unwrap_or_else(|e| panic!("{label}: {e}"));
                    let result = lint_configured(
                        case["message"].as_str().unwrap(),
                        &configuration,
                        &EvaluationContext::default(),
                    );
                    assert!(
                        result.is_err(),
                        "{label}: upstream lint throws, native should refuse"
                    );
                    assert!(case["upstream"]["lintError"].is_string());
                }
                "match" => {
                    match_count += 1;
                    assert!(
                        case["deviation"].is_null(),
                        "{label}: match rows cannot disguise deviations"
                    );
                    assert!(case["upstream"]["loadError"].is_null());
                    assert!(case["upstream"]["lintError"].is_null());
                    let configuration = configuration.unwrap_or_else(|e| panic!("{label}: {e}"));
                    // No active trailer rule appears in this corpus. Disabled trailer
                    // rules must succeed without any configured Git executable.
                    let raw = case["message"].as_str().unwrap();
                    let actual =
                        lint_configured(raw, &configuration, &EvaluationContext::default())
                            .unwrap_or_else(|e| panic!("{label}: {e}"));
                    let expected = &case["upstream"]["result"];
                    assert_eq!(
                        actual.valid,
                        expected["valid"].as_bool().unwrap(),
                        "{label}: validity"
                    );
                    assert_eq!(
                        diagnostics(&actual.errors),
                        expected["errors"],
                        "{label}: ordered errors"
                    );
                    assert_eq!(
                        diagnostics(&actual.warnings),
                        expected["warnings"],
                        "{label}: ordered warnings"
                    );
                    if raw.is_empty() || case["upstream"]["ignored"].as_bool().unwrap() {
                        assert!(
                            actual.parsed.is_none(),
                            "{label}: ignored/empty short circuit"
                        );
                    } else {
                        let actual = actual
                            .parsed
                            .as_ref()
                            .unwrap_or_else(|| panic!("{label}: missing parsed fields"));
                        assert_eq!(
                            parsed(actual, raw),
                            case["upstream"]["parsed"],
                            "{label}: parsed fields/references"
                        );
                    }
                }
                other => panic!("{label}: unknown fixture expectation {other}"),
            }
        }));
        if let Err(error) = checked {
            let detail = error
                .downcast_ref::<String>()
                .cloned()
                .or_else(|| error.downcast_ref::<&str>().map(|s| s.to_string()))
                .unwrap_or_else(|| "unknown failure".into());
            failures.push(format!("{label}: {detail}"));
        }
    }
    assert!(match_count >= 70);
    assert!(
        failures.is_empty(),
        "{} configured oracle mismatches:\n{}",
        failures.len(),
        failures.join("\n")
    );
}
