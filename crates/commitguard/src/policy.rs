//! Attribution policy. Message linting itself lives in the embedded
//! `commitlint-rust` library; identity and credit rules belong to the guard.
use crate::{Identity, Result};
use regex::Regex;
use std::sync::OnceLock;

pub use commitlint_rust::lint_message;

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
