//! Runtime-independent implementation of the pinned commitlint policy subset.
use crate::Result;
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
