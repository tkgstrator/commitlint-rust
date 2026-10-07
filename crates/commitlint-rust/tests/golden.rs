use commitlint_rust::lint_message;

#[test]
fn pinned_commitlint_golden_parity() {
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/commitlint-golden.json")).unwrap();
    let cases = fixture["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 221);
    for item in cases {
        let message = item["message"].as_str().unwrap();
        assert_eq!(
            lint_message(message.as_bytes()).is_ok(),
            item["valid"].as_bool().unwrap(),
            "{} {:?}",
            item["id"],
            message
        );
    }
}
