//! Fresh human identity and raw Git object/live outgoing-history checks.
use crate::{Config, Identity, Result, policy, util};
use regex::Regex;
use serde::Serialize;
use std::{
    collections::BTreeSet,
    fs,
    io::{self, Read},
    path::{Path, PathBuf},
    sync::OnceLock,
    time::Duration,
};

#[derive(Clone)]
pub struct Tools {
    pub git: PathBuf,
    pub gh: PathBuf,
}
pub fn tools(cfg: Option<&Config>) -> Result<Tools> {
    if let Some(cfg) = cfg {
        Ok(Tools {
            git: cfg.git.clone(),
            gh: cfg.gh.clone(),
        })
    } else {
        Ok(Tools {
            git: util::find_tool("git")?,
            gh: util::find_tool("gh")?,
        })
    }
}
pub fn query(git: &Path, args: &[String], input: Option<&[u8]>) -> Result<Vec<u8>> {
    let output = util::capture(git, args, input, Duration::from_secs(60))?;
    if output.code != 0 {
        return Err("cannot inspect Git state safely".into());
    }
    Ok(output.stdout)
}
pub fn text(bytes: &[u8]) -> Result<&str> {
    std::str::from_utf8(bytes).map_err(|_| "Git/authentication output is not UTF-8".into())
}
pub fn git_text(git: &Path, args: &[&str]) -> Result<String> {
    Ok(text(&query(
        git,
        &args.iter().map(|v| v.to_string()).collect::<Vec<_>>(),
        None,
    )?)?
    .trim()
    .to_string())
}
pub fn oid_valid(value: &str) -> bool {
    matches!(value.len(), 40 | 64)
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
pub fn account(tools: &Tools) -> Result<Identity> {
    let args = ["api", "--hostname", "github.com", "user"].map(str::to_string);
    let output = util::capture(&tools.gh, &args, None, Duration::from_secs(15))?;
    if output.code != 0 {
        return Err("fresh gh authentication failed; no cached identity is accepted".into());
    }
    let user: serde_json::Value =
        serde_json::from_slice(&output.stdout).map_err(|_| "invalid gh authentication response")?;
    if user.get("type").and_then(|v| v.as_str()) != Some("User") {
        return Err("gh authentication must identify a human User".into());
    }
    let login = user
        .get("login")
        .and_then(|v| v.as_str())
        .ok_or("invalid gh login")?;
    if login.is_empty()
        || login.len() > 39
        || !login.as_bytes()[0].is_ascii_alphanumeric()
        || !login
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-')
    {
        return Err("invalid gh login".into());
    }
    let id = user
        .get("id")
        .and_then(|v| v.as_u64())
        .filter(|id| *id > 0 && *id <= 9_007_199_254_740_991)
        .ok_or("invalid gh user ID")?;
    Ok(Identity {
        login: login.into(),
        email: format!("{id}+{login}@users.noreply.github.com"),
    })
}
pub fn check_ident(value: &str, identity: &Identity, label: &str) -> Result<()> {
    static RE: OnceLock<Regex> = OnceLock::new();
    let capture = RE
        .get_or_init(|| Regex::new(r"^(.*?) <([^<>]+)> [0-9]+ [+-][0-9]{4}$").unwrap())
        .captures(value)
        .ok_or_else(|| format!("invalid {label} identity"))?;
    if capture.get(1).map(|v| v.as_str()) != Some(identity.login.as_str())
        || capture.get(2).map(|v| v.as_str()) != Some(identity.email.as_str())
    {
        return Err(format!("{label} must match fresh human gh identity"));
    }
    Ok(())
}
pub fn check_effective(git: &Path, identity: &Identity, both: bool) -> Result<()> {
    for label in if both { &['A', 'C'][..] } else { &['C'][..] } {
        let field = if *label == 'A' {
            "GIT_AUTHOR_IDENT"
        } else {
            "GIT_COMMITTER_IDENT"
        };
        check_ident(&git_text(git, &["var", field])?, identity, field)?;
    }
    Ok(())
}
pub fn resolve_commit(git: &Path, reference: &str) -> Result<Option<String>> {
    let output = util::capture(
        git,
        &[
            "rev-parse".into(),
            "--verify".into(),
            "--end-of-options".into(),
            format!("{reference}^{{commit}}"),
        ],
        None,
        Duration::from_secs(15),
    )?;
    if output.code != 0 {
        return Ok(None);
    }
    let oid = text(&output.stdout)?.trim();
    if !oid_valid(oid) {
        return Err("invalid resolved commit identifier".into());
    }
    Ok(Some(oid.into()))
}
pub fn validate_message_with_git(git: &Path, bytes: &[u8], identity: &Identity) -> Result<()> {
    policy::validate_message(bytes, identity)?;
    validate_trailers_with_git(git, bytes, identity)
}
pub fn validate_trailers_with_git(git: &Path, bytes: &[u8], identity: &Identity) -> Result<()> {
    policy::validate_attribution(bytes, identity)?;
    let parsed = query(
        git,
        &[
            "-c",
            "trailer.separators=:",
            "-c",
            "trailer.co-authored-by.key=Co-authored-by",
            "-c",
            "trailer.signed-off-by.key=Signed-off-by",
            "interpret-trailers",
            "--parse",
        ]
        .map(str::to_string),
        Some(bytes),
    )?;
    policy::validate_attribution(&parsed, identity)
}
pub fn validate_commit_bytes(oid: &str, bytes: &[u8], identity: &Identity) -> Result<()> {
    let raw = text(bytes)?;
    let (headers, message) = raw.split_once("\n\n").ok_or("malformed raw commit")?;
    if headers.contains(['\r', '\0']) {
        return Err(format!("malformed raw commit headers in {oid}"));
    }
    for line in headers.lines() {
        let key = line.split(' ').next().unwrap_or_default();
        if ["encoding", "gpgsig", "gpgsig-sha256"].contains(&key) {
            return Err(format!("unsupported encoding or signature in {oid}"));
        }
    }
    for label in ["author", "committer"] {
        let prefix = format!("{label} ");
        let entries = headers
            .lines()
            .filter(|line| line.split(' ').next() == Some(label))
            .collect::<Vec<_>>();
        if entries.len() != 1 {
            return Err(format!("missing or duplicate {label} in {oid}"));
        }
        let value = entries[0]
            .strip_prefix(&prefix)
            .ok_or_else(|| format!("malformed {label} in {oid}"))?;
        check_ident(value, identity, label).map_err(|_| format!("{label} mismatch in {oid}"))?;
    }
    policy::validate_message(message.as_bytes(), identity)
        .map_err(|error| format!("{oid}: {error}"))
}
pub fn validate_commit(git: &Path, oid: &str, identity: &Identity) -> Result<()> {
    if !oid_valid(oid) {
        return Err("invalid commit identifier".into());
    }
    let raw = query(git, &["cat-file".into(), "commit".into(), oid.into()], None)?;
    validate_commit_bytes(oid, &raw, identity)?;
    let (_, message) = text(&raw)?
        .split_once("\n\n")
        .ok_or("malformed raw commit")?;
    validate_trailers_with_git(git, message.as_bytes(), identity)
        .map_err(|error| format!("{oid}: {error}"))
}
pub fn history_safety(git: &Path) -> Result<()> {
    if git_text(git, &["rev-parse", "--is-shallow-repository"])? == "true" {
        return Err("shallow history cannot be checked; fetch --unshallow".into());
    }
    if Path::new(&git_text(git, &["rev-parse", "--git-path", "info/grafts"])?).exists() {
        return Err("grafted history cannot be safely checked".into());
    }
    Ok(())
}
#[derive(Serialize, Clone)]
pub struct Source {
    pub reference: String,
    pub oid: String,
    pub destination: Option<String>,
}
#[derive(Serialize)]
pub struct Outgoing {
    pub checked: Vec<String>,
    pub sources: Vec<Source>,
    pub destinations: usize,
}
fn check_ref(git: &Path, reference: &str) -> Result<bool> {
    if !reference.starts_with("refs/") {
        return Ok(false);
    }
    Ok(util::capture(
        git,
        &["check-ref-format".into(), reference.into()],
        None,
        Duration::from_secs(15),
    )?
    .code
        == 0)
}
pub fn pre_push_sources(git: &Path, payload: &[u8]) -> Result<Vec<Source>> {
    let input = text(payload)?;
    let mut sources = Vec::new();
    let mut destinations = BTreeSet::new();
    for line in input.split_terminator('\n') {
        let fields: Vec<_> = line.split(' ').collect();
        if fields.len() != 4 {
            return Err("malformed pre-push update".into());
        }
        let (reference, oid, destination, old) = (fields[0], fields[1], fields[2], fields[3]);
        if !oid_valid(oid)
            || !oid_valid(old)
            || oid.len() != old.len()
            || !check_ref(git, destination)?
            || !destinations.insert(destination)
        {
            return Err("malformed pre-push ref/OID update".into());
        }
        if oid.bytes().all(|b| b == b'0') {
            if reference != "(delete)" {
                return Err("invalid pre-push deletion".into());
            }
            continue;
        }
        // Git can pass a rev-expression such as HEAD~1 as the source label.
        // The immutable supplied OID remains authoritative; never resolve this
        // label during validation, because its ref may already have moved.
        if reference.is_empty()
            || reference.starts_with('-')
            || reference
                .chars()
                .any(|c| c.is_control() || c.is_whitespace())
        {
            return Err("invalid pre-push source label".into());
        }
        if oid_valid(reference) && reference != oid {
            return Err("pre-push source identifier mismatch".into());
        }
        if !old.bytes().all(|b| b == b'0') && resolve_commit(git, old)?.is_none() {
            return Err("remote base missing locally; fetch before pushing".into());
        }
        // Peel the immutable object Git supplied, not a concurrently moved ref.
        resolve_commit(git, oid)?.ok_or("pushed sources must peel to commits")?;
        sources.push(Source {
            reference: reference.into(),
            oid: oid.into(),
            destination: Some(destination.into()),
        });
    }
    Ok(sources)
}
pub fn outgoing_at(git: &Path, urls: &[String], sources: Vec<Source>) -> Result<Outgoing> {
    history_safety(git)?;
    if urls.is_empty()
        || urls
            .iter()
            .any(|url| url.is_empty() || url.contains(['\r', '\n', '\0']))
    {
        return Err("push destination is unresolved".into());
    }
    let revisions = sources
        .iter()
        .map(|source| {
            resolve_commit(git, &source.oid)?
                .ok_or_else(|| "pushed source must peel to a commit".to_string())
        })
        .collect::<Result<Vec<_>>>()?;
    let mut commits = BTreeSet::new();
    for url in urls {
        let advertised = query(
            git,
            &[
                "ls-remote".into(),
                "--refs".into(),
                "--".into(),
                url.clone(),
            ],
            None,
        )?;
        let mut excluded = BTreeSet::new();
        for line in text(&advertised)?.lines() {
            let Some((oid, reference)) = line.split_once('\t') else {
                return Err("malformed live ref advertisement".into());
            };
            if !oid_valid(oid) || !check_ref(git, reference)? {
                return Err("malformed live ref advertisement".into());
            }
            if let Some(known) = resolve_commit(git, oid)? {
                excluded.insert(known);
            }
            // Unknown remote tips cannot excuse history; never trust tracking refs.
        }
        if revisions.is_empty() {
            continue;
        }
        let mut traversal = revisions.join("\n");
        traversal.push('\n');
        for oid in excluded {
            traversal.push('^');
            traversal.push_str(&oid);
            traversal.push('\n');
        }
        let listed = query(
            git,
            &["rev-list".into(), "--stdin".into()],
            Some(traversal.as_bytes()),
        )?;
        for oid in text(&listed)?.lines() {
            if !oid_valid(oid) {
                return Err("invalid outgoing commit identifier".into());
            }
            commits.insert(oid.into());
        }
    }
    Ok(Outgoing {
        checked: commits.into_iter().collect(),
        sources,
        destinations: urls.len(),
    })
}
pub fn validate_pre_push(cfg: &Config, args: &[String], payload: &[u8]) -> Result<()> {
    if args.len() != 2 {
        return Err("pre-push requires remote name and exact destination URL".into());
    }
    let tools = tools(Some(cfg))?;
    let identity = account(&tools)?;
    let report = outgoing_at(
        &tools.git,
        &[args[1].clone()],
        pre_push_sources(&tools.git, payload)?,
    )?;
    for oid in report.checked {
        validate_commit(&tools.git, &oid, &identity)?;
    }
    Ok(())
}
pub fn check(mode: &str, args: &[String], cfg: Option<&Config>) -> Result<()> {
    let tools = tools(cfg)?;
    let identity = account(&tools)?;
    if mode == "account" || mode == "identity" {
        if !args.is_empty() {
            return Err("identity/account take no arguments".into());
        }
        if mode == "identity" {
            check_effective(&tools.git, &identity, true)?;
        }
        println!(
            "{}",
            serde_json::to_string(&identity).map_err(|_| "cannot serialize identity")?
        );
        return Ok(());
    }
    let mut report = Outgoing {
        checked: Vec::new(),
        sources: Vec::new(),
        destinations: 0,
    };
    match mode {
        "message" => {
            if args.len() == 2 && args[0] == "--clean" {
                check_effective(&tools.git, &identity, true)?;
                crate::hooks::clean_message(&tools.git, Path::new(&args[1]), &identity)?;
            } else {
                if args.len() != 1 {
                    return Err("message requires one file or --clean FILE".into());
                }
                let bytes = fs::read(&args[0]).map_err(|_| "cannot read message file")?;
                validate_message_with_git(&tools.git, &bytes, &identity)?;
            }
            report.checked.push("candidate".into());
        }
        "commits" => {
            if args.is_empty() {
                return Err("commits requires explicit source refs".into());
            }
            for reference in args {
                let oid = resolve_commit(&tools.git, reference)?.ok_or("source is not a commit")?;
                validate_commit(&tools.git, &oid, &identity)?;
                report.checked.push(oid);
            }
        }
        "push" => {
            if args.len() < 2 || args[0].starts_with('-') {
                return Err("push requires a destination and explicit source refs".into());
            }
            let named = util::capture(
                &tools.git,
                &[
                    "remote".into(),
                    "get-url".into(),
                    "--push".into(),
                    "--all".into(),
                    args[0].clone(),
                ],
                None,
                Duration::from_secs(15),
            )?;
            let urls = if named.code == 0 {
                text(&named.stdout)?.lines().map(str::to_string).collect()
            } else {
                vec![args[0].clone()]
            };
            let sources = args[1..]
                .iter()
                .map(|reference| {
                    Ok(Source {
                        reference: reference.clone(),
                        oid: resolve_commit(&tools.git, reference)?
                            .ok_or("source is not a commit")?,
                        destination: None,
                    })
                })
                .collect::<Result<Vec<_>>>()?;
            report = outgoing_at(&tools.git, &urls, sources)?;
            for oid in &report.checked {
                validate_commit(&tools.git, oid, &identity)?;
            }
        }
        "pre-push" => {
            if args.len() != 1 {
                return Err("pre-push requires exact destination URL".into());
            }
            let mut payload = Vec::new();
            io::stdin()
                .read_to_end(&mut payload)
                .map_err(|_| "cannot read pre-push updates")?;
            report = outgoing_at(&tools.git, args, pre_push_sources(&tools.git, &payload)?)?;
            for oid in &report.checked {
                validate_commit(&tools.git, oid, &identity)?;
            }
        }
        _ => return Err("unknown native checker mode".into()),
    }
    println!(
        "{}",
        serde_json::json!({"valid":true,"identity":identity,"checked":report.checked,"sources":report.sources,"destinations":report.destinations})
    );
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn raw_identity_and_header_validation() {
        let identity = Identity {
            login: "tester".into(),
            email: "44+tester@users.noreply.github.com".into(),
        };
        let good = "tree abc\nauthor tester <44+tester@users.noreply.github.com> 123 +0000\ncommitter tester <44+tester@users.noreply.github.com> 124 +0000\n\nfix: change\n";
        assert!(validate_commit_bytes("fixture", good.as_bytes(), &identity).is_ok());
        for bad in [
            good.replace("committer tester", "committer Other"),
            good.replace("\n\n", "\nencoding UTF-8\n\n"),
            good.replace("\n\n", "\ngpgsig fixture\n\n"),
            good.replace(
                "\n\n",
                "\nauthor tester <44+tester@users.noreply.github.com> 123 +0000\n\n",
            ),
            good.replace("\n\n", "\nauthor\n\n"),
        ] {
            assert!(validate_commit_bytes("fixture", bad.as_bytes(), &identity).is_err());
        }
    }
    #[test]
    fn raw_identity_whitespace_is_not_normalized() {
        let identity = Identity {
            login: "tester".into(),
            email: "44+tester@users.noreply.github.com".into(),
        };
        let value = "tester <44+tester@users.noreply.github.com> 123 +0000";
        assert!(check_ident(value, &identity, "author").is_ok());
        for invalid in [
            format!(" {value}"),
            format!("{value} "),
            format!("{value}\r"),
        ] {
            assert!(check_ident(&invalid, &identity, "author").is_err());
        }
        let raw = format!("tree abc\r\nauthor {value}\r\ncommitter {value}\r\n\nfix: valid\n");
        assert!(validate_commit_bytes("fixture", raw.as_bytes(), &identity).is_err());
    }
}
