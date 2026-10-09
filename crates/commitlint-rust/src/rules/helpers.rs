use super::types::{RuleCondition, RuleOutcome};
use super::values::Limit;
use crate::parser::{is_js_whitespace, js_len, split_lines};
use regex::Regex;
use std::sync::OnceLock;

pub(super) fn outcome(valid: bool, message: impl Into<String>) -> RuleOutcome {
    RuleOutcome {
        valid,
        message: Some(message.into()),
    }
}

pub(super) fn pass() -> RuleOutcome {
    RuleOutcome {
        valid: true,
        message: None,
    }
}

/// @commitlint/message: drops falsy (empty) parts, joins with a space.
pub(super) fn message(parts: &[Option<&str>]) -> String {
    parts
        .iter()
        .flatten()
        .copied()
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

pub(super) fn negated(when: Option<RuleCondition>) -> bool {
    when == Some(RuleCondition::Never)
}

pub(super) fn falsy(field: &Option<String>) -> Option<&str> {
    field.as_deref().filter(|s| !s.is_empty())
}

// ---------------------------------------------------------------- values

fn url_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    // /\bhttps?:\/\/\S+/ with ASCII \b and JavaScript \S.
    RE.get_or_init(|| {
        Regex::new(
            "(?-u:\\b)https?://[^ \\t\\n\\x0B\\x0C\\r\\x{A0}\\x{1680}\\x{2000}-\\x{200A}\\x{2028}\\x{2029}\\x{202F}\\x{205F}\\x{3000}\\x{FEFF}]",
        )
        .unwrap()
    })
}

/// @commitlint/ensure `maxLineLength`: URL-bearing lines are exempt.
pub(super) fn max_line_length(value: &str, max: Limit) -> bool {
    split_lines(value)
        .iter()
        .all(|line| url_regex().is_match(line) || max.max_ok(js_len(line)))
}

pub(super) fn bang_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    // /^(\w*)(?:\((.*)\))?!: (.*)$/ with ASCII \w and JavaScript `.`.
    RE.get_or_init(|| {
        let dot = r"[^\n\r\x{2028}\x{2029}]";
        Regex::new(&format!(r"^[A-Za-z0-9_]*(?:\({dot}*\))?!: {dot}*$")).unwrap()
    })
}

/// /^BREAKING[ -]CHANGE:/m (case sensitive, JavaScript line terminators).
pub(super) fn has_breaking_footer(footer: &str) -> bool {
    let matches_at = |rest: &str| {
        rest.strip_prefix("BREAKING")
            .and_then(|r| r.strip_prefix([' ', '-']))
            .and_then(|r| r.strip_prefix("CHANGE:"))
            .is_some()
    };
    if matches_at(footer) {
        return true;
    }
    footer.char_indices().any(|(i, c)| {
        matches!(c, '\n' | '\r' | '\u{2028}' | '\u{2029}')
            && matches_at(&footer[i + c.len_utf8()..])
    })
}

pub(super) fn leading_is_blank(lines: &[&str], start: isize) -> bool {
    // JavaScript Array.prototype.slice(start)[0]
    let len = lines.len() as isize;
    let index = if start < 0 {
        (len + start).max(0)
    } else {
        start
    };
    lines
        .get(index as usize)
        .is_some_and(|line| line.is_empty())
}

/// `input[input.length - 1] === value`, comparing UTF-16 units.
pub(super) fn last_unit_is(input: &str, value: &str) -> bool {
    input.encode_utf16().last().is_some_and(|last| {
        let mut units = value.encode_utf16();
        units.next() == Some(last) && units.next().is_none()
    })
}

/// /^\(cherry picked from commit [0-9a-f]{7,64}\)$/i (ASCII case-insensitive).
pub(super) fn is_cherry_pick(line: &str) -> bool {
    const PREFIX: &str = "(cherry picked from commit ";
    let line = line.trim_matches(is_js_whitespace);
    if line.len() < PREFIX.len() || !line.is_char_boundary(PREFIX.len()) {
        return false;
    }
    let (head, rest) = line.split_at(PREFIX.len());
    let Some(hex) = rest.strip_suffix(')') else {
        return false;
    };
    head.eq_ignore_ascii_case(PREFIX)
        && (7..=64).contains(&hex.len())
        && hex.bytes().all(|b| b.is_ascii_hexdigit())
}
