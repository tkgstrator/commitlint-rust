use super::helpers::{is_cherry_pick, message, negated, outcome};
use super::types::{EvaluationContext, RuleCondition, RuleOutcome, RuleValue};
use super::values::text_value;
use crate::Result;
use crate::parser::split_lines;

pub(super) fn signed_off_by(
    name: &str,
    raw: &str,
    when: Option<RuleCondition>,
    value: &RuleValue,
) -> Result<RuleOutcome> {
    let neg = negated(when);
    let prefix = text_value(name, value, "")?;
    let last = split_lines(raw)
        .into_iter()
        .rfind(|ln| !ln.starts_with('#') && !is_cherry_pick(ln) && !ln.is_empty());
    let signed = last.is_some_and(|ln| ln.starts_with(prefix));
    Ok(outcome(
        if neg { !signed } else { signed },
        message(&[
            Some("message"),
            Some(if neg { "must not" } else { "must" }),
            Some("be signed off"),
        ]),
    ))
}

pub(super) fn trailer_exists(
    name: &str,
    raw: &str,
    when: Option<RuleCondition>,
    value: &RuleValue,
    context: &EvaluationContext,
) -> Result<RuleOutcome> {
    let neg = negated(when);
    let prefix = text_value(name, value, "")?;
    let trailers = crate::git::interpret_trailers(
        raw,
        context.git.as_deref(),
        context.cwd.as_deref(),
        context.timeout,
        context.output_limit,
    )?;
    let has = split_lines(&trailers)
        .iter()
        .any(|ln| ln.starts_with(prefix));
    Ok(outcome(
        if neg { !has } else { has },
        message(&[
            Some("message"),
            Some(if neg { "must not" } else { "must" }),
            Some(format!("have `{prefix}` trailer").as_str()),
        ]),
    ))
}
