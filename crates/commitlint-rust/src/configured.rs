//! Ordered native JSON configuration and linting, independent of the fixed policy.
use crate::Result;
use crate::parser::{ParserOptions, ParserPreset, is_js_whitespace, parse_message_with_options};
use crate::policy::{Diagnostic, LintOutcome, Severity};
use crate::rules::{
    CaseCheck, EvaluationContext, RuleCondition, RuleValue, SUPPORTED_RULES,
    evaluate_rule_with_context, validate_rule_value,
};
use regex::Regex;
use serde::{
    Deserialize, Deserializer,
    de::{MapAccess, Visitor},
};
use serde_json::Value;
use std::sync::OnceLock;
use std::{collections::BTreeMap, fmt};

// Preserve only rule insertion order. Enabling serde_json/preserve_order
// would also change the guard's JSON bytes through Cargo feature unification.
#[derive(Default)]
struct OrderedRules(Vec<(String, Value)>);

impl<'de> Deserialize<'de> for OrderedRules {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        struct RulesVisitor;
        impl<'de> Visitor<'de> for RulesVisitor {
            type Value = OrderedRules;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a rules object")
            }
            fn visit_map<M: MapAccess<'de>>(
                self,
                mut map: M,
            ) -> std::result::Result<Self::Value, M::Error> {
                let mut rules = Vec::<(String, Value)>::new();
                while let Some((name, value)) = map.next_entry::<String, Value>()? {
                    if let Some(existing) = rules.iter_mut().find(|(key, _)| key == &name) {
                        existing.1 = value;
                    } else {
                        rules.push((name, value));
                    }
                }
                Ok(OrderedRules(rules))
            }
        }
        deserializer.deserialize_map(RulesVisitor)
    }
}

struct ConfigurationFile {
    fields: BTreeMap<String, Value>,
    rules: OrderedRules,
}

impl<'de> Deserialize<'de> for ConfigurationFile {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        struct ConfigVisitor;
        impl<'de> Visitor<'de> for ConfigVisitor {
            type Value = ConfigurationFile;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a configuration object")
            }
            fn visit_map<M: MapAccess<'de>>(
                self,
                mut map: M,
            ) -> std::result::Result<Self::Value, M::Error> {
                let mut config = ConfigurationFile {
                    fields: BTreeMap::new(),
                    rules: OrderedRules::default(),
                };
                while let Some(key) = map.next_key::<String>()? {
                    let value = if key == "rules" {
                        config.rules = map.next_value()?;
                        Value::Null
                    } else {
                        map.next_value()?
                    };
                    config.fields.insert(key, value);
                }
                Ok(config)
            }
        }
        deserializer.deserialize_map(ConfigVisitor)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConfigSeverity {
    Disabled,
    Warning,
    Error,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RuleSpec {
    pub name: String,
    pub severity: ConfigSeverity,
    pub condition: Option<RuleCondition>,
    pub value: RuleValue,
}

#[derive(Clone, Debug)]
pub struct Configuration {
    pub parser: ParserOptions,
    pub rules: Vec<RuleSpec>,
    pub default_ignores: bool,
}

impl Default for Configuration {
    fn default() -> Self {
        Self {
            parser: ParserOptions::default(),
            rules: vec![],
            default_ignores: true,
        }
    }
}

fn condition(value: &Value) -> Result<RuleCondition> {
    match value.as_str() {
        Some("always") => Ok(RuleCondition::Always),
        Some("never") => Ok(RuleCondition::Never),
        _ => Err("rule condition must be always or never".into()),
    }
}

fn strings(value: &Value, label: &str) -> Result<Vec<String>> {
    value
        .as_array()
        .ok_or_else(|| format!("{label} requires an array"))?
        .iter()
        .map(|v| {
            v.as_str()
                .map(str::to_owned)
                .ok_or_else(|| format!("{label} requires string entries"))
        })
        .collect()
}

fn case_checks(value: &Value) -> Result<Vec<CaseCheck>> {
    let entries = match value {
        Value::Array(a) => a.iter().collect(),
        _ => vec![value],
    };
    entries
        .into_iter()
        .map(|entry| {
            if let Some(target) = entry.as_str() {
                return Ok(CaseCheck {
                    target: target.into(),
                    when: None,
                });
            }
            let object = entry
                .as_object()
                .ok_or("case check must be a string or object")?;
            if object.keys().any(|k| k != "case" && k != "when") {
                return Err("unsupported case-check field".into());
            }
            let target = object
                .get("case")
                .and_then(Value::as_str)
                .ok_or("case check requires case")?
                .to_owned();
            let when = object.get("when").map(condition).transpose()?;
            Ok(CaseCheck { target, when })
        })
        .collect()
}

fn rule_value(name: &str, value: &Value) -> Result<RuleValue> {
    if value.is_null() {
        return Err(format!("rule {name} does not accept null"));
    }
    if name.ends_with("-case") {
        if name == "scope-case"
            && let Some(object) = value.as_object()
        {
            if object.keys().any(|k| k != "cases" && k != "delimiters") {
                return Err("unsupported scope-case field".into());
            }
            let cases = case_checks(object.get("cases").ok_or("scope-case requires cases")?)?;
            let delimiters = object
                .get("delimiters")
                .map(|v| strings(v, "delimiters"))
                .transpose()?
                .unwrap_or_default();
            return Ok(RuleValue::ScopeCases { cases, delimiters });
        }
        return Ok(RuleValue::CaseChecks(case_checks(value)?));
    }
    if name == "scope-enum"
        && let Some(object) = value.as_object()
    {
        if object.keys().any(|k| k != "scopes" && k != "delimiters") {
            return Err("unsupported scope-enum field".into());
        }
        let scopes = strings(
            object.get("scopes").ok_or("scope-enum requires scopes")?,
            "scopes",
        )?;
        let delimiters = object
            .get("delimiters")
            .map(|v| strings(v, "delimiters"))
            .transpose()?
            .unwrap_or_default();
        return Ok(RuleValue::ScopeEnum { scopes, delimiters });
    }
    match value {
        Value::Number(n) => Ok(RuleValue::Number(
            n.as_f64().ok_or("invalid numeric rule value")?,
        )),
        Value::String(s) => Ok(RuleValue::Text(s.clone())),
        Value::Array(_) => Ok(RuleValue::List(strings(value, "rule value")?)),
        _ => Err(format!("unsupported value for rule {name}")),
    }
}

fn parse_rule(name: &str, value: &Value) -> Result<RuleSpec> {
    if !SUPPORTED_RULES.contains(&name) {
        return Err(format!("unsupported commitlint rule: {name}"));
    }
    let tuple = value
        .as_array()
        .ok_or_else(|| format!("rule {name} must be an array"))?;
    if tuple.is_empty() || tuple.len() > 3 {
        return Err(format!("rule {name} must have one to three tuple entries"));
    }
    let severity = match tuple[0].as_f64() {
        Some(0.0) => ConfigSeverity::Disabled,
        Some(1.0) => ConfigSeverity::Warning,
        Some(2.0) => ConfigSeverity::Error,
        _ => return Err(format!("rule {name} severity must be 0, 1 or 2")),
    };
    if severity != ConfigSeverity::Disabled && tuple.len() < 2 {
        return Err(format!("rule {name} requires a condition"));
    }
    let condition = tuple.get(1).map(condition).transpose()?;
    // Disabled rules are never evaluated and their unused value is immaterial.
    let value = if severity == ConfigSeverity::Disabled {
        RuleValue::None
    } else {
        tuple
            .get(2)
            .map(|v| rule_value(name, v))
            .transpose()?
            .unwrap_or(RuleValue::None)
    };
    if severity != ConfigSeverity::Disabled {
        validate_rule_value(name, &value)?;
    }
    Ok(RuleSpec {
        name: name.into(),
        severity,
        condition,
        value,
    })
}

fn preset_name(name: &str) -> Result<ParserPreset> {
    match name {
        "conventional-changelog-angular" => Ok(ParserPreset::Angular),
        "conventional-changelog-conventionalcommits" => Ok(ParserPreset::ConventionalCommits),
        _ => Err(format!("unsupported native parser preset: {name}")),
    }
}

fn parser_preset(value: &Value, options: &mut ParserOptions) -> Result<()> {
    if let Some(name) = value.as_str() {
        options.preset = preset_name(name)?;
        return Ok(());
    }
    let object = value
        .as_object()
        .ok_or("parserPreset must be a string or object")?;
    if object.keys().any(|k| k != "name" && k != "parserOpts") {
        return Err("unsupported parserPreset field".into());
    }
    // Upstream resolves string module names only. Explicit parserOpts replaces
    // inherited options; a metadata-only object keeps them. Name loads nothing.
    if object.contains_key("parserOpts") {
        options.preset = ParserPreset::Angular;
    }
    if let Some(name) = object.get("name") {
        preset_name(name.as_str().ok_or("parser name must be text")?)?;
    }
    if let Some(opts) = object.get("parserOpts") {
        let opts = opts.as_object().ok_or("parserOpts must be an object")?;
        for (key, value) in opts {
            match key.as_str() {
                "commentChar" => {
                    let text = value.as_str().ok_or("commentChar must be a string")?;
                    let mut chars = text.chars();
                    options.comment_char = chars.next();
                    if chars.next().is_some() {
                        return Err("commentChar must contain at most one character".into());
                    }
                }
                "issuePrefixes" => options.issue_prefixes = strings(value, "issuePrefixes")?,
                "issuePrefixesCaseSensitive" => {
                    options.issue_prefixes_case_sensitive = value
                        .as_bool()
                        .ok_or("issuePrefixesCaseSensitive must be Boolean")?
                }
                "referenceActions" => {
                    options.reference_actions = strings(value, "referenceActions")?
                }
                _ => return Err(format!("unsupported native parser option: {key}")),
            }
        }
    }
    Ok(())
}

fn conventional() -> Result<Configuration> {
    let json = r#"{"parserPreset":"conventional-changelog-conventionalcommits","rules":{
      "body-leading-blank":[1,"always"],"body-max-line-length":[2,"always",100],
      "footer-leading-blank":[1,"always"],"footer-max-line-length":[2,"always",100],
      "header-max-length":[2,"always",100],"header-trim":[2,"always"],
      "subject-case":[2,"never",["sentence-case","start-case","pascal-case","upper-case"]],
      "subject-empty":[2,"never"],"subject-full-stop":[2,"never","."],
      "type-case":[2,"always","lower-case"],"type-empty":[2,"never"],
      "type-enum":[2,"always",["build","chore","ci","docs","feat","fix","perf","refactor","revert","style","test"]]}}"#;
    parse_json_config(json)
}

pub fn parse_json_config(text: &str) -> Result<Configuration> {
    parse_configuration(text, false)
}

/// Explicit edit files remain Git-free; JSON can override or disable `#`.
pub(crate) fn parse_edit_configuration(text: &str) -> Result<Configuration> {
    parse_configuration(text, true)
}

fn parse_configuration(text: &str, edit: bool) -> Result<Configuration> {
    let file: ConfigurationFile =
        serde_json::from_str(text).map_err(|e| format!("invalid JSON configuration: {e}"))?;
    let object = &file.fields;
    let mut config = Configuration::default();
    if let Some(extends) = object.get("extends") {
        let names = if let Some(name) = extends.as_str() {
            vec![name.to_owned()]
        } else {
            strings(extends, "extends")?
        };
        for name in names {
            if name != "@commitlint/config-conventional" {
                return Err(format!("unsupported native extends preset: {name}"));
            }
            config = conventional()?;
        }
    }
    for (key, value) in object {
        match key.as_str() {
            "extends" => (),
            "parserPreset" => parser_preset(value, &mut config.parser)?,
            "defaultIgnores" => {
                config.default_ignores = value.as_bool().ok_or("defaultIgnores must be Boolean")?
            }
            "ignores" if value.as_array().is_some_and(Vec::is_empty) => (),
            "rules" => {
                for (name, value) in &file.rules.0 {
                    let spec = parse_rule(name, value)?;
                    if let Some(existing) = config.rules.iter_mut().find(|r| r.name == *name) {
                        *existing = spec;
                    } else {
                        config.rules.push(spec);
                    }
                }
            }
            _ => return Err(format!("unsupported native configuration field: {key}")),
        }
    }
    if edit
        && object
            .get("parserPreset")
            .and_then(|v| v.get("parserOpts"))
            .and_then(|v| v.get("commentChar"))
            .is_none()
    {
        config.parser.comment_char = Some('#');
    }
    validate_configuration(&config)?;
    Ok(config)
}

pub fn validate_configuration(config: &Configuration) -> Result<()> {
    let mut names = std::collections::HashSet::new();
    for spec in &config.rules {
        if !SUPPORTED_RULES.contains(&spec.name.as_str()) {
            return Err(format!("unsupported commitlint rule: {}", spec.name));
        }
        if !names.insert(&spec.name) {
            return Err(format!("duplicate configured rule: {}", spec.name));
        }
        if spec.severity != ConfigSeverity::Disabled {
            if spec.condition.is_none() {
                return Err(format!("rule {} requires a condition", spec.name));
            }
            validate_rule_value(&spec.name, &spec.value)?;
        }
    }
    Ok(())
}

fn ignore_patterns() -> &'static Vec<Regex> {
    static PATTERNS: OnceLock<Vec<Regex>> = OnceLock::new();
    PATTERNS.get_or_init(|| {
        [
            r"(?m)^((Merge pull request)|(Merge (.*?) into (.*?)|(Merge branch (.*?)))(?:\r?\n)*$)",
            r"(?m)^(Merge tag (.*?))(?:\r?\n)*$",
            r"^(R|r)evert (.*)",
            r"^(R|r)eapply (.*)",
            r"^(amend|fixup|squash)!",
            r"^(Merged (.*?)(in|into) (.*)|Merged PR (.*): (.*))",
        r"^Merge remote-tracking branch([\t\n\x0B\x0C\r \x{A0}\x{1680}\x{2000}-\x{200A}\x{2028}\x{2029}\x{202F}\x{205F}\x{3000}\x{FEFF}]*)(.*)",
            r"^Automatic merge(.*)",
            r"^Auto-merged (.*?) into (.*)",
        ]
        .into_iter()
        .map(|p| Regex::new(p).expect("fixed ignore pattern"))
        .collect()
    })
}

pub fn is_default_ignored(raw: &str) -> bool {
    let raw = raw.trim_end_matches(is_js_whitespace);
    // These fixed wildcard patterns use only dot/line anchors and optional
    // LF runs. Normalize all ECMAScript line terminators for their Boolean
    // match so Rust's dot and multiline anchors have the same boundaries.
    let wildcard_input: String = raw
        .chars()
        .map(|c| match c {
            '\r' | '\u{2028}' | '\u{2029}' => '\n',
            _ => c,
        })
        .collect();
    if ignore_patterns()
        .iter()
        .any(|p| p.is_match(&wildcard_input))
    {
        return true;
    }
    static CHORE: OnceLock<Regex> = OnceLock::new();
    static BRACKET: OnceLock<Regex> = OnceLock::new();
    static PAREN: OnceLock<Regex> = OnceLock::new();
    let line = raw.split('\n').next().unwrap_or_default();
    let line = CHORE
        .get_or_init(|| Regex::new(r"^chore(\([^)]+\))?:").unwrap())
        .replace(line, "");
    let line = BRACKET
        .get_or_init(|| Regex::new(r"\[(?i-u:skip|ci)(?:-|[\t\n\x0B\x0C\r \x{A0}\x{1680}\x{2000}-\x{200A}\x{2028}\x{2029}\x{202F}\x{205F}\x{3000}\x{FEFF}])(?i-u:ci|skip)\]").unwrap())
        .replace(&line, "");
    let line = PAREN
        .get_or_init(|| Regex::new(r"\((?i-u:skip|ci)(?:-|[\t\n\x0B\x0C\r \x{A0}\x{1680}\x{2000}-\x{200A}\x{2028}\x{2029}\x{202F}\x{205F}\x{3000}\x{FEFF}])(?i-u:ci|skip)\)").unwrap())
        .replace(&line, "");
    let candidate = line.trim_matches(is_js_whitespace);
    if candidate.len() > 256 {
        return false;
    }
    let candidate = candidate.strip_prefix('v').unwrap_or(candidate);
    semver::Version::parse(candidate).is_ok_and(|v| {
        [v.major, v.minor, v.patch]
            .into_iter()
            .all(|n| n <= 9_007_199_254_740_991)
    })
}

pub fn lint_configured(
    raw: &str,
    config: &Configuration,
    context: &EvaluationContext,
) -> Result<LintOutcome> {
    validate_configuration(config)?;
    if raw.is_empty() || (config.default_ignores && is_default_ignored(raw)) {
        return Ok(LintOutcome {
            valid: true,
            parsed: None,
            errors: vec![],
            warnings: vec![],
        });
    }
    let parsed = parse_message_with_options(raw, &config.parser)?;
    if parsed.header.is_none() && parsed.body.is_none() && parsed.footer.is_none() {
        return Ok(LintOutcome {
            valid: true,
            parsed: Some(parsed),
            errors: vec![],
            warnings: vec![],
        });
    }
    let mut errors = vec![];
    let mut warnings = vec![];
    for spec in &config.rules {
        if spec.severity == ConfigSeverity::Disabled {
            continue;
        }
        let result = evaluate_rule_with_context(
            &spec.name,
            &parsed,
            raw,
            spec.condition,
            &spec.value,
            context,
        )?;
        if !result.valid {
            let severity = if spec.severity == ConfigSeverity::Warning {
                Severity::Warning
            } else {
                Severity::Error
            };
            let diagnostic = Diagnostic {
                name: spec.name.clone(),
                severity,
                message: result.message.unwrap_or_default(),
            };
            if severity == Severity::Warning {
                warnings.push(diagnostic);
            } else {
                errors.push(diagnostic);
            }
        }
    }
    Ok(LintOutcome {
        valid: errors.is_empty(),
        parsed: Some(parsed),
        errors,
        warnings,
    })
}
