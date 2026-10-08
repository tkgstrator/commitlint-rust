#![cfg(unix)]
mod common;
use common::*;
use serde_json::Value;

fn sorted(value: Value) -> Value {
    match value {
        Value::Object(map) => {
            let ordered: std::collections::BTreeMap<_, _> = map.into_iter().collect();
            Value::Object(ordered.into_iter().map(|(k, v)| (k, sorted(v))).collect())
        }
        Value::Array(values) => Value::Array(values.into_iter().map(sorted).collect()),
        scalar => scalar,
    }
}

#[test]
fn installation_config_retains_original_sorted_json_bytes() {
    let f = Fixture::new();
    accepted(f.install(&[]));
    let bytes = std::fs::read(f.guard_root().join("config.json")).unwrap();
    let expected =
        serde_json::to_vec_pretty(&sorted(serde_json::from_slice(&bytes).unwrap())).unwrap();
    assert_eq!(bytes, expected);
}

#[test]
fn checker_stdout_retains_original_sorted_json_bytes() {
    let f = Fixture::new();
    let repo = f.repo("wire order");
    let oid = f.commit(&repo, "fix: wire contract", &[]);
    let output = f.canonical(&["commits", &oid], &repo, &[], None);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let expected = serde_json::to_string(&sorted(serde_json::from_slice(&output.stdout).unwrap()))
        .unwrap()
        + "\n";
    assert_eq!(output.stdout, expected.as_bytes());
}
