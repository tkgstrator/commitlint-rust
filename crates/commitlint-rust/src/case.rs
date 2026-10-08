//! Commitlint / es-toolkit case behavior frozen to Node 26's Unicode 17 data.

use crate::unicode_properties as unicode;
use unicode_normalization::UnicodeNormalization;

pub fn is_target_case(target: &str) -> bool {
    matches!(
        target,
        "camel-case"
            | "kebab-case"
            | "snake-case"
            | "pascal-case"
            | "start-case"
            | "upper-case"
            | "uppercase"
            | "sentence-case"
            | "sentencecase"
            | "lower-case"
            | "lowercase"
            | "lowerCase"
    )
}

pub fn ensure_case(raw: &str, target: &str) -> Result<bool, String> {
    let stripped = strip_quotes(raw);
    let input = stripped.trim_matches(js_whitespace);
    let transformed = to_case(input, target)?;
    Ok(transformed.is_empty()
        || transformed.starts_with(|ch: char| ch.is_ascii_digit())
        || transformed == input)
}

pub fn subject_gate(first: char) -> bool {
    unicode::classify(first) & unicode::SUBJECT_GATE != 0
}

fn js_whitespace(ch: char) -> bool {
    matches!(ch, '\u{9}'..='\u{d}' | ' ' | '\u{a0}' | '\u{1680}' | '\u{2000}'..='\u{200a}' | '\u{2028}' | '\u{2029}' | '\u{202f}' | '\u{205f}' | '\u{3000}' | '\u{feff}')
}

fn strip_quotes(raw: &str) -> String {
    let mut output = String::with_capacity(raw.len());
    let mut chars = raw.char_indices().peekable();
    while let Some((start, ch)) = chars.next() {
        if matches!(ch, '`' | '"' | '\'') {
            let mut closing = None;
            for (offset, candidate) in raw[start + ch.len_utf8()..].char_indices() {
                if matches!(candidate, '\n' | '\r' | '\u{2028}' | '\u{2029}') {
                    break;
                }
                if candidate == ch {
                    closing = Some(start + ch.len_utf8() + offset);
                    break;
                }
            }
            if let Some(end) = closing {
                while chars.peek().is_some_and(|&(index, _)| index <= end) {
                    chars.next();
                }
                continue;
            }
        }
        output.push(ch);
    }
    output
}

fn upper(input: &str) -> String {
    let mut result = String::with_capacity(input.len());
    for ch in input.chars() {
        if let Some((mapped, _)) = unicode::mapping(ch) {
            result.push_str(mapped);
        } else {
            result.push(ch);
        }
    }
    result
}

/// ECMAScript default lowercase using the same frozen Unicode data as case rules.
pub(crate) fn js_lowercase(input: &str) -> String {
    lower(input)
}

fn lower(input: &str) -> String {
    let chars: Vec<char> = input.chars().collect();
    // Final_Sigma ignores Case_Ignorable even when it also has Cased property.
    let mut next_cased = vec![false; chars.len()];
    let mut next = false;
    for i in (0..chars.len()).rev() {
        next_cased[i] = next;
        let mask = unicode::classify(chars[i]);
        if mask & unicode::CASE_IGNORABLE == 0 {
            next = mask & unicode::CASED != 0;
        }
    }
    let mut previous = false;
    let mut result = String::with_capacity(input.len());
    for (i, &ch) in chars.iter().enumerate() {
        if ch == 'Σ' && previous && !next_cased[i] {
            result.push('ς');
        } else if let Some((_, mapped)) = unicode::mapping(ch) {
            result.push_str(mapped);
        } else {
            result.push(ch);
        }
        let mask = unicode::classify(ch);
        if mask & unicode::CASE_IGNORABLE == 0 {
            previous = mask & unicode::CASED != 0;
        }
    }
    result
}

// es-toolkit upperFirst/capitalize operate on the first UTF-16 code unit.
// A supplementary scalar's first unit is a lone high surrogate, unchanged by
// uppercase; the low surrogate in the remaining slice is also unchanged.
fn upper_first(input: &str, lower_rest: bool) -> String {
    let mut chars = input.chars();
    let Some(first) = chars.next() else {
        return String::new();
    };
    let mut result = if first.len_utf16() == 2 {
        first.to_string()
    } else {
        upper(&first.to_string())
    };
    if lower_rest {
        result.push_str(&lower(chars.as_str()));
    } else {
        result.push_str(chars.as_str());
    }
    result
}

fn deburr(input: &str) -> String {
    let mut result = String::with_capacity(input.len());
    for ch in input.nfd() {
        if matches!(ch, '\u{300}'..='\u{36f}' | '\u{20d0}'..='\u{20ff}' | '\u{fe20}'..='\u{fe2f}' | '\'' | '\u{2019}')
        {
            continue;
        }
        let replacement = match ch {
            'Æ' => "Ae",
            'Ð' => "D",
            'Ø' => "O",
            'Þ' => "Th",
            'ß' => "ss",
            'æ' => "ae",
            'ð' => "d",
            'ø' => "o",
            'þ' => "th",
            'Đ' => "D",
            'đ' => "d",
            'Ħ' => "H",
            'ħ' => "h",
            'ı' => "i",
            'Ĳ' => "IJ",
            'ĳ' => "ij",
            'ĸ' => "k",
            'Ŀ' => "L",
            'ŀ' => "l",
            'Ł' => "L",
            'ł' => "l",
            'ŉ' => "n",
            'Ŋ' => "N",
            'ŋ' => "n",
            'Œ' => "Oe",
            'œ' => "oe",
            'Ŧ' => "T",
            'ŧ' => "t",
            'ſ' => "s",
            _ => {
                result.push(ch);
                continue;
            }
        };
        result.push_str(replacement);
    }
    result
}

struct Symbol {
    ch: char,
    offset: usize,
    mask: u16,
}

fn misc_end(chars: &[Symbol], i: usize) -> Option<usize> {
    if chars.get(i)?.mask & (unicode::MODIFIER | unicode::OTHER) == 0 {
        return None;
    }
    let mut end = i + 1;
    while chars
        .get(end)
        .is_some_and(|ch| ch.mask & unicode::MARK != 0)
    {
        end += 1;
    }
    Some(end)
}

fn is_break(ch: &Symbol) -> bool {
    ch.mask & (unicode::SEPARATOR | unicode::PUNCTUATION) != 0
        || matches!(ch.ch, '\u{0}'..='\u{2f}' | '\u{3a}'..='\u{40}' | '\u{5b}'..='\u{60}' | '\u{7b}'..='\u{bf}' | '\u{d7}' | '\u{f7}')
}

fn is_upper(chars: &[Symbol], i: usize) -> bool {
    chars.get(i).is_some_and(|ch| ch.mask & unicode::UPPER != 0)
}
fn is_lower(chars: &[Symbol], i: usize) -> bool {
    chars.get(i).is_some_and(|ch| ch.mask & unicode::LOWER != 0)
}
fn is_misc(chars: &[Symbol], i: usize) -> bool {
    chars
        .get(i)
        .is_some_and(|ch| ch.mask & (unicode::MODIFIER | unicode::OTHER) != 0)
}

fn word_end(chars: &[Symbol], start: usize) -> Option<usize> {
    let after_upper = start + usize::from(is_upper(chars, start));
    // Alternative 1: Lu? Ll+ followed by break, Lu, or end.
    let mut end = after_upper;
    let mut accepted_lower = None;
    while is_lower(chars, end) {
        end += 1;
        // A character can belong to both Ll and the explicit Latin break
        // ranges (notably MICRO SIGN). Keep every accepted lookahead endpoint
        // so the greedy Ll+ can backtrack to its longest successful match.
        if end == chars.len() || is_break(&chars[end]) || is_upper(chars, end) {
            accepted_lower = Some(end);
        }
    }
    if accepted_lower.is_some() {
        return accepted_lower;
    }
    // Alternative 2: (Lu | (Lm|Lo) M*)+ with the acronym lookahead.
    // Keep the longest accepted endpoint while scanning forward once.
    end = start;
    let mut accepted = None;
    loop {
        if is_upper(chars, end) {
            end += 1;
        } else if let Some(next) = misc_end(chars, end) {
            end = next;
        } else {
            break;
        }
        if end == chars.len()
            || is_break(&chars[end])
            || (is_upper(chars, end) && (is_lower(chars, end + 1) || is_misc(chars, end + 1)))
        {
            accepted = Some(end);
        }
    }
    if accepted.is_some() {
        return accepted;
    }
    // Alternative 3: Lu? (Ll | (Lm|Lo) M*)+.
    end = after_upper;
    loop {
        if is_lower(chars, end) {
            end += 1;
        } else if let Some(next) = misc_end(chars, end) {
            end = next;
        } else {
            break;
        }
    }
    if end > after_upper {
        return Some(end);
    }
    // Alternative 4: Lu+.
    end = start;
    while is_upper(chars, end) {
        end += 1;
    }
    if end > start {
        return Some(end);
    }
    // Alternatives 5/6/7: ASCII ordinals (case-specific boundaries), then digits.
    end = start;
    while chars.get(end).is_some_and(|ch| ch.ch.is_ascii_digit()) {
        end += 1;
    }
    if end > start {
        let suffix = match chars[end - 1].ch {
            '1' => ['s', 't'],
            '2' => ['n', 'd'],
            '3' => ['r', 'd'],
            _ => ['t', 'h'],
        };
        for uppercase in [true, false] {
            let expected = suffix.map(|ch| {
                if uppercase {
                    ch.to_ascii_uppercase()
                } else {
                    ch
                }
            });
            if chars.get(end).is_some_and(|ch| ch.ch == expected[0])
                && chars.get(end + 1).is_some_and(|ch| ch.ch == expected[1])
            {
                let boundary = chars.get(end + 2).is_none_or(|ch| {
                    !ch.ch.is_ascii_alphanumeric()
                        || if uppercase {
                            ch.ch.is_ascii_lowercase()
                        } else {
                            ch.ch.is_ascii_uppercase()
                        }
                });
                if boundary {
                    return Some(end + 2);
                }
            }
        }
        return Some(end);
    }
    // Alternatives 8/9 are individual emoji scalars, including each regional flag.
    if chars[start].mask & (unicode::EMOJI_PRESENTATION | unicode::EXTENDED_PICTOGRAPHIC) != 0 {
        return Some(start + 1);
    }
    None
}

fn words(input: &str) -> Vec<&str> {
    let chars: Vec<Symbol> = input
        .char_indices()
        .map(|(offset, ch)| Symbol {
            ch,
            offset,
            mask: unicode::classify(ch),
        })
        .collect();
    let mut result = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        if let Some(end) = word_end(&chars, i) {
            let byte_end = chars.get(end).map_or(input.len(), |ch| ch.offset);
            result.push(&input[chars[i].offset..byte_end]);
            i = end;
        } else {
            i += 1;
        }
    }
    result
}

fn to_case(input: &str, target: &str) -> Result<String, String> {
    match target {
        "upper-case" | "uppercase" => return Ok(upper(input)),
        "lower-case" | "lowercase" | "lowerCase" => return Ok(js_lowercase(input)),
        "sentence-case" | "sentencecase" => return Ok(upper_first(input, false)),
        _ if !is_target_case(target) => {
            return Err(format!("to-case: Unknown target case \"{target}\""));
        }
        _ => {}
    }
    let normalized = deburr(input);
    let split = words(&normalized);
    let transformed = match target {
        "camel-case" | "pascal-case" => {
            let camel = split
                .iter()
                .enumerate()
                .map(|(i, word)| {
                    if i == 0 {
                        lower(word)
                    } else {
                        upper_first(word, true)
                    }
                })
                .collect::<String>();
            if target == "pascal-case" {
                upper_first(&camel, false)
            } else {
                camel
            }
        }
        "kebab-case" | "snake-case" => split
            .iter()
            .map(|word| lower(word))
            .collect::<Vec<_>>()
            .join(if target == "kebab-case" { "-" } else { "_" }),
        "start-case" => split
            .iter()
            .map(|word| {
                if *word == upper(word) {
                    word.to_string()
                } else {
                    upper_first(word, true)
                }
            })
            .collect::<Vec<_>>()
            .join(" "),
        _ => unreachable!("validated case target"),
    };
    Ok(transformed)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exact_oracle_transformations() {
        let data: serde_json::Value =
            serde_json::from_str(include_str!("../tests/fixtures/case-unicode.json")).unwrap();
        for item in data["cases"].as_array().unwrap() {
            let input = item["input"].as_str().unwrap();
            let target = item["target"].as_str().unwrap();
            let stripped = strip_quotes(input);
            assert_eq!(
                to_case(stripped.trim_matches(js_whitespace), target).unwrap(),
                item["transformed"].as_str().unwrap(),
                "input {input:?}, target {target}"
            );
        }
    }
    #[test]
    fn large_runs_use_a_bounded_scanner() {
        let raw = "A".repeat(100_000) + "1";
        assert!(!ensure_case(&raw, "camel-case").unwrap());
        assert_eq!(to_case(&raw, "camel-case").unwrap().len(), raw.len());
        let micro = "a".to_owned() + &"µ".repeat(100_000) + "1";
        assert_eq!(
            to_case(&micro, "camel-case").unwrap(),
            "a".to_owned() + &"µ".repeat(99_999) + "Μ1"
        );
        let sigma = "AΣ\u{0345}".repeat(20_000);
        assert_eq!(lower(&sigma), "aσ\u{0345}".repeat(19_999) + "aς\u{0345}");
    }
}
