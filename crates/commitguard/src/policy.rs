//! Attribution policy. Message linting itself lives in the embedded
//! `commitlint-rust` library; identity and credit rules belong to the guard.
use crate::{Identity, Result};
use regex::Regex;
use std::borrow::Cow;
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

/// Only the final blank-line-delimited trailer paragraph is eligible. Keep
/// other trailers and all prose in the lint projection; never edit the object.
fn message_without_final_coauthors(message: &str) -> Cow<'_, str> {
    let trimmed = message.trim_end_matches('\n');
    let Some((content, trailers)) = trimmed.rsplit_once("\n\n") else {
        return Cow::Borrowed(message);
    };
    if content.trim().is_empty() {
        return Cow::Borrowed(message);
    }
    let mut retained = Vec::new();
    let mut removed = false;
    for line in trailers.split('\n') {
        let Some((key, value)) = line.split_once(": ") else {
            return Cow::Borrowed(message);
        };
        if key.is_empty()
            || !key.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
            || value.is_empty()
            || value.trim() != value
        {
            return Cow::Borrowed(message);
        }
        if key.eq_ignore_ascii_case("co-authored-by") {
            removed = true;
        } else {
            retained.push(line);
        }
    }
    if !removed {
        Cow::Borrowed(message)
    } else if retained.is_empty() {
        Cow::Borrowed(content.trim_end_matches('\n'))
    } else {
        Cow::Owned(format!("{content}\n\n{}", retained.join("\n")))
    }
}

pub fn validate_message(bytes: &[u8], identity: &Identity) -> Result<()> {
    // Validate the original bytes before removing any attribution from length
    // accounting, so the projection cannot conceal invalid credits or CRLF.
    if bytes
        .iter()
        .any(|b| *b != b'\n' && !(b' '..=b'~').contains(b))
    {
        return Err("message requires English printable ASCII and LF line endings".into());
    }
    validate_attribution(bytes, identity)?;
    let message = std::str::from_utf8(bytes).map_err(|_| "invalid message encoding")?;
    lint_message(message_without_final_coauthors(message).as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn identity() -> Identity {
        Identity {
            login: "tester".into(),
            email: "44+tester@users.noreply.github.com".into(),
        }
    }

    #[test]
    fn final_coauthors_do_not_consume_the_message_budget() {
        let content = format!("fix: {}", "x".repeat(123));
        assert_eq!(content.len(), 128);
        for credit in [
            "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>",
            "co-authored-by: Codex",
            "Co-Authored-By: tester <44+tester@users.noreply.github.com>",
            "Co-Authored-By: Claude <noreply@anthropic.com>\nCo-Authored-By: Codex",
        ] {
            let message = format!("{content}\n\n{credit}\n");
            assert!(
                validate_message(message.as_bytes(), &identity()).is_ok(),
                "{credit}"
            );
            assert!(lint_message(message.as_bytes()).is_err());
            let too_long = format!("{content}x\n\n{credit}");
            assert!(validate_message(too_long.as_bytes(), &identity()).is_err());
        }
    }

    #[test]
    fn other_final_trailers_stay_counted_in_either_order() {
        let content = format!("fix: {}", "x".repeat(63));
        let signoff = "Signed-off-by: tester <44+tester@users.noreply.github.com>";
        let credit = "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>";
        assert_eq!(format!("{content}\n\n{signoff}").len(), 128);
        for trailers in [
            format!("{credit}\n{signoff}"),
            format!("{signoff}\n{credit}"),
        ] {
            assert!(
                validate_message(format!("{content}\n\n{trailers}").as_bytes(), &identity())
                    .is_ok()
            );
            assert!(
                validate_message(format!("{content}x\n\n{trailers}").as_bytes(), &identity())
                    .is_err()
            );
        }
    }

    #[test]
    fn prose_and_malformed_coauthor_lines_are_not_exempt() {
        let content = format!("fix: {}", "x".repeat(123));
        for suffix in [
            "\nCo-Authored-By: Codex",
            "\n\nCo-Authored-By: Codex\nbody follows",
            "\n\nCo-Authrored-By: Codex",
            "\n\n Co-Authored-By: Codex",
            "\n\nCo-Authored-By : Codex",
            "\n\nCo-Authored-By: Codex ",
            "\n\nCo-Authored-By: Codex\n extra prose",
        ] {
            assert!(
                validate_message(format!("{content}{suffix}").as_bytes(), &identity()).is_err(),
                "{suffix:?}"
            );
        }
    }

    #[test]
    fn exemption_never_hides_invalid_original_credits_or_ascii() {
        for credit in [
            "Co-Authored-By: Other <other@example.com>",
            "Co-Authored-By: Claude <invented@example.com>",
            "Co-Authored-By: Codex\r",
            "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>\r",
            "Co-Authored-By: Claude 日本語 <noreply@anthropic.com>",
        ] {
            assert!(
                validate_message(format!("fix: valid\n\n{credit}").as_bytes(), &identity())
                    .is_err(),
                "{credit}"
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
