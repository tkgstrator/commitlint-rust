//! Strict shape of the optional `source-sha256` immutable-origin commit header.
//! This validates syntax and position only; it never proves historical authority.
use crate::Result;

pub const KEY: &str = "source-sha256";

pub fn digest_valid(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// Inspect a raw commit header block (everything before the first blank line).
/// Returns the digest when exactly one well-formed header occupies the final
/// slot immediately after `committer`; any other use of the key is refused.
pub fn header_origin(headers: &str) -> Result<Option<&str>> {
    let lines: Vec<&str> = headers.split('\n').collect();
    let mut found = None;
    for (index, line) in lines.iter().enumerate() {
        // A space-prefixed line is data of the previous extension header (for
        // example an embedded mergetag message), not a new commit header.
        // Real origin folding still fails because the origin is no longer final.
        if line.starts_with(' ') {
            continue;
        }
        if !line
            .split_whitespace()
            .next()
            .is_some_and(|key| key.eq_ignore_ascii_case(KEY))
        {
            continue;
        }
        let value = line
            .strip_prefix(KEY)
            .and_then(|rest| rest.strip_prefix(' '))
            .filter(|value| digest_valid(value))
            .ok_or("malformed source-sha256 header")?;
        if found.is_some()
            || index + 1 != lines.len()
            || index == 0
            || !lines[index - 1].starts_with("committer ")
        {
            return Err("duplicate or misplaced source-sha256 header".into());
        }
        found = Some(value);
    }
    Ok(found)
}
