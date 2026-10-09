use super::delimiters::{alternatives, contains_match, segment_text, split_units};
use super::helpers::{falsy, message, negated, outcome, pass};
use super::types::{CaseCheck, RuleCondition, RuleOutcome, RuleValue};
use super::values::case_checks;
use crate::Result;
use crate::case::{ensure_case, subject_gate};
use crate::parser::ParsedMessage;

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

pub(super) fn body_case(
    name: &str,
    parsed: &ParsedMessage,
    when: Option<RuleCondition>,
    value: &RuleValue,
) -> Result<RuleOutcome> {
    let Some(body) = falsy(&parsed.body) else {
        return Ok(pass());
    };
    let (checks, _) = case_checks(name, value, false)?;
    simple_case("body", body, when, &checks)
}

pub(super) fn header_case(
    name: &str,
    parsed: &ParsedMessage,
    when: Option<RuleCondition>,
    value: &RuleValue,
) -> Result<RuleOutcome> {
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

pub(super) fn subject_case(
    name: &str,
    parsed: &ParsedMessage,
    when: Option<RuleCondition>,
    value: &RuleValue,
) -> Result<RuleOutcome> {
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

pub(super) fn type_case(
    name: &str,
    parsed: &ParsedMessage,
    when: Option<RuleCondition>,
    value: &RuleValue,
) -> Result<RuleOutcome> {
    let Some(kind) = falsy(&parsed.r#type) else {
        return Ok(pass());
    };
    let (checks, _) = case_checks(name, value, false)?;
    simple_case("type", kind, when, &checks)
}

pub(super) fn scope_case(
    name: &str,
    parsed: &ParsedMessage,
    when: Option<RuleCondition>,
    value: &RuleValue,
) -> Result<RuleOutcome> {
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
