//! The 38 upstream @commitlint/rules (21.2.3) over the bounded parse, with
//! upstream defaults, `always`/`never` semantics, JavaScript UTF-16 lengths and
//! delimiter splitting. Case behavior lives in `crate::case`.
use crate::{Result, parser::ParsedMessage};

mod case_rules;
mod delimiters;
mod field_rules;
mod fixed;
mod helpers;
mod length_rules;
mod scope_rules;
mod trailer_rules;
mod types;
mod values;

pub(crate) use fixed::{header_trim, subject_case, type_case};
pub use types::{CaseCheck, EvaluationContext, RuleCondition, RuleOutcome, RuleValue};
pub use values::validate_rule_value;

/// Exported rule names in upstream `index.ts` order.
pub const SUPPORTED_RULES: [&str; 38] = [
    "body-case",
    "body-empty",
    "body-full-stop",
    "body-leading-blank",
    "body-max-length",
    "body-max-line-length",
    "body-min-length",
    "breaking-change-exclamation-mark",
    "footer-empty",
    "footer-leading-blank",
    "footer-max-length",
    "footer-max-line-length",
    "footer-min-length",
    "header-case",
    "header-full-stop",
    "header-max-length",
    "header-min-length",
    "header-trim",
    "references-empty",
    "scope-case",
    "scope-delimiter-style",
    "scope-empty",
    "scope-enum",
    "scope-max-length",
    "scope-min-length",
    "signed-off-by",
    "subject-case",
    "subject-empty",
    "subject-exclamation-mark",
    "subject-full-stop",
    "subject-max-length",
    "subject-min-length",
    "trailer-exists",
    "type-case",
    "type-empty",
    "type-enum",
    "type-max-length",
    "type-min-length",
];

/// Evaluate one upstream rule with the default evaluation context. `raw` is
/// the unparsed message handed to the parser.
pub fn evaluate_rule(
    name: &str,
    parsed: &ParsedMessage,
    raw: &str,
    when: Option<RuleCondition>,
    value: &RuleValue,
) -> Result<RuleOutcome> {
    evaluate_rule_with_context(
        name,
        parsed,
        raw,
        when,
        value,
        &EvaluationContext::default(),
    )
}

pub fn evaluate_rule_with_context(
    name: &str,
    parsed: &ParsedMessage,
    raw: &str,
    when: Option<RuleCondition>,
    value: &RuleValue,
    context: &EvaluationContext,
) -> Result<RuleOutcome> {
    validate_rule_value(name, value)?;
    match name {
        "type-empty" | "subject-empty" | "body-empty" | "footer-empty" => {
            field_rules::field_empty(name, parsed, when)
        }
        // These two default to `never`: `always` is the negated form.
        "scope-empty" | "references-empty" => {
            scope_rules::scope_or_references_empty(name, parsed, when)
        }
        "type-enum" => scope_rules::type_enum(name, parsed, when, value),
        "header-max-length" | "header-min-length" => {
            length_rules::header_length(name, parsed, value)
        }
        "body-max-length" | "body-min-length" | "footer-max-length" | "footer-min-length"
        | "scope-max-length" | "scope-min-length" | "subject-max-length" | "subject-min-length"
        | "type-max-length" | "type-min-length" => length_rules::field_length(name, parsed, value),
        "body-max-line-length" | "footer-max-line-length" => {
            length_rules::line_length(name, parsed, value)
        }
        "body-full-stop" => field_rules::body_full_stop(name, parsed, when, value),
        "header-full-stop" => field_rules::header_full_stop(name, parsed, when, value),
        "subject-full-stop" => field_rules::subject_full_stop(name, parsed, when, value),
        "body-leading-blank" => field_rules::body_leading_blank(parsed, raw, when),
        "footer-leading-blank" => field_rules::footer_leading_blank(parsed, raw, when),
        "header-trim" => Ok(header_trim(parsed)),
        "breaking-change-exclamation-mark" => {
            field_rules::breaking_change_exclamation_mark(parsed, when)
        }
        "subject-exclamation-mark" => field_rules::subject_exclamation_mark(parsed, when),
        "body-case" => case_rules::body_case(name, parsed, when, value),
        "header-case" => case_rules::header_case(name, parsed, when, value),
        "subject-case" => case_rules::subject_case(name, parsed, when, value),
        "type-case" => case_rules::type_case(name, parsed, when, value),
        "scope-case" => case_rules::scope_case(name, parsed, when, value),
        "scope-enum" => scope_rules::scope_enum(parsed, when, value),
        "scope-delimiter-style" => scope_rules::scope_delimiter_style(name, parsed, when, value),
        "signed-off-by" => trailer_rules::signed_off_by(name, raw, when, value),
        "trailer-exists" => trailer_rules::trailer_exists(name, raw, when, value, context),
        _ => Err(format!("unsupported commitlint rule: {name}")),
    }
}
