use commitlint_rust::rules::{
    CaseCheck, EvaluationContext, SUPPORTED_RULES, evaluate_rule_with_context, validate_rule_value,
};
use commitlint_rust::{
    ParserPreset, RuleCondition, RuleOutcome, RuleValue, evaluate_rule, parse_message,
};
use std::path::PathBuf;
#[cfg(unix)]
mod common;

const ALWAYS: Option<RuleCondition> = Some(RuleCondition::Always);
const NEVER: Option<RuleCondition> = Some(RuleCondition::Never);

fn run(
    name: &str,
    raw: &str,
    when: Option<RuleCondition>,
    value: RuleValue,
) -> Result<RuleOutcome, String> {
    let parsed = parse_message(raw, ParserPreset::ConventionalCommits).unwrap();
    evaluate_rule(name, &parsed, raw, when, &value)
}

fn ok(name: &str, raw: &str, when: Option<RuleCondition>, value: RuleValue) -> RuleOutcome {
    run(name, raw, when, value).unwrap()
}

fn msg(valid: bool, text: &str) -> RuleOutcome {
    RuleOutcome {
        valid,
        message: Some(text.into()),
    }
}

fn bare() -> RuleOutcome {
    RuleOutcome {
        valid: true,
        message: None,
    }
}

fn strings(items: &[&str]) -> Vec<String> {
    items.iter().map(|s| (*s).to_owned()).collect()
}

fn checks(items: &[(&str, Option<RuleCondition>)]) -> Vec<CaseCheck> {
    items
        .iter()
        .map(|(target, when)| CaseCheck {
            target: (*target).into(),
            when: *when,
        })
        .collect()
}

#[path = "all_rules/breaking.rs"]
mod breaking;
#[path = "all_rules/case.rs"]
mod case;
#[path = "all_rules/empty.rs"]
mod empty;
#[path = "all_rules/length.rs"]
mod length;
#[path = "all_rules/punctuation.rs"]
mod punctuation;
#[path = "all_rules/registry.rs"]
mod registry;
#[path = "all_rules/scope.rs"]
mod scope;
#[path = "all_rules/trailers.rs"]
mod trailers;
#[path = "all_rules/upstream.rs"]
mod upstream;
