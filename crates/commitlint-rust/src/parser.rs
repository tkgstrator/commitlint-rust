//! Bounded port of conventional-commits-parser 7.1.3 (pinned) covering only
//! the six fields `header`, `type`, `scope`, `subject`, `body` and `footer`
//! with the pinned conventional-changelog Angular / Conventional Commits
//! preset defaults and `fieldPattern: null` (as @commitlint/parse sets it).
//! No JS plugins, custom parsers, references, mentions, reverts or merges.
use crate::Result;
use crate::references::{self, ReferenceParser};
use regex::Regex;
use std::sync::{Arc, OnceLock};

const SCISSOR: &str = "------------------------ >8 ------------------------";
// JS `.` excludes these line terminators.
const DOT: &str = r"[^\n\r\x{2028}\x{2029}]";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParserPreset {
    Angular,
    ConventionalCommits,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ParsedMessage {
    pub header: Option<String>,
    pub r#type: Option<String>,
    pub scope: Option<String>,
    pub subject: Option<String>,
    pub body: Option<String>,
    pub footer: Option<String>,
    pub references: Vec<Reference>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Reference {
    pub action: Option<String>,
    pub owner: Option<String>,
    pub repository: Option<String>,
    pub issue: String,
    pub prefix: String,
    pub raw: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParserOptions {
    pub preset: ParserPreset,
    pub comment_char: Option<char>,
    pub issue_prefixes: Vec<String>,
    pub issue_prefixes_case_sensitive: bool,
    pub reference_actions: Vec<String>,
}

impl Default for ParserOptions {
    fn default() -> Self {
        Self {
            preset: ParserPreset::Angular,
            comment_char: None,
            issue_prefixes: vec!["#".into()],
            issue_prefixes_case_sensitive: false,
            reference_actions: [
                "close", "closes", "closed", "fix", "fixes", "fixed", "resolve", "resolves",
                "resolved",
            ]
            .into_iter()
            .map(str::to_owned)
            .collect(),
        }
    }
}

/// JavaScript `\s` / `String.prototype.trim` whitespace.
pub(crate) fn is_js_whitespace(c: char) -> bool {
    matches!(
        c,
        '\t' | '\n' | '\u{b}' | '\u{c}' | '\r' | ' ' | '\u{a0}' | '\u{1680}' | '\u{2000}'
            ..='\u{200a}'
                | '\u{2028}'
                | '\u{2029}'
                | '\u{202f}'
                | '\u{205f}'
                | '\u{3000}'
                | '\u{feff}'
    )
}

/// JavaScript string length (UTF-16 code units).
pub(crate) fn js_len(s: &str) -> usize {
    s.encode_utf16().count()
}

/// `input.split(/\r?\n/)`; always yields at least one element.
pub(crate) fn split_lines(s: &str) -> Vec<&str> {
    let mut lines = s.split('\n').peekable();
    std::iter::from_fn(|| {
        let line = lines.next()?;
        Some(if lines.peek().is_some() {
            line.strip_suffix('\r').unwrap_or(line)
        } else {
            line
        })
    })
    .collect()
}

fn trim_new_lines(s: &str) -> &str {
    s.trim_matches(|c| c == '\r' || c == '\n')
}

fn is_dot(c: char) -> bool {
    !matches!(c, '\n' | '\r' | '\u{2028}' | '\u{2029}')
}

fn compile(bang: &str) -> Regex {
    Regex::new(&format!(
        r"^([A-Za-z0-9_]*)(?:\(({dot}*)\))?{bang}: ({dot}*)$",
        dot = DOT,
        bang = bang
    ))
    .unwrap()
}

fn angular_header() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| compile(""))
}

fn conventional_header() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| compile("!?"))
}

fn breaking_header() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| compile("!"))
}

type HeaderFields = (Option<String>, Option<String>, Option<String>);

fn match_header(header: &str, preset: ParserPreset) -> Option<HeaderFields> {
    let caps = match preset {
        ParserPreset::Angular => angular_header().captures(header),
        // breakingHeaderPattern is tried before headerPattern.
        ParserPreset::ConventionalCommits => breaking_header()
            .captures(header)
            .or_else(|| conventional_header().captures(header)),
    }?;
    let group = |i: usize| {
        caps.get(i)
            .map(|m| m.as_str())
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
    };
    Some((group(1), group(2), group(3)))
}

fn is_gpg(line: &str) -> bool {
    line.trim_start_matches(is_js_whitespace)
        .starts_with("gpg:")
}

/// /^(?:BREAKING CHANGE|[\w-]+)(?::\s+|\s+(?:#)).+/i
fn is_footer_token(line: &str, references: &ReferenceParser) -> bool {
    let mut ends = Vec::new();
    let prefix = "BREAKING CHANGE";
    if line.len() >= prefix.len()
        && line.is_char_boundary(prefix.len())
        && line[..prefix.len()].eq_ignore_ascii_case(prefix)
    {
        ends.push(prefix.len());
    }
    let run = line
        .char_indices()
        .find(|(_, c)| !(c.is_ascii_alphanumeric() || *c == '_' || *c == '-'))
        .map_or(line.len(), |(i, _)| i);
    if run > 0 {
        ends.push(run);
    }
    ends.iter()
        .any(|end| footer_separator(&line[*end..], references))
}

fn footer_separator(rest: &str, references: &ReferenceParser) -> bool {
    if let Some(after) = rest.strip_prefix(':') {
        let chars: Vec<char> = after.chars().collect();
        let spaces = chars.iter().take_while(|c| is_js_whitespace(**c)).count();
        // `\s+` may give characters back to the trailing `.+`.
        for keep in 1..=spaces {
            if chars.get(keep).is_some_and(|c| is_dot(*c)) {
                return true;
            }
        }
    }
    let mut offset = 0;
    for c in rest.chars().take_while(|c| is_js_whitespace(*c)) {
        offset += c.len_utf8();
        if references.footer_prefix_has_tail(&rest[offset..]) {
            return true;
        }
    }
    false
}

/// /^(?:\*\s+)?(KEYWORDS):\s*(.*)/i
fn is_note(line: &str, preset: ParserPreset) -> bool {
    let keywords: &[&str] = match preset {
        ParserPreset::Angular => &["BREAKING CHANGE"],
        ParserPreset::ConventionalCommits => &["BREAKING CHANGE", "BREAKING-CHANGE"],
    };
    let starts = |s: &str| {
        keywords.iter().any(|kw| {
            s.len() > kw.len()
                && s.is_char_boundary(kw.len())
                && s[..kw.len()].eq_ignore_ascii_case(kw)
                && s[kw.len()..].starts_with(':')
        })
    };
    if starts(line) {
        return true;
    }
    if let Some(after) = line.strip_prefix('*') {
        let trimmed = after.trim_start_matches(is_js_whitespace);
        return trimmed.len() < after.len() && starts(trimmed);
    }
    false
}

/// JS `appendLine`: a falsy (absent or empty) source is replaced by the line.
fn append_line(src: &mut Option<String>, line: &str) {
    match src {
        Some(s) if !s.is_empty() => {
            s.push('\n');
            s.push_str(line);
        }
        _ => *src = Some(line.to_owned()),
    }
}

struct State<'a> {
    lines: Vec<&'a str>,
    index: usize,
    preset: ParserPreset,
    body: Option<String>,
    footer: Option<String>,
    references: Vec<Reference>,
    reference_parser: Arc<ReferenceParser>,
}

impl State<'_> {
    fn available(&self) -> bool {
        self.index < self.lines.len()
    }

    fn parse_notes(&mut self) -> bool {
        if !self.available() || !is_note(self.lines[self.index], self.preset) {
            return false;
        }
        append_line(&mut self.footer, self.lines[self.index]);
        self.index += 1;
        while self.available() {
            let line = self.lines[self.index];
            if is_note(line, self.preset) {
                append_line(&mut self.footer, line);
                self.index += 1;
                continue;
            }
            let token = is_footer_token(line, &self.reference_parser);
            self.references
                .extend(self.reference_parser.parse(line, token));
            append_line(&mut self.footer, line);
            self.index += 1;
            if token {
                break;
            }
        }
        true
    }

    fn parse_body_and_footer(&mut self, is_body: bool) -> bool {
        if !self.available() {
            return is_body;
        }
        let line = self.lines[self.index];
        let token = is_footer_token(line, &self.reference_parser);
        self.references
            .extend(self.reference_parser.parse(line, token));
        let still_body = !token && is_body;
        if still_body {
            append_line(&mut self.body, line);
        } else {
            append_line(&mut self.footer, line);
        }
        self.index += 1;
        still_body
    }
}

pub fn parse_message(raw: &str, preset: ParserPreset) -> Result<ParsedMessage> {
    parse_message_with_comment_char(raw, preset, None)
}

/// `comment_char` mirrors the parser's `commentChar` option (used by a few
/// upstream rule tests); the fixed policy never sets it.
pub fn parse_message_with_comment_char(
    raw: &str,
    preset: ParserPreset,
    comment_char: Option<char>,
) -> Result<ParsedMessage> {
    parse_message_with_options(
        raw,
        &ParserOptions {
            preset,
            comment_char,
            ..ParserOptions::default()
        },
    )
}

pub fn parse_message_with_options(raw: &str, options: &ParserOptions) -> Result<ParsedMessage> {
    if raw.trim_matches(is_js_whitespace).is_empty() {
        return Err("Expected a raw commit".into());
    }
    let raw_lines = split_lines(trim_new_lines(raw));
    let lines: Vec<&str> = match options.comment_char {
        Some(c) => {
            let scissor = format!("{c} {SCISSOR}");
            let end = raw_lines
                .iter()
                .position(|line| *line == scissor.as_str())
                .unwrap_or(raw_lines.len());
            raw_lines[..end]
                .iter()
                .copied()
                .filter(|line| !line.starts_with(c) && !is_gpg(line))
                .collect()
        }
        None => raw_lines.into_iter().filter(|line| !is_gpg(line)).collect(),
    };
    let header = lines.first().copied().filter(|line| !line.is_empty());
    let mut parsed = ParsedMessage {
        header: header.map(str::to_owned),
        ..ParsedMessage::default()
    };
    if let Some((kind, scope, subject)) = header.and_then(|h| match_header(h, options.preset)) {
        parsed.r#type = kind;
        parsed.scope = scope;
        parsed.subject = subject;
    }
    let mut state = State {
        lines,
        index: 1,
        preset: options.preset,
        body: None,
        footer: None,
        references: Vec::new(),
        reference_parser: ReferenceParser::cached(options)?,
    };
    if let Some(header) = &parsed.header {
        state
            .references
            .extend(state.reference_parser.parse(header, false));
    }
    let mut is_body = true;
    while state.available() {
        if state.parse_notes() {
            is_body = false;
        }
        if !state.parse_body_and_footer(is_body) {
            is_body = false;
        }
    }
    // cleanupCommit: `&&=` leaves an empty (falsy) string untouched.
    let clean = |value: Option<String>| {
        value.map(|v| {
            if v.is_empty() {
                v
            } else {
                trim_new_lines(&v).to_owned()
            }
        })
    };
    parsed.body = clean(state.body);
    parsed.footer = clean(state.footer);
    references::deduplicate(&mut state.references);
    parsed.references = state.references;
    Ok(parsed)
}
