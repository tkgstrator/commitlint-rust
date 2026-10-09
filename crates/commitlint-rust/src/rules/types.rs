use std::{path::PathBuf, time::Duration};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RuleCondition {
    Always,
    Never,
}

#[derive(Clone, Debug, PartialEq)]
pub enum RuleValue {
    /// Upstream `undefined` (not `null`): the rule default applies.
    None,
    Length(usize),
    Text(String),
    List(Vec<String>),
    Number(f64),
    CaseChecks(Vec<CaseCheck>),
    ScopeEnum {
        scopes: Vec<String>,
        delimiters: Vec<String>,
    },
    ScopeCases {
        cases: Vec<CaseCheck>,
        delimiters: Vec<String>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CaseCheck {
    pub target: String,
    pub when: Option<RuleCondition>,
}

#[derive(Clone, Debug)]
pub struct EvaluationContext {
    pub git: Option<PathBuf>,
    pub cwd: Option<PathBuf>,
    pub timeout: Duration,
    pub output_limit: usize,
}

impl Default for EvaluationContext {
    fn default() -> Self {
        Self {
            git: None,
            cwd: None,
            timeout: Duration::from_secs(10),
            output_limit: 64 * 1024 * 1024,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuleOutcome {
    pub valid: bool,
    /// `None` where upstream returns a bare `[true]` without a message.
    pub message: Option<String>,
}
