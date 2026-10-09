use super::delimiters::{alternatives, scope_delimiters, split_units};
use super::helpers::{falsy, message, negated, outcome, pass};
use super::types::{RuleCondition, RuleOutcome, RuleValue};
use super::values::list_value;
use crate::Result;
use crate::parser::{ParsedMessage, is_js_whitespace};
use regex::Regex;
use std::sync::OnceLock;

pub(super) fn scope_or_references_empty(
    name: &str,
    parsed: &ParsedMessage,
    when: Option<RuleCondition>,
) -> Result<RuleOutcome> {
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

pub(super) fn type_enum(
    name: &str,
    parsed: &ParsedMessage,
    when: Option<RuleCondition>,
    value: &RuleValue,
) -> Result<RuleOutcome> {
    let neg = negated(when);
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

pub(super) fn scope_enum(
    parsed: &ParsedMessage,
    when: Option<RuleCondition>,
    value: &RuleValue,
) -> Result<RuleOutcome> {
    let neg = negated(when);
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
    let allowed: Vec<Vec<u16>> = scopes.iter().map(|s| s.encode_utf16().collect()).collect();
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

pub(super) fn scope_delimiter_style(
    name: &str,
    parsed: &ParsedMessage,
    when: Option<RuleCondition>,
    value: &RuleValue,
) -> Result<RuleOutcome> {
    let neg = negated(when);
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
