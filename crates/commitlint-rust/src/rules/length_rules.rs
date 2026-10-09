use super::helpers::{falsy, max_line_length, outcome, pass};
use super::types::{RuleOutcome, RuleValue};
use super::values::limit_value;
use crate::Result;
use crate::parser::{ParsedMessage, js_len};

pub(super) fn header_length(
    name: &str,
    parsed: &ParsedMessage,
    value: &RuleValue,
) -> Result<RuleOutcome> {
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

pub(super) fn field_length(
    name: &str,
    parsed: &ParsedMessage,
    value: &RuleValue,
) -> Result<RuleOutcome> {
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

pub(super) fn line_length(
    name: &str,
    parsed: &ParsedMessage,
    value: &RuleValue,
) -> Result<RuleOutcome> {
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
