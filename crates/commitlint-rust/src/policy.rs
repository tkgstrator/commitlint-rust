//! Runtime-independent implementation of the pinned commitlint policy subset.
use crate::Result;
use crate::parser::{self, ParsedMessage, ParserPreset, parse_message};
use crate::rules::{self, RuleCondition, RuleValue, evaluate_rule};
use regex::Regex;
use std::sync::OnceLock;

fn header_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"^(\w*)(?:\((.*)\))?!?: (.*)$").unwrap())
}

pub fn lint_message(bytes: &[u8]) -> Result<()> {
    if bytes
        .iter()
        .any(|byte| *byte != b'\n' && !(b' '..=b'~').contains(byte))
    {
        return Err("message requires English printable ASCII and LF line endings".into());
    }
    let message = std::str::from_utf8(bytes).map_err(|_| "invalid message encoding")?;
    if message.trim_end_matches('\n').len() > 128 {
        return Err("entire message exceeds 128 characters".into());
    }
    if message.trim().is_empty() {
        return Err("commit message is empty".into());
    }
    // The actual conventional-commits parser removes surrounding newlines and
    // lowercase gpg diagnostic lines. These are parser semantics, not ignores.
    let header = message
        .trim_matches('\n')
        .split('\n')
        .find(|line| !line.trim_start().starts_with("gpg:"))
        .ok_or("commit message has no header")?;
    if header.trim() != header {
        return Err("header must not contain surrounding whitespace".into());
    }
    let capture = header_regex()
        .captures(header)
        .ok_or("invalid Conventional Commit header")?;
    let kind = capture.get(1).map(|v| v.as_str()).unwrap_or_default();
    if ![
        "build", "chore", "ci", "docs", "feat", "fix", "perf", "refactor", "revert", "style",
        "test",
    ]
    .contains(&kind)
    {
        return Err("unsupported or non-lowercase Conventional Commit type".into());
    }
    let subject = capture.get(3).map(|v| v.as_str()).unwrap_or_default();
    if subject.is_empty() {
        return Err("Conventional Commit subject is empty".into());
    }
    // For ASCII subjects beginning in an uppercase letter, sentence-case's
    // upperFirst comparison already matches. Non-letter initial symbols are
    // explicitly exempt in commitlint's subject-case rule.
    if subject
        .as_bytes()
        .first()
        .is_some_and(u8::is_ascii_uppercase)
    {
        return Err("subject must not use sentence/start/pascal/upper case".into());
    }
    if header.ends_with('.') && !header.ends_with("...") {
        return Err("subject must not end with a full stop".into());
    }
    // Every line is necessarily <=128 once the entire trimmed message is.
    // Leading blank body/footer rules are warnings, not rejection criteria.
    Ok(())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Diagnostic {
    pub name: String,
    pub severity: Severity,
    pub message: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LintOutcome {
    pub valid: bool,
    pub parsed: Option<ParsedMessage>,
    pub errors: Vec<Diagnostic>,
    pub warnings: Vec<Diagnostic>,
}

const POLICY_LIMIT: usize = 128;
const TYPES: [&str; 11] = [
    "build", "chore", "ci", "docs", "feat", "fix", "perf", "refactor", "revert", "style", "test",
];

fn diagnostic(name: &str, severity: Severity, message: Option<String>) -> Diagnostic {
    Diagnostic {
        name: name.to_owned(),
        severity,
        message: message.unwrap_or_default(),
    }
}

/// Fixed-policy diagnostics: every failing rule of the pinned configuration
/// (config-conventional with 128-character limits plus the ASCII and
/// whole-message rules), in configuration order. `valid` agrees with
/// `lint_message` for every input.
///
/// The fixed policy rejects empty messages and missing headers explicitly,
/// even where upstream skips them. Whitespace-only input reports the parser's
/// `parse-error`. The legacy API remains the acceptance authority.
pub fn lint_detailed(bytes: &[u8]) -> LintOutcome {
    let mut errors = Vec::new();
    let mut warnings = Vec::new();
    let ascii = bytes
        .iter()
        .all(|byte| *byte == b'\n' || (b' '..=b'~').contains(byte));
    let Ok(raw) = std::str::from_utf8(bytes) else {
        errors.push(diagnostic(
            "ascii-message",
            Severity::Error,
            Some("use English printable ASCII text only".into()),
        ));
        return LintOutcome {
            valid: false,
            parsed: None,
            errors,
            warnings,
        };
    };
    let parsed = match parse_message(raw, ParserPreset::ConventionalCommits) {
        Ok(parsed) => parsed,
        Err(message) => {
            if raw.is_empty() {
                errors.push(diagnostic(
                    "message-empty",
                    Severity::Error,
                    Some("commit message is empty".into()),
                ));
            } else {
                errors.push(diagnostic("parse-error", Severity::Error, Some(message)));
            }
            return LintOutcome {
                valid: false,
                parsed: None,
                errors,
                warnings,
            };
        }
    };
    if parsed.header.is_none() && parsed.body.is_none() && parsed.footer.is_none() {
        errors.push(diagnostic(
            "header-empty",
            Severity::Error,
            Some("commit message has no header".into()),
        ));
        return LintOutcome {
            valid: false,
            parsed: Some(parsed),
            errors,
            warnings,
        };
    }
    let run = |name: &str, when: Option<RuleCondition>, value: RuleValue| {
        let outcome = evaluate_rule(name, &parsed, raw, when, &value)
            .expect("fixed policy rules are supported and well-typed");
        (!outcome.valid).then(|| diagnostic(name, Severity::Error, outcome.message))
    };
    let always = Some(RuleCondition::Always);
    let never = Some(RuleCondition::Never);
    let limit = RuleValue::Length(POLICY_LIMIT);
    let mut push = |found: Option<Diagnostic>| errors.extend(found);
    push(run("body-max-line-length", always, limit.clone()));
    push(run("footer-max-line-length", always, limit.clone()));
    push(run("header-max-length", always, limit));
    let trim = rules::header_trim(&parsed);
    push((!trim.valid).then(|| diagnostic("header-trim", Severity::Error, trim.message)));
    let case = rules::subject_case(&parsed);
    push((!case.valid).then(|| diagnostic("subject-case", Severity::Error, case.message)));
    push(run("subject-empty", never, RuleValue::None));
    push(run("subject-full-stop", never, RuleValue::Text(".".into())));
    let case = rules::type_case(&parsed);
    push((!case.valid).then(|| diagnostic("type-case", Severity::Error, case.message)));
    push(run("type-empty", never, RuleValue::None));
    push(run(
        "type-enum",
        always,
        RuleValue::List(TYPES.iter().map(|t| (*t).to_owned()).collect()),
    ));
    if !ascii {
        errors.push(diagnostic(
            "ascii-message",
            Severity::Error,
            Some("use English printable ASCII text only".into()),
        ));
    }
    if parser::js_len(raw.trim_end_matches('\n')) > POLICY_LIMIT {
        errors.push(diagnostic(
            "message-max-length",
            Severity::Error,
            Some(format!(
                "entire message must be at most {POLICY_LIMIT} characters"
            )),
        ));
    }
    for (name, rule_when) in [
        ("body-leading-blank", always),
        ("footer-leading-blank", always),
    ] {
        let outcome = evaluate_rule(name, &parsed, raw, rule_when, &RuleValue::None)
            .expect("fixed policy rules are supported and well-typed");
        if !outcome.valid {
            warnings.push(diagnostic(name, Severity::Warning, outcome.message));
        }
    }
    let legacy = lint_message(bytes);
    if let Err(message) = &legacy
        && errors.is_empty()
    {
        errors.push(diagnostic(
            "policy-compatibility",
            Severity::Error,
            Some(message.clone()),
        ));
    }
    LintOutcome {
        valid: legacy.is_ok(),
        parsed: Some(parsed),
        errors,
        warnings,
    }
}
