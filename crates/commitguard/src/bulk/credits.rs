//! Pure Rust attribution parsing. No Git processes or provider inference.
use crate::{Result, policy};
use regex::Regex;
use std::{collections::BTreeMap, sync::OnceLock};

pub(super) type Actor = (String, String, String, String, String);
pub(super) type Counter = BTreeMap<Actor, usize>;
pub(super) struct Block {
    pub raw: String,
    pub key: String,
    pub value: String,
    pub ai: Option<Counter>,
}
fn actor(provider: &str, tool: &str, model: &str, version: &str, context: &str) -> Actor {
    (
        provider.into(),
        tool.into(),
        model.into(),
        version.into(),
        context.into(),
    )
}
pub(super) fn add(counter: &mut Counter, values: &Counter) {
    for (key, count) in values {
        *counter.entry(key.clone()).or_default() += count;
    }
}
fn singleton(value: Actor) -> Counter {
    BTreeMap::from([(value, 1)])
}
fn claude(name: &str) -> Option<Actor> {
    static RE: OnceLock<Regex> = OnceLock::new();
    let captures = RE.get_or_init(|| Regex::new(r"^claude(?: +(opus|sonnet|haiku|fable)(?: +([0-9]+(?:\.[0-9]+)*))?(?: +\(([0-9]+)m context\))?)?$").unwrap()).captures(name)?;
    Some(actor(
        "anthropic",
        "claude",
        captures.get(1).map_or("", |m| m.as_str()),
        captures.get(2).map_or("", |m| m.as_str()),
        &captures
            .get(3)
            .map_or(String::new(), |m| format!("{}m", m.as_str())),
    ))
}
fn gpt(name: &str) -> Option<Actor> {
    static RE: OnceLock<Regex> = OnceLock::new();
    let captures = RE
        .get_or_init(|| Regex::new(r"^gpt(?:[ -]?([0-9]+(?:\.[0-9]+)*))?(?: +(astra))?$").unwrap())
        .captures(name)?;
    Some(actor(
        "openai",
        "gpt",
        captures.get(2).map_or("", |m| m.as_str()),
        captures.get(1).map_or("", |m| m.as_str()),
        "",
    ))
}
pub(super) fn standard(value: &str) -> Option<Actor> {
    let value = value.split_whitespace().collect::<Vec<_>>().join(" ");
    if value.eq_ignore_ascii_case("codex") {
        return Some(actor("openai", "codex", "", "", ""));
    }
    if !policy::recognized_ai(&value) {
        return None;
    }
    let (name, provider) = value.strip_suffix('>')?.rsplit_once(" <")?;
    let name = name.to_ascii_lowercase();
    match provider.to_ascii_lowercase().as_str() {
        "noreply@anthropic.com" => claude(&name),
        "noreply@openai.com" => match name.as_str() {
            "codex" | "chatgpt" => Some(actor("openai", &name, "", "", "")),
            _ => gpt(&name),
        },
        "cursoragent@cursor.com" if name == "cursor" => Some(actor("cursor", "cursor", "", "", "")),
        "https://github.com/qwenlm/qwen-code" if name == "qwen-cli" => {
            Some(actor("qwen", "qwen-cli", "", "", ""))
        }
        _ => None,
    }
}
pub(super) fn checked_standard(value: &str) -> Result<Option<Actor>> {
    let parsed = standard(value);
    let normalized = value.split_whitespace().collect::<Vec<_>>().join(" ");
    if parsed.is_none() && policy::recognized_ai(&normalized) {
        return Err("recognized AI credit has unsupported model/version/context syntax".into());
    }
    Ok(parsed)
}
const MAX_MULTIPLICITY: usize = 100_000;
const CLAUDE_CODE_FOOTER: &str = "🤖 Generated with [Claude Code](https://claude.com/claude-code)";
fn compact(value: &str) -> Result<Counter> {
    // A multiplicity suffix belongs to the complete, single actor, never to
    // a shared model prefix or one version in a slash list. No nested weights.
    if let Some((base, count)) = value.rsplit_once(" x") {
        if base.is_empty()
            || base.ends_with(char::is_whitespace)
            || count.is_empty()
            || count.starts_with('0')
            || !count.bytes().all(|byte| byte.is_ascii_digit())
        {
            return Err(
                "compact AI multiplicity requires a canonical positive decimal count".into(),
            );
        }
        let count = count
            .parse::<usize>()
            .ok()
            .filter(|count| *count <= MAX_MULTIPLICITY)
            .ok_or("compact AI multiplicity exceeds safe bounds")?;
        let mut parsed = compact_unweighted(base)?;
        if parsed.len() != 1 || parsed.values().next() != Some(&1) {
            return Err("compact AI multiplicity requires exactly one unweighted actor".into());
        }
        *parsed.values_mut().next().unwrap() = count;
        return Ok(parsed);
    }
    compact_unweighted(value)
}
fn compact_unweighted(value: &str) -> Result<Counter> {
    if value.is_empty() || !value.bytes().all(|b| (b' '..=b'~').contains(&b)) {
        return Err("invalid compact AI credit bytes".into());
    }
    let value = value.to_ascii_lowercase();
    let mut result = Counter::new();
    let mut anthropic = false;
    let mut model = None;
    let mut family_prefix = false;
    for segment in value.split([';', ',']) {
        let mut segment = segment.trim();
        if segment.is_empty() {
            return Err("empty compact AI credit segment".into());
        }
        if segment == "claude code" {
            add(
                &mut result,
                &singleton(actor("anthropic", "claude-code", "", "", "")),
            );
            anthropic = false;
            model = None;
            continue;
        }
        if let Some(rest) = segment
            .strip_prefix("claude")
            .filter(|rest| rest.is_empty() || rest.starts_with(' '))
        {
            anthropic = true;
            model = None;
            segment = rest.trim();
            if segment.is_empty() {
                family_prefix = true;
                continue;
            }
        }
        if ["codex", "chatgpt", "cursor", "qwen-cli", "qwen cli"].contains(&segment) {
            let key = match segment {
                "codex" | "chatgpt" => actor("openai", segment, "", "", ""),
                "cursor" => actor("cursor", "cursor", "", "", ""),
                _ => actor("qwen", "qwen-cli", "", "", ""),
            };
            add(&mut result, &singleton(key));
            anthropic = false;
            model = None;
            continue;
        }
        if segment.starts_with("gpt") {
            let key = gpt(segment).ok_or("unsupported compact GPT credit syntax")?;
            add(&mut result, &singleton(key));
            anthropic = false;
            model = None;
            continue;
        }
        for name in ["opus", "sonnet", "haiku", "fable"] {
            if let Some(rest) = segment
                .strip_prefix(name)
                .filter(|rest| rest.is_empty() || rest.starts_with(' '))
            {
                if !anthropic {
                    return Err("compact Claude model requires an explicit provider prefix".into());
                }
                model = Some(name);
                segment = rest.trim();
                break;
            }
        }
        let model = model
            .filter(|_| anthropic)
            .ok_or("unsupported compact AI credit syntax")?;
        if segment.is_empty() {
            add(
                &mut result,
                &singleton(actor("anthropic", "claude", model, "", "")),
            );
            continue;
        }
        static VERSION: OnceLock<Regex> = OnceLock::new();
        let regex = VERSION.get_or_init(|| {
            Regex::new(r"^([0-9]+(?:\.[0-9]+)*)(?:-([0-9]+)m| *\(([0-9]+)m context\))?$").unwrap()
        });
        for version in segment.split('/') {
            let captures = regex
                .captures(version.trim())
                .ok_or("unsupported compact AI version/context syntax")?;
            let context = captures
                .get(2)
                .or_else(|| captures.get(3))
                .map_or(String::new(), |m| format!("{}m", m.as_str()));
            add(
                &mut result,
                &singleton(actor(
                    "anthropic",
                    "claude",
                    model,
                    captures.get(1).unwrap().as_str(),
                    &context,
                )),
            );
        }
    }
    // A standalone Claude prefix is a family scope when it qualifies models,
    // rather than a second invented actor. Bare Claude alone remains a credit.
    if family_prefix
        && !result
            .keys()
            .any(|key| key.0 == "anthropic" && key.1 == "claude")
    {
        add(
            &mut result,
            &singleton(actor("anthropic", "claude", "", "", "")),
        );
    }
    if result.is_empty() {
        return Err("empty compact AI credit".into());
    }
    Ok(result)
}
fn compact_key(key: &str) -> bool {
    matches!(
        key,
        "ai-credit" | "ai-credits" | "ai credit" | "ai credits" | "ai"
    )
}
pub(super) fn parse(block: &str) -> Result<Block> {
    if block == CLAUDE_CODE_FOOTER {
        return Ok(Block {
            raw: block.into(),
            key: "generated-with".into(),
            value: "Claude Code".into(),
            ai: Some(singleton(actor("anthropic", "claude-code", "", "", ""))),
        });
    }
    if block.ends_with('\n') || block.contains('\r') {
        return Err("invalid bulk credit block".into());
    }
    let mut lines = block.split('\n');
    let (key, value) = lines
        .next()
        .and_then(|v| v.trim_start().split_once(':'))
        .ok_or("invalid bulk credit block")?;
    let key = key.trim().to_ascii_lowercase();
    if !["co-authored-by", "signed-off-by"].contains(&key.as_str()) && !compact_key(&key) {
        return Err("invalid bulk credit key".into());
    }
    let mut value = value.trim().to_string();
    for line in lines {
        if line.is_empty() || !line.starts_with([' ', '\t']) {
            return Err("invalid bulk credit continuation".into());
        }
        value.push(' ');
        value.push_str(line.trim());
    }
    let ai = if compact_key(&key) {
        Some(compact(&value)?)
    } else {
        checked_standard(&value)?.map(singleton)
    };
    Ok(Block {
        raw: block.into(),
        key,
        value,
        ai,
    })
}
pub(super) fn scan(message: &str) -> Result<Vec<Block>> {
    let lines: Vec<_> = message.split('\n').collect();
    let mut blocks = Vec::new();
    let mut index = 0;
    while index < lines.len() {
        if lines[index] == CLAUDE_CODE_FOOTER {
            blocks.push(parse(lines[index])?);
            index += 1;
            continue;
        }
        let trimmed = lines[index].trim_start();
        let lower = trimmed.to_ascii_lowercase();
        if ["ai-credit", "ai credit"]
            .iter()
            .any(|prefix| lower.starts_with(prefix))
            && !trimmed
                .split_once(':')
                .is_some_and(|(key, _)| compact_key(&key.trim().to_ascii_lowercase()))
        {
            return Err("malformed compact AI credit key or separator".into());
        }
        if let Some((key, _)) = lines[index].trim_start().split_once(':') {
            let key = key.trim().to_ascii_lowercase();
            if ["co-authored-by", "signed-off-by"].contains(&key.as_str()) || compact_key(&key) {
                let start = index;
                index += 1;
                while index < lines.len()
                    && !lines[index].is_empty()
                    && lines[index].starts_with([' ', '\t'])
                {
                    index += 1;
                }
                blocks.push(parse(&lines[start..index].join("\n"))?);
                continue;
            }
        }
        index += 1;
    }
    Ok(blocks)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn compact_context_belongs_only_to_its_version_and_counts_duplicates() {
        let parsed = compact("Claude Opus 4.8-1M/5/4.7; Fable 5; GPT-6 Astra; Codex").unwrap();
        assert_eq!(
            parsed.get(&actor("anthropic", "claude", "opus", "4.8", "1m")),
            Some(&1)
        );
        assert_eq!(
            parsed.get(&actor("anthropic", "claude", "opus", "5", "")),
            Some(&1)
        );
        assert_eq!(
            parsed.get(&actor("anthropic", "claude", "fable", "5", "")),
            Some(&1)
        );
        assert_eq!(
            parsed.get(&actor("openai", "gpt", "astra", "6", "")),
            Some(&1)
        );
        assert_eq!(
            compact("Claude Opus 5/5").unwrap().values().sum::<usize>(),
            2
        );
        assert!(compact("Opus 5").is_err());
        assert!(compact("Claude Opus 5; Unknown 1").is_err());
        assert!(compact("Claude Opus 5-").is_err());
        assert!(scan("fix: test\nAI-credit Claude Opus 5\n\nbody").is_err());
        assert!(scan("fix: test\nAI-credit: Unknown 5\n\nbody").is_err());
    }
    #[test]
    fn standard_and_compact_preserve_provider_model_context() {
        let source = standard("Claude Opus 4.8 (1m context) <noreply@anthropic.com>").unwrap();
        assert_eq!(compact("Claude Opus 4.8-1M").unwrap(), singleton(source));
        assert!(standard("Claude Opus 4.8 <other@example.com>").is_none());
        assert_ne!(
            compact("Claude Opus 4.8-1M").unwrap(),
            compact("Claude Opus 4.8").unwrap()
        );
        assert_eq!(
            compact("GPT 5 Astra").unwrap(),
            singleton(standard("GPT 5 Astra <noreply@openai.com>").unwrap())
        );
        assert!(policy::recognized_ai(
            "Claude Opus 5..5 <noreply@anthropic.com>"
        ));
        assert!(checked_standard("Claude Opus 5..5 <noreply@anthropic.com>").is_err());
        assert!(
            scan("fix: test\n\nCo-authored-by: Claude Opus 5..5 <noreply@anthropic.com>").is_err()
        );
    }
    #[test]
    fn weighted_single_actor_counts_are_exact_and_canonical() {
        let weighted = compact("Claude Opus 5.5 x141").unwrap();
        assert_eq!(
            weighted.get(&actor("anthropic", "claude", "opus", "5.5", "")),
            Some(&141)
        );
        assert_eq!(
            compact("Claude Opus 5.5-1M x7").unwrap().get(&actor(
                "anthropic",
                "claude",
                "opus",
                "5.5",
                "1m"
            )),
            Some(&7)
        );
        assert_eq!(compact("Codex x1").unwrap(), compact("Codex").unwrap());
        assert_eq!(
            compact("Codex x100000").unwrap().values().next(),
            Some(&100000)
        );
        for invalid in [
            "Claude Opus 5.5 x0",
            "Claude Opus 5.5 x-1",
            "Claude Opus 5.5 x+1",
            "Claude Opus 5.5 x01",
            "Claude Opus 5.5 x1.0",
            "Claude Opus 5.5 x",
            "Claude Opus 5.5  x7",
            "Claude Opus 5.5 x100001",
            "Claude Opus 5.5 x184467440737095516160",
            "Claude Opus 5.5/5.5 x7",
            "Claude Opus 5.5; Codex x7",
            "Claude Opus 5.5 x7 x2",
            "Claude Opus 5.5 x7-1M",
            "Claude Opus 5.5 x7 (1m context)",
        ] {
            assert!(compact(invalid).is_err(), "{invalid}");
        }
    }
    #[test]
    fn only_the_exact_evidenced_claude_code_footer_is_a_credit() {
        let blocks = scan(&format!(
            "old subject\n{CLAUDE_CODE_FOOTER}\n\nbody follows"
        ))
        .unwrap();
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].key, "generated-with");
        assert_eq!(blocks[0].raw, CLAUDE_CODE_FOOTER);
        assert_eq!(blocks[0].ai, parse("AI-credit: Claude Code").unwrap().ai);
        for unsupported in [
            "Generated with Claude Code",
            "🤖 Generated with [Claude Code](https://example.com/claude-code)",
            "🤖 Generated with [Claude Code](https://claude.com/claude-code?token=secret)",
            "🤖 Generated with [Claude Code](https://claude.com/claude-code) extra",
        ] {
            assert!(parse(unsupported).is_err());
            assert!(scan(unsupported).unwrap().is_empty());
        }
        let duplicate = scan(&format!("{CLAUDE_CODE_FOOTER}\n{CLAUDE_CODE_FOOTER}")).unwrap();
        let mut counts = Counter::new();
        for block in duplicate {
            add(&mut counts, &block.ai.unwrap());
        }
        assert_eq!(counts, compact("Claude Code x2").unwrap());
    }
}
