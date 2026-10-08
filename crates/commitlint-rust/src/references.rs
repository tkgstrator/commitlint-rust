//! Reference extraction for the supported native parser options.
use crate::{
    Result,
    parser::{ParserOptions, Reference, is_js_whitespace},
};
use regex::Regex;
use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, OnceLock},
};

const WS: &str = r"[\t\n\x0B\x0C\r \x{A0}\x{1680}\x{2000}-\x{200A}\x{2028}\x{2029}\x{202F}\x{205F}\x{3000}\x{FEFF}]";
const DOT: &str = r"[^\n\r\x{2028}\x{2029}]";

fn word(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}
fn is_dot(c: char) -> bool {
    !matches!(c, '\n' | '\r' | '\u{2028}' | '\u{2029}')
}

// RegExp without the `u` flag canonicalizes individual UTF-16 code units.
fn canonical(unit: u16) -> u16 {
    let Some(c) = char::from_u32(unit as u32) else {
        return unit;
    };
    let Some((upper, _)) = crate::unicode_properties::mapping(c) else {
        return unit;
    };
    let mut units = upper.encode_utf16();
    let Some(mapped) = units.next() else {
        return unit;
    };
    if units.next().is_some() || (unit >= 128 && mapped < 128) {
        unit
    } else {
        mapped
    }
}

pub(crate) fn case_equal(a: &str, b: &str) -> bool {
    a.encode_utf16()
        .map(canonical)
        .eq(b.encode_utf16().map(canonical))
}

fn case_pattern(value: &str) -> String {
    // Existing regex tables use Unicode 16. Build the literal /i equivalence
    // classes from our frozen Node 17 map instead of that engine's folding.
    static CLASSES: OnceLock<HashMap<u16, Vec<char>>> = OnceLock::new();
    let classes = CLASSES.get_or_init(|| {
        let mut result: HashMap<u16, Vec<char>> = HashMap::new();
        for unit in 0..=u16::MAX {
            let Some(c) = char::from_u32(unit as u32) else {
                continue;
            };
            let key = canonical(unit);
            if key != unit {
                result
                    .entry(key)
                    .or_insert_with(|| vec![char::from_u32(key as u32).unwrap()])
                    .push(c);
            }
        }
        result
    });
    let mut result = String::new();
    for c in value.chars() {
        if c.len_utf16() == 1
            && let Some(group) = classes.get(&canonical(c as u16))
        {
            result.push_str("(?:");
            result.push_str(
                &group
                    .iter()
                    .map(|c| regex::escape(&c.to_string()))
                    .collect::<Vec<_>>()
                    .join("|"),
            );
            result.push(')');
        } else {
            result.push_str(&regex::escape(&c.to_string()));
        }
    }
    result
}

fn normalized(values: &[String]) -> Vec<String> {
    values
        .iter()
        .map(|s| s.trim_matches(is_js_whitespace))
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .collect()
}

pub(crate) struct ReferenceParser {
    parts: Regex,
    prefixes: Vec<String>,
    actions: Vec<String>,
    sensitive: bool,
    url: Regex,
}

impl ReferenceParser {
    /// Cache only the immutable engine for exact default reference options.
    /// Raw text, extracted references and header/body/footer state stay local.
    pub(crate) fn cached(opts: &ParserOptions) -> Result<Arc<Self>> {
        static DEFAULT: OnceLock<(ParserOptions, Arc<ReferenceParser>)> = OnceLock::new();
        let (defaults, parser) = DEFAULT.get_or_init(|| {
            let defaults = ParserOptions::default();
            let parser =
                Self::new(&defaults).expect("constant default reference options are valid");
            (defaults, Arc::new(parser))
        });
        if opts.issue_prefixes == defaults.issue_prefixes
            && opts.issue_prefixes_case_sensitive == defaults.issue_prefixes_case_sensitive
            && opts.reference_actions == defaults.reference_actions
        {
            Ok(Arc::clone(parser))
        } else {
            Self::new(opts).map(Arc::new)
        }
    }
    pub(crate) fn footer_prefix_has_tail(&self, input: &str) -> bool {
        if self.prefixes.is_empty() {
            return input.chars().next().is_some_and(is_dot);
        }
        for prefix in &self.prefixes {
            let count = prefix.chars().count();
            let end = input
                .char_indices()
                .nth(count)
                .map_or(input.len(), |(i, _)| i);
            // Prefix alternatives belong to the complete `prefix .+` match.
            // A long alternative consuming the whole input must give the
            // following shorter alternative an opportunity to match.
            if case_equal(&input[..end], prefix) && input[end..].chars().next().is_some_and(is_dot)
            {
                return true;
            }
        }
        false
    }
    pub(crate) fn new(opts: &ParserOptions) -> Result<Self> {
        let prefixes = normalized(&opts.issue_prefixes);
        let selection = prefixes
            .iter()
            .map(|s| {
                if opts.issue_prefixes_case_sensitive {
                    regex::escape(s)
                } else {
                    case_pattern(s)
                }
            })
            .collect::<Vec<_>>()
            .join("|");
        // Consume the assertion's boundary in the regex, then leave it out of
        // the raw record and next cursor. This preserves regex backtracking
        // before a later prefix when an earlier issue has a bad boundary.
        let pattern = format!(
            r"(?:{DOT}*?)??{WS}*([A-Za-z0-9_\-./]*?)??({selection})([A-Za-z0-9_\-]+)(?:{WS}|[,;.)\]]|$)"
        );
        let parts = Regex::new(&pattern).map_err(|_| "invalid reference parser options")?;
        Ok(Self {
            parts,
            prefixes,
            actions: normalized(&opts.reference_actions),
            sensitive: opts.issue_prefixes_case_sensitive,
            url: Regex::new(r"(?-u:\b)(?:https?)://(?:www\.)?[-a-zA-Z0-9@:%_+.~#?&//=]+(?-u:\b)")
                .unwrap(),
        })
    }

    fn keyword_alternatives_at(&self, input: &str, at: usize) -> Vec<(usize, String)> {
        if input[..at].chars().next_back().is_some_and(word) {
            return Vec::new();
        }
        if self.actions.is_empty() {
            return if !input[at..].chars().next().is_some_and(word) {
                vec![(at, String::new())]
            } else {
                Vec::new()
            };
        }
        let mut alternatives = Vec::new();
        for action in &self.actions {
            // Equal UTF-16-unit lengths need not have equal UTF-8 byte lengths.
            let count = action.chars().count();
            let end = input[at..]
                .char_indices()
                .nth(count)
                .map_or(input.len(), |(i, _)| at + i);
            let candidate = &input[at..end];
            if case_equal(candidate, action) && !input[end..].chars().next().is_some_and(word) {
                alternatives.push((end, candidate.to_owned()));
            }
        }
        alternatives
    }

    pub(crate) fn parse(&self, input: &str, footer: bool) -> Vec<Reference> {
        let mut keywords = Vec::new();
        for at in input
            .char_indices()
            .map(|(i, _)| i)
            .chain(std::iter::once(input.len()))
        {
            // Keep lexical boundaries for the next-keyword lookahead, even
            // when a candidate's full separator/whitespace match fails. Each
            // alternative still gets a chance to complete that actual match.
            keywords.extend(
                self.keyword_alternatives_at(input, at)
                    .into_iter()
                    .map(|(end, action)| (at, end, action)),
            );
        }
        let mut chunks = Vec::new();
        let mut cursor = 0;
        for (index, (start, end, action)) in keywords.iter().enumerate() {
            if *start < cursor {
                continue;
            }
            let mut after = *end;
            if footer && input[after..].starts_with(':') {
                after += 1;
            }
            let whitespace_end = input[after..]
                .char_indices()
                .take_while(|(_, c)| is_js_whitespace(*c))
                .last()
                .map(|(i, c)| after + i + c.len_utf8());
            let Some(begin) = whitespace_end else {
                continue;
            };
            let finish = keywords[index + 1..]
                .iter()
                .find(|(at, _, _)| *at >= begin)
                .map_or(input.len(), |(at, _, _)| *at);
            if !input[begin..finish].chars().all(is_dot) {
                continue;
            }
            chunks.push((
                &input[begin..finish],
                if action.is_empty() {
                    None
                } else {
                    Some(action.clone())
                },
            ));
            cursor = finish;
        }
        if chunks.is_empty() {
            let mut begin = 0;
            for (at, c) in input.char_indices() {
                if !is_dot(c) {
                    if begin < at {
                        chunks.push((&input[begin..at], None));
                    }
                    begin = at + c.len_utf8();
                }
            }
            if begin < input.len() {
                chunks.push((&input[begin..], None));
            }
        }
        let mut result = Vec::new();
        for (chunk, action) in chunks {
            // The upstream parser excludes a whole reference sentence when
            // that sentence contains an HTTP URL.
            if self.url.is_match(chunk) {
                continue;
            }
            let mut offset = 0;
            while let Some(caps) = self.parts.captures_at(chunk, offset) {
                let matched = caps.get(0).unwrap();
                let prefix = caps.get(2).unwrap().as_str();
                let prefix_allowed = self.sensitive
                    || self.prefixes.is_empty()
                    || self.prefixes.iter().any(|s| case_equal(prefix, s));
                if !prefix_allowed {
                    if matched.start() == chunk.len() {
                        break;
                    }
                    offset = matched.start()
                        + chunk[matched.start()..].chars().next().unwrap().len_utf8();
                    continue;
                }
                let mut repository = caps
                    .get(1)
                    .map(|m| m.as_str())
                    .filter(|s| !s.is_empty())
                    .map(str::to_owned);
                let mut owner = None;
                if let Some(repo) = &repository
                    && let Some(slash) = repo.find('/')
                {
                    owner = Some(repo[..slash].to_owned());
                    repository = Some(repo[slash + 1..].to_owned());
                }
                let issue = caps.get(3).unwrap();
                result.push(Reference {
                    raw: chunk[matched.start()..issue.end()].to_owned(),
                    action: action.clone(),
                    owner,
                    repository,
                    prefix: prefix.to_owned(),
                    issue: issue.as_str().to_owned(),
                });
                offset = issue.end();
            }
        }
        result
    }
}

pub(crate) fn deduplicate(references: &mut Vec<Reference>) {
    let mut seen = HashSet::new();
    references.retain(|reference| {
        seen.insert(crate::case::js_lowercase(&format!(
            "{} {}",
            reference.action.as_deref().unwrap_or("null"),
            reference.raw
        )))
    });
}
