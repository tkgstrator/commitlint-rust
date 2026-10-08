//! Message-only Conventional Commit linting. No account, identity,
//! attribution or hook concerns live in this crate. Native JSON rule
//! configuration remains independent of account and guard configuration.
pub mod case;
pub mod cli;
pub mod configured;
pub mod git;
pub mod parser;
pub mod policy;
mod references;
pub mod rules;
mod unicode_properties;

pub use configured::{ConfigSeverity, Configuration, RuleSpec, lint_configured, parse_json_config};
pub use parser::{ParsedMessage, ParserPreset, parse_message, parse_message_with_comment_char};
pub use parser::{ParserOptions, Reference, parse_message_with_options};
pub use policy::{Diagnostic, LintOutcome, Severity, lint_detailed, lint_message};
pub use rules::{RuleCondition, RuleOutcome, RuleValue, evaluate_rule};
pub type Result<T> = std::result::Result<T, String>;
