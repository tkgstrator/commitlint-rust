//! The 38 upstream @commitlint/rules (21.2.3) over the bounded parse, with
//! upstream defaults, `always`/`never` semantics, JavaScript UTF-16 lengths and
//! delimiter splitting. Case behavior lives in `crate::case`.
use crate::Result;
use crate::case::{ensure_case, is_target_case, subject_gate};
use crate::parser::{ParsedMessage, is_js_whitespace, js_len, split_lines};
use regex::Regex;
use std::sync::OnceLock;
use std::{path::PathBuf, time::Duration};

/// Exported rule names in upstream `index.ts` order.
pub const SUPPORTED_RULES: [&str; 38] = [
    "body-case",
    "body-empty",
    "body-full-stop",
    "body-leading-blank",
    "body-max-length",
    "body-max-line-length",
    "body-min-length",
    "breaking-change-exclamation-mark",
    "footer-empty",
    "footer-leading-blank",
    "footer-max-length",
    "footer-max-line-length",
    "footer-min-length",
    "header-case",
    "header-full-stop",
    "header-max-length",
    "header-min-length",
    "header-trim",
    "references-empty",
    "scope-case",
    "scope-delimiter-style",
    "scope-empty",
    "scope-enum",
    "scope-max-length",
    "scope-min-length",
    "signed-off-by",
    "subject-case",
    "subject-empty",
    "subject-exclamation-mark",
    "subject-full-stop",
    "subject-max-length",
    "subject-min-length",
    "trailer-exists",
    "type-case",
    "type-empty",
    "type-enum",
    "type-max-length",
    "type-min-length",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RuleCondition {
    Always,
    Never,
}

#[derive(Clone, Debug, PartialEq)]
pub enum RuleValue {
    /// Upstream `undefined` (not `null`): the rule default applies.
    None,
    Length(usize),
    Text(String),
    List(Vec<String>),
    Number(f64),
    CaseChecks(Vec<CaseCheck>),
    ScopeEnum {
        scopes: Vec<String>,
        delimiters: Vec<String>,
    },
    ScopeCases {
        cases: Vec<CaseCheck>,
        delimiters: Vec<String>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CaseCheck {
    pub target: String,
    pub when: Option<RuleCondition>,
}

#[derive(Clone, Debug)]
pub struct EvaluationContext {
    pub git: Option<PathBuf>,
    pub cwd: Option<PathBuf>,
    pub timeout: Duration,
    pub output_limit: usize,
}

impl Default for EvaluationContext {
    fn default() -> Self {
        Self {
            git: None,
            cwd: None,
            timeout: Duration::from_secs(10),
            output_limit: 64 * 1024 * 1024,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuleOutcome {
    pub valid: bool,
    /// `None` where upstream returns a bare `[true]` without a message.
    pub message: Option<String>,
}

fn outcome(valid: bool, message: impl Into<String>) -> RuleOutcome {
    RuleOutcome {
        valid,
        message: Some(message.into()),
    }
}

fn pass() -> RuleOutcome {
    RuleOutcome {
        valid: true,
        message: None,
    }
}

/// @commitlint/message: drops falsy (empty) parts, joins with a space.
fn message(parts: &[Option<&str>]) -> String {
    parts
        .iter()
        .flatten()
        .copied()
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

fn negated(when: Option<RuleCondition>) -> bool {
    when == Some(RuleCondition::Never)
}

fn falsy(field: &Option<String>) -> Option<&str> {
    field.as_deref().filter(|s| !s.is_empty())
}

// ---------------------------------------------------------------- values

/// A numeric limit: exact for integers, IEEE-754 for direct float values.
#[derive(Clone, Copy, Debug)]
enum Limit {
    Int(usize),
    Float(f64),
}

impl Limit {
    fn max_ok(self, len: usize) -> bool {
        match self {
            Limit::Int(n) => len <= n,
            Limit::Float(f) => (len as f64) <= f,
        }
    }

    fn min_ok(self, len: usize) -> bool {
        match self {
            Limit::Int(n) => len >= n,
            Limit::Float(f) => (len as f64) >= f,
        }
    }

    /// JavaScript template-literal rendering of the number.
    fn show(self) -> String {
        match self {
            Limit::Int(n) => n.to_string(),
            Limit::Float(f) => ryu_js::Buffer::new().format(f).to_owned(),
        }
    }
}

fn no_value(name: &str, value: &RuleValue) -> Result<()> {
    match value {
        RuleValue::None => Ok(()),
        _ => Err(format!("rule {name} does not accept a value")),
    }
}

fn limit_value(name: &str, value: &RuleValue) -> Result<Limit> {
    match value {
        RuleValue::None => Ok(Limit::Int(0)),
        RuleValue::Length(n) => Ok(Limit::Int(*n)),
        RuleValue::Number(f) => Ok(Limit::Float(*f)),
        _ => Err(format!("rule {name} requires a numeric value")),
    }
}

fn text_value<'a>(name: &str, value: &'a RuleValue, default: &'a str) -> Result<&'a str> {
    match value {
        RuleValue::None => Ok(default),
        RuleValue::Text(text) => Ok(text.as_str()),
        _ => Err(format!("rule {name} requires a text value")),
    }
}

fn list_value<'a>(name: &str, value: &'a RuleValue) -> Result<&'a [String]> {
    match value {
        RuleValue::None => Ok(&[]),
        RuleValue::List(list) => Ok(list.as_slice()),
        _ => Err(format!("rule {name} requires a list value")),
    }
}

/// Normalizes a case rule value to checks plus (for scope-case) delimiters.
fn case_checks(
    name: &str,
    value: &RuleValue,
    allow_object: bool,
) -> Result<(Vec<CaseCheck>, Vec<String>)> {
    let always = |target: &String| CaseCheck {
        target: target.clone(),
        when: None,
    };
    let (checks, delimiters) = match value {
        RuleValue::None => (Vec::new(), Vec::new()),
        RuleValue::Text(text) => (vec![always(text)], Vec::new()),
        RuleValue::List(list) => (list.iter().map(always).collect(), Vec::new()),
        RuleValue::CaseChecks(checks) => (checks.clone(), Vec::new()),
        RuleValue::ScopeCases { cases, delimiters } if allow_object => {
            (cases.clone(), delimiters.clone())
        }
        _ => return Err(format!("rule {name} requires a case value")),
    };
    for check in &checks {
        if !is_target_case(&check.target) {
            return Err(format!(
                "rule {name}: unknown case target \"{}\"",
                check.target
            ));
        }
    }
    Ok((checks, delimiters))
}

/// Checks that `name` is a supported rule and `value` has the type it takes,
/// before any message data is looked at.
pub fn validate_rule_value(name: &str, value: &RuleValue) -> Result<()> {
    match name {
        "body-empty"
        | "footer-empty"
        | "subject-empty"
        | "type-empty"
        | "scope-empty"
        | "references-empty"
        | "body-leading-blank"
        | "footer-leading-blank"
        | "header-trim"
        | "breaking-change-exclamation-mark"
        | "subject-exclamation-mark" => no_value(name, value),
        "body-max-length"
        | "body-min-length"
        | "body-max-line-length"
        | "footer-max-length"
        | "footer-min-length"
        | "footer-max-line-length"
        | "header-max-length"
        | "header-min-length"
        | "scope-max-length"
        | "scope-min-length"
        | "subject-max-length"
        | "subject-min-length"
        | "type-max-length"
        | "type-min-length" => limit_value(name, value).map(drop),
        "body-full-stop" | "header-full-stop" | "subject-full-stop" | "signed-off-by"
        | "trailer-exists" => text_value(name, value, "").map(drop),
        "type-enum" | "scope-delimiter-style" => list_value(name, value).map(drop),
        "scope-enum" => match value {
            RuleValue::None | RuleValue::List(_) | RuleValue::ScopeEnum { .. } => Ok(()),
            _ => Err(format!("rule {name} requires a list or scope object value")),
        },
        "body-case" | "header-case" | "subject-case" | "type-case" => {
            case_checks(name, value, false).map(drop)
        }
        "scope-case" => case_checks(name, value, true).map(drop),
        _ => Err(format!("unsupported commitlint rule: {name}")),
    }
}

// --------------------------------------------------------------- helpers

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
fn max_line_length(value: &str, max: Limit) -> bool {
    split_lines(value)
        .iter()
        .all(|line| url_regex().is_match(line) || max.max_ok(js_len(line)))
}

fn bang_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    // /^(\w*)(?:\((.*)\))?!: (.*)$/ with ASCII \w and JavaScript `.`.
    RE.get_or_init(|| {
        let dot = r"[^\n\r\x{2028}\x{2029}]";
        Regex::new(&format!(r"^[A-Za-z0-9_]*(?:\({dot}*\))?!: {dot}*$")).unwrap()
    })
}

/// /^BREAKING[ -]CHANGE:/m (case sensitive, JavaScript line terminators).
fn has_breaking_footer(footer: &str) -> bool {
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

fn leading_is_blank(lines: &[&str], start: isize) -> bool {
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
fn last_unit_is(input: &str, value: &str) -> bool {
    input.encode_utf16().last().is_some_and(|last| {
        let mut units = value.encode_utf16();
        units.next() == Some(last) && units.next().is_none()
    })
}

/// /^\(cherry picked from commit [0-9a-f]{7,64}\)$/i (ASCII case-insensitive).
fn is_cherry_pick(line: &str) -> bool {
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

// ------------------------------------------------------ scope delimiters

enum Alt {
    /// `, ?`
    Comma,
    Literal(Vec<u16>),
}

fn alternatives(delimiters: &[String]) -> Vec<Alt> {
    let default = ["/", "\\", ","].map(String::from);
    let chosen: &[String] = if delimiters.is_empty() {
        &default
    } else {
        delimiters
    };
    chosen
        .iter()
        .map(|d| {
            if d == "," {
                Alt::Comma
            } else {
                Alt::Literal(d.encode_utf16().collect())
            }
        })
        .collect()
}

/// Sticky match of the ordered alternation at `at`; returns the match end.
fn match_at(units: &[u16], at: usize, alts: &[Alt]) -> Option<usize> {
    for alt in alts {
        match alt {
            Alt::Comma => {
                if units.get(at) == Some(&(b',' as u16)) {
                    let mut end = at + 1;
                    if units.get(end) == Some(&(b' ' as u16)) {
                        end += 1;
                    }
                    return Some(end);
                }
            }
            Alt::Literal(lit) => {
                if units[at..].starts_with(lit) {
                    return Some(at + lit.len());
                }
            }
        }
    }
    None
}

/// `regex.test(segment)`: a match anywhere (an empty alternative always hits).
fn contains_match(units: &[u16], alts: &[Alt]) -> bool {
    (0..=units.len()).any(|at| match_at(units, at, alts).is_some())
}

/// `String.prototype.split(regex)` for a non-empty input (ES `@@split`).
fn split_units(units: &[u16], alts: &[Alt]) -> Vec<Vec<u16>> {
    let size = units.len();
    let mut parts = Vec::new();
    let (mut p, mut q) = (0, 0);
    while q < size {
        match match_at(units, q, alts) {
            None => q += 1,
            Some(end) => {
                let e = end.min(size);
                if e == p {
                    q += 1;
                } else {
                    parts.push(units[p..q].to_vec());
                    p = e;
                    q = p;
                }
            }
        }
    }
    parts.push(units[p..].to_vec());
    parts
}

/// Unpaired surrogates (from splitting inside an astral scalar with an empty
/// delimiter) cannot be a Rust string; they become U+FFFD, which every case
/// target treats as the same caseless non-word symbol.
fn segment_text(units: &[u16]) -> String {
    String::from_utf16_lossy(units)
}

fn scope_delimiters(value: &RuleValue) -> &[String] {
    match value {
        RuleValue::ScopeEnum { delimiters, .. } | RuleValue::ScopeCases { delimiters, .. } => {
            delimiters
        }
        _ => &[],
    }
}

// ------------------------------------------------------------- case rules

fn case_outcome(
    label: &str,
    when: Option<RuleCondition>,
    checks: &[CaseCheck],
    mut holds: impl FnMut(&str) -> Result<bool>,
) -> Result<RuleOutcome> {
    let mut matches = Vec::new();
    for check in checks {
        let r = holds(&check.target)?;
        if if negated(check.when) { !r } else { r } {
            matches.push(check);
        }
    }
    let result = !matches.is_empty();
    let neg = negated(when);
    // A `never` rule fails because a case matched, so report the case(s) that
    // did; an `always` rule keeps reporting every configured case.
    let reported: Vec<&CaseCheck> = if neg && result {
        matches
    } else {
        checks.iter().collect()
    };
    let list = reported
        .iter()
        .map(|c| c.target.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    Ok(outcome(
        if neg { !result } else { result },
        message(&[
            Some(format!("{label} must").as_str()),
            neg.then_some("not"),
            Some(format!("be {list}").as_str()),
        ]),
    ))
}

fn simple_case(
    label: &str,
    text: &str,
    when: Option<RuleCondition>,
    checks: &[CaseCheck],
) -> Result<RuleOutcome> {
    case_outcome(label, when, checks, |target| ensure_case(text, target))
}

// ------------------------------------------------------------- evaluation

/// Evaluate one upstream rule with the default evaluation context. `raw` is
/// the unparsed message handed to the parser.
pub fn evaluate_rule(
    name: &str,
    parsed: &ParsedMessage,
    raw: &str,
    when: Option<RuleCondition>,
    value: &RuleValue,
) -> Result<RuleOutcome> {
    evaluate_rule_with_context(
        name,
        parsed,
        raw,
        when,
        value,
        &EvaluationContext::default(),
    )
}

pub fn evaluate_rule_with_context(
    name: &str,
    parsed: &ParsedMessage,
    raw: &str,
    when: Option<RuleCondition>,
    value: &RuleValue,
    context: &EvaluationContext,
) -> Result<RuleOutcome> {
    validate_rule_value(name, value)?;
    let neg = negated(when);
    let must = |neg: bool| if neg { "may not" } else { "must" };
    match name {
        "type-empty" | "subject-empty" | "body-empty" | "footer-empty" => {
            let (label, field) = match name {
                "type-empty" => ("type", &parsed.r#type),
                "subject-empty" => ("subject", &parsed.subject),
                "body-empty" => ("body", &parsed.body),
                _ => ("footer", &parsed.footer),
            };
            let not_empty = falsy(field).is_some();
            Ok(outcome(
                if neg { not_empty } else { !not_empty },
                message(&[Some(label), Some(must(neg)), Some("be empty")]),
            ))
        }
        // These two default to `never`: `always` is the negated form.
        "scope-empty" | "references-empty" => {
            let always = when == Some(RuleCondition::Always);
            let (label, not_empty) = if name == "scope-empty" {
                ("scope", falsy(&parsed.scope).is_some())
            } else {
                ("references", !parsed.references.is_empty())
            };
            Ok(outcome(
                if always { !not_empty } else { not_empty },
                message(&[
                    Some(label),
                    Some(if always { "must" } else { "may not" }),
                    Some("be empty"),
                ]),
            ))
        }
        "type-enum" => {
            let list = list_value(name, value)?;
            let Some(kind) = falsy(&parsed.r#type) else {
                return Ok(pass());
            };
            let found = list.iter().any(|item| item == kind);
            let text = format!("be one of [{}]", list.join(", "));
            Ok(outcome(
                if neg { !found } else { found },
                message(&[Some("type must"), neg.then_some("not"), Some(text.as_str())]),
            ))
        }
        "header-max-length" | "header-min-length" => {
            let limit = limit_value(name, value)?;
            let length = parsed.header.as_deref().map(js_len);
            let max = name == "header-max-length";
            let ok = length.is_some_and(|n| {
                if max {
                    limit.max_ok(n)
                } else {
                    limit.min_ok(n)
                }
            });
            Ok(outcome(
                ok,
                format!(
                    "header must not be {} than {} characters, current length is {}",
                    if max { "longer" } else { "shorter" },
                    limit.show(),
                    length.map_or("undefined".to_owned(), |n| n.to_string())
                ),
            ))
        }
        "body-max-length" | "body-min-length" | "footer-max-length" | "footer-min-length"
        | "scope-max-length" | "scope-min-length" | "subject-max-length" | "subject-min-length"
        | "type-max-length" | "type-min-length" => {
            let limit = limit_value(name, value)?;
            let (label, field) = match &name[..name.find('-').unwrap()] {
                "body" => ("body", &parsed.body),
                "footer" => ("footer", &parsed.footer),
                "scope" => ("scope", &parsed.scope),
                "subject" => ("subject", &parsed.subject),
                _ => ("type", &parsed.r#type),
            };
            let Some(text) = falsy(field) else {
                return Ok(pass());
            };
            let max = name.ends_with("max-length");
            let len = js_len(text);
            Ok(outcome(
                if max {
                    limit.max_ok(len)
                } else {
                    limit.min_ok(len)
                },
                format!(
                    "{label} must not be {} than {} characters",
                    if max { "longer" } else { "shorter" },
                    limit.show()
                ),
            ))
        }
        "body-max-line-length" | "footer-max-line-length" => {
            let limit = limit_value(name, value)?;
            let (label, field) = if name == "body-max-line-length" {
                ("body's", &parsed.body)
            } else {
                ("footer's", &parsed.footer)
            };
            let Some(text) = falsy(field) else {
                return Ok(pass());
            };
            Ok(outcome(
                max_line_length(text, limit),
                format!(
                    "{label} lines must not be longer than {} characters",
                    limit.show()
                ),
            ))
        }
        "body-full-stop" => {
            let stop = text_value(name, value, ".")?;
            let Some(body) = falsy(&parsed.body) else {
                return Ok(pass());
            };
            let has_stop = last_unit_is(body, stop);
            Ok(outcome(
                if neg { !has_stop } else { has_stop },
                message(&[Some("body"), Some(must(neg)), Some("end with full stop")]),
            ))
        }
        "header-full-stop" => {
            let stop = text_value(name, value, ".")?;
            let has_stop = parsed
                .header
                .as_deref()
                .is_some_and(|h| last_unit_is(h, stop));
            Ok(outcome(
                if neg { !has_stop } else { has_stop },
                message(&[Some("header"), Some(must(neg)), Some("end with full stop")]),
            ))
        }
        "subject-full-stop" => {
            let stop = text_value(name, value, ".")?;
            let header = parsed.header.as_deref();
            if let Some(h) = header {
                // header?.indexOf(":") || 0, in UTF-16 units
                if let Some(pos) = h.find(':') {
                    let index = js_len(&h[..pos]);
                    if index > 0 && index == js_len(h) - 1 {
                        return Ok(pass());
                    }
                }
            }
            let has_stop = !header.is_some_and(|h| h.ends_with("..."))
                && header.is_some_and(|h| last_unit_is(h, stop));
            Ok(outcome(
                if neg { !has_stop } else { has_stop },
                message(&[Some("subject"), Some(must(neg)), Some("end with full stop")]),
            ))
        }
        "body-leading-blank" => {
            if falsy(&parsed.body).is_none() {
                return Ok(pass());
            }
            let lines = split_lines(raw);
            let succeeds = leading_is_blank(&lines, 1);
            Ok(outcome(
                if neg { !succeeds } else { succeeds },
                message(&[
                    Some("body"),
                    Some(must(neg)),
                    Some("have leading blank line"),
                ]),
            ))
        }
        "footer-leading-blank" => {
            let Some(footer) = falsy(&parsed.footer) else {
                return Ok(pass());
            };
            let raw_lines = split_lines(raw);
            let first = split_lines(footer)[0];
            let offset = raw_lines
                .iter()
                .position(|line| *line == first)
                .map_or(-1, |i| i as isize);
            let succeeds = leading_is_blank(&raw_lines, offset - 1);
            Ok(outcome(
                if neg { !succeeds } else { succeeds },
                message(&[
                    Some("footer"),
                    Some(must(neg)),
                    Some("have leading blank line"),
                ]),
            ))
        }
        "header-trim" => Ok(header_trim(parsed)),
        "breaking-change-exclamation-mark" => {
            let header = falsy(&parsed.header);
            let footer = falsy(&parsed.footer);
            if header.is_none() && footer.is_none() {
                return Ok(pass());
            }
            let bang = header.is_some_and(|h| bang_regex().is_match(h));
            let breaking = footer.is_some_and(has_breaking_footer);
            let check = bang == breaking;
            Ok(outcome(
                if neg { !check } else { check },
                message(&[
                    Some("breaking changes"),
                    Some(if neg { "must not" } else { "must" }),
                    Some("have both an exclamation mark in the header"),
                    Some("and BREAKING CHANGE in the footer"),
                    Some("to identify a breaking change"),
                ]),
            ))
        }
        "subject-exclamation-mark" => {
            let Some(header) = falsy(&parsed.header) else {
                return Ok(outcome(true, ""));
            };
            let has = bang_regex().is_match(header);
            Ok(outcome(
                if neg { !has } else { has },
                message(&[
                    Some("subject"),
                    Some(if neg { "must not" } else { "must" }),
                    Some("have an exclamation mark in the subject to identify a breaking change"),
                ]),
            ))
        }
        "body-case" => {
            let Some(body) = falsy(&parsed.body) else {
                return Ok(pass());
            };
            let (checks, _) = case_checks(name, value, false)?;
            simple_case("body", body, when, &checks)
        }
        "header-case" => {
            let Some(header) = parsed
                .header
                .as_deref()
                .filter(|h| h.chars().next().is_some_and(|c| c.is_ascii_alphabetic()))
            else {
                return Ok(pass());
            };
            let (checks, _) = case_checks(name, value, false)?;
            simple_case("header", header, when, &checks)
        }
        "subject-case" => {
            let Some(subject) = parsed
                .subject
                .as_deref()
                .filter(|s| s.chars().next().is_some_and(subject_gate))
            else {
                return Ok(pass());
            };
            let (checks, _) = case_checks(name, value, false)?;
            simple_case("subject", subject, when, &checks)
        }
        "type-case" => {
            let Some(kind) = falsy(&parsed.r#type) else {
                return Ok(pass());
            };
            let (checks, _) = case_checks(name, value, false)?;
            simple_case("type", kind, when, &checks)
        }
        "scope-case" => {
            let Some(scope) = falsy(&parsed.scope) else {
                return Ok(pass());
            };
            let (checks, delimiters) = case_checks(name, value, true)?;
            let alts = alternatives(&delimiters);
            let segments = split_units(&scope.encode_utf16().collect::<Vec<_>>(), &alts);
            case_outcome("scope", when, &checks, |target| {
                for segment in &segments {
                    if contains_match(segment, &alts) {
                        continue;
                    }
                    if !ensure_case(&segment_text(segment), target)? {
                        return Ok(false);
                    }
                }
                Ok(true)
            })
        }
        "scope-enum" => {
            let scopes: &[String] = match value {
                RuleValue::List(list) => list,
                RuleValue::ScopeEnum { scopes, .. } => scopes,
                _ => &[],
            };
            let Some(scope) = falsy(&parsed.scope).filter(|_| !scopes.is_empty()) else {
                return Ok(outcome(true, ""));
            };
            let alts = alternatives(scope_delimiters(value));
            let scope_units: Vec<u16> = scope.encode_utf16().collect();
            let segments = split_units(&scope_units, &alts);
            let allowed: Vec<Vec<u16>> =
                scopes.iter().map(|s| s.encode_utf16().collect()).collect();
            let in_enum = |units: &[u16]| allowed.iter().any(|a| a == units);
            let valid = if neg {
                !segments.iter().any(|s| in_enum(s)) && !in_enum(&scope_units)
            } else {
                segments.iter().all(|s| in_enum(s)) || in_enum(&scope_units)
            };
            Ok(outcome(
                valid,
                message(&[
                    Some("scope must"),
                    neg.then_some("not"),
                    Some(format!("be one of [{}]", scopes.join(", ")).as_str()),
                ]),
            ))
        }
        "scope-delimiter-style" => {
            let Some(scope) = falsy(&parsed.scope) else {
                return Ok(pass());
            };
            let configured = list_value(name, value)?;
            let default = ["/", "\\", ","].map(String::from);
            let delimiters: &[String] = if configured.is_empty() {
                &default
            } else {
                configured
            };
            static RAW: OnceLock<Regex> = OnceLock::new();
            let raw_delimiters = RAW.get_or_init(|| Regex::new(r"[^A-Za-z0-9_-]+").unwrap());
            let mut unique: Vec<&str> = Vec::new();
            for found in raw_delimiters.find_iter(scope) {
                let text = found.as_str();
                let text = if text.trim_matches(is_js_whitespace) == "," {
                    ","
                } else {
                    text
                };
                if !unique.contains(&text) {
                    unique.push(text);
                }
            }
            let all_allowed = unique.iter().all(|d| delimiters.iter().any(|x| x == d));
            Ok(outcome(
                if neg { !all_allowed } else { all_allowed },
                format!(
                    "scope delimiters must {}be one of [{}]",
                    if neg { "not " } else { "" },
                    delimiters.join(", ")
                ),
            ))
        }
        "signed-off-by" => {
            let prefix = text_value(name, value, "")?;
            let last = split_lines(raw)
                .into_iter()
                .rfind(|ln| !ln.starts_with('#') && !is_cherry_pick(ln) && !ln.is_empty());
            let signed = last.is_some_and(|ln| ln.starts_with(prefix));
            Ok(outcome(
                if neg { !signed } else { signed },
                message(&[
                    Some("message"),
                    Some(if neg { "must not" } else { "must" }),
                    Some("be signed off"),
                ]),
            ))
        }
        "trailer-exists" => {
            let prefix = text_value(name, value, "")?;
            let trailers = crate::git::interpret_trailers(
                raw,
                context.git.as_deref(),
                context.cwd.as_deref(),
                context.timeout,
                context.output_limit,
            )?;
            let has = split_lines(&trailers)
                .iter()
                .any(|ln| ln.starts_with(prefix));
            Ok(outcome(
                if neg { !has } else { has },
                message(&[
                    Some("message"),
                    Some(if neg { "must not" } else { "must" }),
                    Some(format!("have `{prefix}` trailer").as_str()),
                ]),
            ))
        }
        _ => Err(format!("unsupported commitlint rule: {name}")),
    }
}

/// config-conventional `header-trim`.
pub(crate) fn header_trim(parsed: &ParsedMessage) -> RuleOutcome {
    let Some(header) = falsy(&parsed.header) else {
        return pass();
    };
    let start = header.starts_with(is_js_whitespace);
    let end = header.ends_with(is_js_whitespace);
    match (start, end) {
        (true, true) => outcome(false, "header must not be surrounded by whitespace"),
        (true, false) => outcome(false, "header must not start with whitespace"),
        (false, true) => outcome(false, "header must not end with whitespace"),
        _ => pass(),
    }
}

fn fixed_case(
    name: &str,
    parsed: &ParsedMessage,
    when: Option<RuleCondition>,
    targets: &[&str],
) -> RuleOutcome {
    let value = RuleValue::List(targets.iter().map(|t| (*t).to_owned()).collect());
    evaluate_rule(name, parsed, "", when, &value).unwrap_or_else(|e| outcome(false, e))
}

/// config-conventional `subject-case`: never sentence/start/pascal/upper.
pub(crate) fn subject_case(parsed: &ParsedMessage) -> RuleOutcome {
    fixed_case(
        "subject-case",
        parsed,
        Some(RuleCondition::Never),
        &["sentence-case", "start-case", "pascal-case", "upper-case"],
    )
}

/// config-conventional `type-case`: always lower-case.
pub(crate) fn type_case(parsed: &ParsedMessage) -> RuleOutcome {
    fixed_case(
        "type-case",
        parsed,
        Some(RuleCondition::Always),
        &["lower-case"],
    )
}
