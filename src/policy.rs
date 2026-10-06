//! Runtime-independent implementation of the pinned commitlint policy subset.
use crate::{Identity, Result};
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

pub fn recognized_ai(value: &str) -> bool {
    if !value.bytes().all(|b| (b' '..=b'~').contains(&b)) {
        return false;
    }
    let value = value.trim();
    if value == "Codex" {
        return true;
    }
    let Some((name, provider)) = value.strip_suffix('>').and_then(|v| v.rsplit_once(" <")) else {
        return false;
    };
    if name.contains(['<', '>']) || provider.contains(['<', '>']) {
        return false;
    }
    let name = name.to_ascii_lowercase();
    let provider = provider.to_ascii_lowercase();
    if provider == "noreply@anthropic.com" {
        static RE: OnceLock<Regex> = OnceLock::new();
        return RE.get_or_init(|| Regex::new(r"^claude(?: +(?:opus|sonnet|haiku|fable)(?: +[0-9.]+)?(?: +\([0-9]+m context\))?)?$").unwrap()).is_match(&name);
    }
    if provider == "noreply@openai.com" {
        static RE: OnceLock<Regex> = OnceLock::new();
        return RE
            .get_or_init(|| {
                Regex::new(r"^(?:codex|chatgpt|gpt(?:[ -]?[0-9.]+)?(?: +astra)?)$").unwrap()
            })
            .is_match(&name);
    }
    matches!(
        (name.as_str(), provider.as_str()),
        ("cursor", "cursoragent@cursor.com") | ("qwen-cli", "https://github.com/qwenlm/qwen-code")
    )
}

/// Scan standardized attribution independently of Git trailer configuration,
/// including body-position credits and folded values. Repository aliases can
/// neither rename these keys away nor conceal an additional identity.
pub fn validate_attribution(bytes: &[u8], identity: &Identity) -> Result<()> {
    let message = std::str::from_utf8(bytes).map_err(|_| "invalid message encoding")?;
    let lines: Vec<&str> = message.split('\n').collect();
    let canonical = format!("{} <{}>", identity.login, identity.email);
    for (index, line) in lines.iter().enumerate() {
        let Some((key, value)) = line.trim_start().split_once(':') else {
            continue;
        };
        let key = key.trim().to_ascii_lowercase();
        if key != "co-authored-by" && key != "signed-off-by" {
            continue;
        }
        let mut value = value.trim().to_string();
        for continuation in &lines[index + 1..] {
            if continuation.is_empty() || !continuation.starts_with([' ', '\t']) {
                break;
            }
            value.push(' ');
            value.push_str(continuation.trim());
        }
        if value != canonical && !recognized_ai(&value) {
            return Err(format!("{key} names an unapproved identity"));
        }
    }
    Ok(())
}

pub fn validate_message(bytes: &[u8], identity: &Identity) -> Result<()> {
    lint_message(bytes)?;
    validate_attribution(bytes, identity)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pinned_commitlint_golden_parity() {
        let fixture: serde_json::Value =
            serde_json::from_str(include_str!("../tests/fixtures/commitlint-golden.json")).unwrap();
        for item in fixture["cases"].as_array().unwrap() {
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
    #[test]
    fn entire_message_and_attribution() {
        let identity = Identity {
            login: "tester".into(),
            email: "44+tester@users.noreply.github.com".into(),
        };
        assert!(
            validate_message(
                b"fix: valid\n\nCo-authored-by: Claude <noreply@anthropic.com>",
                &identity
            )
            .is_ok()
        );
        assert!(validate_message(b"fix: valid\n\nCo-authored-by: Codex", &identity).is_ok());
        assert!(
            validate_message(
                b"fix: valid\n\nSigned-off-by: Other <other@example.com>",
                &identity
            )
            .is_err()
        );
        assert!(
            validate_message(
                b"fix: valid\n\nCo-authored-by: Codex\n additional identity",
                &identity
            )
            .is_err()
        );
        assert!(
            validate_message(
                b"fix: valid\nCo-authored-by: Other <other@example.com>\n\nbody follows",
                &identity
            )
            .is_err()
        );
        assert!(!recognized_ai("Claude <invented@example.com>"));
        assert!(!recognized_ai("codex"));
    }
}
