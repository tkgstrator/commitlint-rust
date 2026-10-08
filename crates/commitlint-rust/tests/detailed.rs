use commitlint_rust::{Severity, lint_detailed, lint_message};

#[test]
fn detailed_reports_warnings_without_rejecting() {
    let input = b"fix: change behavior\nbody text";
    let outcome = lint_detailed(input);
    assert_eq!(outcome.valid, lint_message(input).is_ok());
    assert!(outcome.errors.is_empty());
    assert_eq!(outcome.warnings.len(), 1);
    assert_eq!(outcome.warnings[0].name, "body-leading-blank");
    assert_eq!(outcome.warnings[0].severity, Severity::Warning);
    assert_eq!(outcome.parsed.unwrap().body.as_deref(), Some("body text"));
}

#[test]
fn detailed_accumulates_independent_errors() {
    let input = b"BANANA: Uppercase.";
    let outcome = lint_detailed(input);
    assert!(!outcome.valid);
    assert_eq!(outcome.valid, lint_message(input).is_ok());
    let names: Vec<_> = outcome.errors.iter().map(|d| d.name.as_str()).collect();
    assert_eq!(
        names,
        [
            "subject-case",
            "subject-full-stop",
            "type-case",
            "type-enum"
        ]
    );
    assert!(outcome.errors.iter().all(|d| d.severity == Severity::Error));
}

#[test]
fn policy_refusals_always_explain_empty_and_missing_header() {
    for (input, name) in [
        ("", "message-empty"),
        ("gpg: diagnostic", "header-empty"),
        (" gpg: diagnostic\n", "header-empty"),
    ] {
        let result = lint_detailed(input.as_bytes());
        assert!(!result.valid);
        assert_eq!(result.valid, lint_message(input.as_bytes()).is_ok());
        assert_eq!(result.errors.len(), 1);
        assert_eq!(result.errors[0].name, name);
    }
}

#[test]
fn detailed_and_legacy_agree_on_composed_inputs() {
    let prefixes = ["", "\n", "gpg: diagnostic\n", " gpg: diagnostic\n", "\n\n"];
    let kinds = [
        "fix",
        "feat",
        "FIX",
        "banana",
        "",
        "fix1",
        "fix(scope)",
        "fix()!",
        "fix(a)(b)",
    ];
    let subjects = [
        "change",
        "Change",
        "iOS",
        "API",
        "1st",
        "a'quoted'B",
        "a`Upper`B",
        "'Quoted'",
        "...",
        ".",
        " ",
        "",
        "日本語",
        "hi😀",
    ];
    let tails = [
        "",
        "\n",
        "\nbody",
        "\n\nbody",
        "\nRefs: #1",
        "\n\nBREAKING CHANGE: new",
        "\n\r",
        "\n\ngpg: diagnostic",
    ];
    for prefix in prefixes {
        for kind in kinds {
            for subject in subjects {
                for tail in tails {
                    let input = format!("{prefix}{kind}: {subject}{tail}");
                    let result = lint_detailed(input.as_bytes());
                    assert_eq!(
                        result.valid,
                        lint_message(input.as_bytes()).is_ok(),
                        "{input:?}"
                    );
                    assert_eq!(
                        result.valid,
                        result.errors.is_empty(),
                        "{input:?} {:?}",
                        result.errors
                    );
                }
            }
        }
    }
}
