use super::helpers::{falsy, outcome, pass};
use super::{RuleCondition, RuleOutcome, RuleValue, evaluate_rule};
use crate::parser::{ParsedMessage, is_js_whitespace};

/// config-conventional `header-trim`.
pub(crate) fn header_trim(parsed: &ParsedMessage) -> RuleOutcome {
    let Some(header) = falsy(&parsed.header) else {
        return pass();
    };
    let start = header.starts_with(is_js_whitespace);
    let end = header.ends_with(is_js_whitespace);
    match (start, end) {
        (true, true) => outcome(false, "header must not be surrounded by whitespace"),
        (true, false) => outcome(false, "header must not start with whitespace"),
        (false, true) => outcome(false, "header must not end with whitespace"),
        _ => pass(),
    }
}

fn fixed_case(
    name: &str,
    parsed: &ParsedMessage,
    when: Option<RuleCondition>,
    targets: &[&str],
) -> RuleOutcome {
    let value = RuleValue::List(targets.iter().map(|t| (*t).to_owned()).collect());
    evaluate_rule(name, parsed, "", when, &value).unwrap_or_else(|e| outcome(false, e))
}

/// config-conventional `subject-case`: never sentence/start/pascal/upper.
pub(crate) fn subject_case(parsed: &ParsedMessage) -> RuleOutcome {
    fixed_case(
        "subject-case",
        parsed,
        Some(RuleCondition::Never),
        &["sentence-case", "start-case", "pascal-case", "upper-case"],
    )
}

/// config-conventional `type-case`: always lower-case.
pub(crate) fn type_case(parsed: &ParsedMessage) -> RuleOutcome {
    fixed_case(
        "type-case",
        parsed,
        Some(RuleCondition::Always),
        &["lower-case"],
    )
}
