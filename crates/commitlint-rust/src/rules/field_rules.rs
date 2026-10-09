use super::helpers::{
    bang_regex, falsy, has_breaking_footer, last_unit_is, leading_is_blank, message, negated,
    outcome, pass,
};
use super::types::{RuleCondition, RuleOutcome, RuleValue};
use super::values::text_value;
use crate::Result;
use crate::parser::{ParsedMessage, js_len, split_lines};

pub(super) fn field_empty(
    name: &str,
    parsed: &ParsedMessage,
    when: Option<RuleCondition>,
) -> Result<RuleOutcome> {
    let must = |neg: bool| if neg { "may not" } else { "must" };
    let neg = negated(when);
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

pub(super) fn body_full_stop(
    name: &str,
    parsed: &ParsedMessage,
    when: Option<RuleCondition>,
    value: &RuleValue,
) -> Result<RuleOutcome> {
    let must = |neg: bool| if neg { "may not" } else { "must" };
    let neg = negated(when);
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

pub(super) fn header_full_stop(
    name: &str,
    parsed: &ParsedMessage,
    when: Option<RuleCondition>,
    value: &RuleValue,
) -> Result<RuleOutcome> {
    let must = |neg: bool| if neg { "may not" } else { "must" };
    let neg = negated(when);
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

pub(super) fn subject_full_stop(
    name: &str,
    parsed: &ParsedMessage,
    when: Option<RuleCondition>,
    value: &RuleValue,
) -> Result<RuleOutcome> {
    let must = |neg: bool| if neg { "may not" } else { "must" };
    let neg = negated(when);
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

pub(super) fn body_leading_blank(
    parsed: &ParsedMessage,
    raw: &str,
    when: Option<RuleCondition>,
) -> Result<RuleOutcome> {
    let must = |neg: bool| if neg { "may not" } else { "must" };
    let neg = negated(when);
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

pub(super) fn footer_leading_blank(
    parsed: &ParsedMessage,
    raw: &str,
    when: Option<RuleCondition>,
) -> Result<RuleOutcome> {
    let must = |neg: bool| if neg { "may not" } else { "must" };
    let neg = negated(when);
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

pub(super) fn breaking_change_exclamation_mark(
    parsed: &ParsedMessage,
    when: Option<RuleCondition>,
) -> Result<RuleOutcome> {
    let neg = negated(when);
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

pub(super) fn subject_exclamation_mark(
    parsed: &ParsedMessage,
    when: Option<RuleCondition>,
) -> Result<RuleOutcome> {
    let neg = negated(when);
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
