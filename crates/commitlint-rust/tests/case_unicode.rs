use commitlint_rust::case;

#[test]
fn node_unicode_case_fixtures() {
    let data: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/case-unicode.json")).unwrap();
    assert_eq!(data["node"], "26.8.2");
    assert_eq!(data["unicode"], "17.0");
    for item in data["cases"].as_array().unwrap() {
        let input = item["input"].as_str().unwrap();
        let target = item["target"].as_str().unwrap();
        assert_eq!(
            case::ensure_case(input, target).unwrap(),
            item["expected"].as_bool().unwrap(),
            "input {input:?}, target {target}"
        );
    }
    for item in data["gates"].as_array().unwrap() {
        let input = item["input"].as_str().unwrap().chars().next().unwrap();
        assert_eq!(
            case::subject_gate(input),
            item["expected"].as_bool().unwrap(),
            "subject gate {input:?}"
        );
    }
}

#[test]
fn rejects_unknown_targets() {
    assert!(!case::is_target_case("title-case"));
    assert_eq!(
        case::ensure_case("abc", "title-case").unwrap_err(),
        "to-case: Unknown target case \"title-case\""
    );
}

#[test]
fn normalization_data_matches_frozen_oracle_unicode_version() {
    assert_eq!(unicode_normalization::UNICODE_VERSION, (17, 0, 0));
}
