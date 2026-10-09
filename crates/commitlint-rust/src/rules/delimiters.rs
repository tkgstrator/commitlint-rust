use super::types::RuleValue;

pub(super) enum Alt {
    /// `, ?`
    Comma,
    Literal(Vec<u16>),
}

pub(super) fn alternatives(delimiters: &[String]) -> Vec<Alt> {
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
pub(super) fn match_at(units: &[u16], at: usize, alts: &[Alt]) -> Option<usize> {
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
pub(super) fn contains_match(units: &[u16], alts: &[Alt]) -> bool {
    (0..=units.len()).any(|at| match_at(units, at, alts).is_some())
}

/// `String.prototype.split(regex)` for a non-empty input (ES `@@split`).
pub(super) fn split_units(units: &[u16], alts: &[Alt]) -> Vec<Vec<u16>> {
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
pub(super) fn segment_text(units: &[u16]) -> String {
    String::from_utf16_lossy(units)
}

pub(super) fn scope_delimiters(value: &RuleValue) -> &[String] {
    match value {
        RuleValue::ScopeEnum { delimiters, .. } | RuleValue::ScopeCases { delimiters, .. } => {
            delimiters
        }
        _ => &[],
    }
}
