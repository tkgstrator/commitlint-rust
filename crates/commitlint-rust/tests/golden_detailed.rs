//! Detailed diagnostics against the pinned oracle corpus (schema 2).
use commitlint_rust::{ParserPreset, Severity, lint_detailed, lint_message, parse_message};
use serde_json::Value;

fn names(value: &Value) -> Vec<&str> {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect()
}

#[test]
fn detailed_matches_golden_parse_errors_and_warnings() {
    let fixture: Value =
        serde_json::from_str(include_str!("fixtures/commitlint-golden.json")).unwrap();
    assert_eq!(fixture["schema"], 2);
    let cases = fixture["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 221);
    let mut unparsed = 0;
    for item in cases {
        let id = item["id"].as_str().unwrap();
        let message = item["message"].as_str().unwrap();
        let outcome = lint_detailed(message.as_bytes());
        assert_eq!(
            outcome.valid,
            item["valid"].as_bool().unwrap(),
            "{id} valid"
        );
        assert_eq!(
            outcome.valid,
            lint_message(message.as_bytes()).is_ok(),
            "{id} legacy"
        );
        assert_eq!(
            parse_message(message, ParserPreset::ConventionalCommits).is_err(),
            item["parseError"].is_string(),
            "{id} parseError"
        );
        // Upstream parse failures ("Expected a raw commit": empty or
        // newline/whitespace-only input) have no six-field parse. They are
        // handled explicitly: no parsed value, and the oracle's own errors
        // (`[]` for "" whose lint is skipped upstream, `parse-error` for
        // whitespace-only input).
        match item["parsed"].as_object() {
            None => {
                unparsed += 1;
                assert!(outcome.parsed.is_none(), "{id} parsed");
                assert!(
                    item["parseError"].is_string(),
                    "{id} unexplained null parse"
                );
            }
            Some(expected) => {
                let parsed = outcome
                    .parsed
                    .as_ref()
                    .unwrap_or_else(|| panic!("{id} parsed"));
                for (field, actual) in [
                    ("header", &parsed.header),
                    ("type", &parsed.r#type),
                    ("scope", &parsed.scope),
                    ("subject", &parsed.subject),
                    ("body", &parsed.body),
                    ("footer", &parsed.footer),
                ] {
                    assert_eq!(actual.as_deref(), expected[field].as_str(), "{id} {field}");
                }
            }
        }
        let got: Vec<_> = outcome.errors.iter().map(|d| d.name.as_str()).collect();
        if item["policyException"] == "empty-message-policy" {
            assert_eq!(message, "");
            assert_eq!(item["upstreamValid"], true);
            assert!(names(&item["errors"]).is_empty());
            assert_eq!(got, ["message-empty"], "{id} explicit policy error");
        } else {
            assert_eq!(got, names(&item["errors"]), "{id} errors {message:?}");
        }
        let got: Vec<_> = outcome.warnings.iter().map(|d| d.name.as_str()).collect();
        assert_eq!(got, names(&item["warnings"]), "{id} warnings {message:?}");
        assert!(outcome.errors.iter().all(|d| d.severity == Severity::Error));
        assert!(
            outcome
                .warnings
                .iter()
                .all(|d| d.severity == Severity::Warning)
        );
    }
    assert_eq!(unparsed, 3, "empty, LF and LF LF only");
}
