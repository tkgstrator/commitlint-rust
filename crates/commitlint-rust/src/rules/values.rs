use super::types::{CaseCheck, RuleValue};
use crate::{Result, case::is_target_case};

/// A numeric limit: exact for integers, IEEE-754 for direct float values.
#[derive(Clone, Copy, Debug)]
pub(super) enum Limit {
    Int(usize),
    Float(f64),
}

impl Limit {
    pub(super) fn max_ok(self, len: usize) -> bool {
        match self {
            Limit::Int(n) => len <= n,
            Limit::Float(f) => (len as f64) <= f,
        }
    }

    pub(super) fn min_ok(self, len: usize) -> bool {
        match self {
            Limit::Int(n) => len >= n,
            Limit::Float(f) => (len as f64) >= f,
        }
    }

    /// JavaScript template-literal rendering of the number.
    pub(super) fn show(self) -> String {
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

pub(super) fn limit_value(name: &str, value: &RuleValue) -> Result<Limit> {
    match value {
        RuleValue::None => Ok(Limit::Int(0)),
        RuleValue::Length(n) => Ok(Limit::Int(*n)),
        RuleValue::Number(f) => Ok(Limit::Float(*f)),
        _ => Err(format!("rule {name} requires a numeric value")),
    }
}

pub(super) fn text_value<'a>(
    name: &str,
    value: &'a RuleValue,
    default: &'a str,
) -> Result<&'a str> {
    match value {
        RuleValue::None => Ok(default),
        RuleValue::Text(text) => Ok(text.as_str()),
        _ => Err(format!("rule {name} requires a text value")),
    }
}

pub(super) fn list_value<'a>(name: &str, value: &'a RuleValue) -> Result<&'a [String]> {
    match value {
        RuleValue::None => Ok(&[]),
        RuleValue::List(list) => Ok(list.as_slice()),
        _ => Err(format!("rule {name} requires a list value")),
    }
}

/// Normalizes a case rule value to checks plus (for scope-case) delimiters.
pub(super) fn case_checks(
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
