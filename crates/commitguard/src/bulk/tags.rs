//! Existing annotated-tag metadata writer; never promotes refs or runs hooks.
use super::{
    Approval, CreditChange, Entry, MAX_BYTES, MAX_COMMIT, MAX_ENTRIES, MAX_MANIFEST,
    MainCreditChange, Manifest, Mapping, Report, check_hash, feed, git::Git, state, validate,
};
use crate::{Config, Identity, Result, auth, core, policy};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TagManifest {
    schema_version: u32,
    policy_version: u32,
    common_dir: PathBuf,
    commit_receipt: Option<PathBuf>,
    entries: Vec<TagEntry>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TagEntry {
    source_oid: String,
    source_sha256: String,
    expected_oid: String,
    candidate_file: PathBuf,
    candidate_sha256: String,
    tagger_ownership: Option<TaggerOwnership>,
    #[serde(default)]
    credit_changes: Vec<CreditChange>,
    #[serde(default)]
    main_credit_changes: Vec<MainCreditChange>,
    #[serde(default)]
    remove_signature: bool,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TaggerOwnership {
    old_tagger: String,
    owned: bool,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CommitReceipt {
    digest: String,
    phase: String,
    common_dir: PathBuf,
    mapping: Vec<ReceiptMapping>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReceiptMapping {
    source_oid: String,
    new_oid: String,
}
struct Tag<'a> {
    target: &'a str,
    kind: &'a str,
    name: &'a str,
    tagger: &'a str,
    message: &'a str,
}
fn tag(raw: &[u8], width: usize) -> Result<Tag<'_>> {
    let raw = core::text(raw)?;
    let (header, message) = raw.split_once("\n\n").ok_or("malformed bulk tag")?;
    if header.contains(['\r', '\0']) || message.contains('\0') {
        return Err("malformed bulk tag bytes".into());
    }
    let lines: Vec<&str> = header.split('\n').collect();
    if lines.len() != 4 {
        return Err("unsupported or duplicate bulk tag headers".into());
    }
    let target = lines[0]
        .strip_prefix("object ")
        .ok_or("bulk tag object header must be first")?;
    if target.len() != width || !core::oid_valid(target) {
        return Err("invalid bulk tag target OID".into());
    }
    let kind = lines[1]
        .strip_prefix("type ")
        .filter(|v| matches!(*v, "commit" | "tag"))
        .ok_or("bulk tags must ultimately target commits")?;
    let name = lines[2]
        .strip_prefix("tag ")
        .filter(|v| !v.is_empty() && !v.chars().any(char::is_control))
        .ok_or("invalid bulk tag name")?;
    let tagger = lines[3]
        .strip_prefix("tagger ")
        .ok_or("bulk tag requires an exact tagger")?;
    validate::date(tagger)?;
    Ok(Tag {
        target,
        kind,
        name,
        tagger,
        message,
    })
}
const ARMOR: [(&str, &str); 2] = [
    (
        "-----BEGIN PGP SIGNATURE-----",
        "-----END PGP SIGNATURE-----",
    ),
    (
        "-----BEGIN SSH SIGNATURE-----",
        "-----END SSH SIGNATURE-----",
    ),
];
fn unsigned(message: &str, permission: bool) -> Result<&str> {
    if message.contains("-----BEGIN SIGNED MESSAGE-----")
        || message.contains("-----END SIGNED MESSAGE-----")
    {
        return Err("unsupported X.509 tag signature".into());
    }
    let mut signature = None;
    for (begin, end) in ARMOR {
        if let Some(start) = message.find(begin) {
            if signature.is_some()
                || (start > 0 && message.as_bytes()[start - 1] != b'\n')
                || message[start + begin.len()..].contains(begin)
            {
                return Err("ambiguous tag signature armor".into());
            }
            let tail = &message[start..];
            if !tail.starts_with(&format!("{begin}\n")) {
                return Err("malformed tag signature opening".into());
            }
            let finish = tail.find(end).ok_or("unterminated tag signature")?;
            if finish == 0
                || tail.as_bytes()[finish - 1] != b'\n'
                || !tail[finish + end.len()..].bytes().all(|b| b == b'\n')
            {
                return Err("tag signature must be terminal complete armor".into());
            }
            signature = Some(start);
        } else if message.contains(end) {
            return Err("orphan tag signature ending".into());
        }
    }
    match (signature, permission) {
        (Some(start), true) => Ok(&message[..start]),
        (Some(_), false) => Err("tag signature removal requires explicit permission".into()),
        (None, true) => Err("tag signature removal permission does not match actual source".into()),
        (None, false) => Ok(message),
    }
}
fn candidate_unsigned(message: &str) -> Result<()> {
    if message.contains("-----BEGIN SIGNED MESSAGE-----")
        || message.contains("-----END SIGNED MESSAGE-----")
    {
        return Err("unsupported X.509 tag signature".into());
    }
    if ARMOR
        .iter()
        .any(|(a, b)| message.contains(a) || message.contains(b))
    {
        return Err("candidate tags must not contain signature armor".into());
    }
    Ok(())
}
fn fake_entry(entry: &TagEntry) -> Result<Entry> {
    if entry
        .credit_changes
        .iter()
        .any(|c| !c.source_blocks.is_empty())
    {
        return Err("tags require contiguous exact prose credit replacements".into());
    }
    let mut mains = Vec::new();
    for change in &entry.main_credit_changes {
        if change.role != "tagger" {
            return Err("tag main credit role must be tagger".into());
        }
        mains.push(MainCreditChange {
            role: "author".into(),
            old_identity: change.old_identity.clone(),
            new: change.new.clone(),
        });
    }
    Ok(Entry {
        source_oid: entry.source_oid.clone(),
        source_sha256: entry.source_sha256.clone(),
        expected_oid: entry.expected_oid.clone(),
        candidate_file: entry.candidate_file.clone(),
        candidate_sha256: entry.candidate_sha256.clone(),
        ownership: None,
        committer_ownership: None,
        main_credit_changes: mains,
        credit_changes: entry
            .credit_changes
            .iter()
            .map(|c| CreditChange {
                old: c.old.clone(),
                source_blocks: c.source_blocks.clone(),
                new: c.new.clone(),
                owned: c.owned,
            })
            .collect(),
        remove_headers: Vec::new(),
        gitlinks: Vec::new(),
        source_provenance: None,
    })
}
fn transformed_prose(entry: &TagEntry, source: &str, candidate: &str, tagger: &str) -> Result<()> {
    // Locate every approved replacement in original bytes before changing any
    // text. Overlapping, inline and invented old blocks have no authority.
    let mut edits: Vec<(usize, usize, &str)> = Vec::new();
    for change in &entry.credit_changes {
        if change.old.is_empty()
            || change.old.ends_with('\n')
            || change.new.is_empty()
            || change.new.ends_with('\n')
        {
            return Err("tag credit changes require exact full credit blocks".into());
        }
        let occurrence = source
            .match_indices(&change.old)
            .find_map(|(start, _)| {
                let end = start + change.old.len();
                let full = (start == 0 || source.as_bytes()[start - 1] == b'\n')
                    && (end == source.len() || source.as_bytes()[end] == b'\n');
                (full && edits.iter().all(|(a, b, _)| end <= *a || start >= *b))
                    .then_some((start, end))
            })
            .ok_or("tag credit replacement must consume original full lines")?;
        edits.push((occurrence.0, occurrence.1, change.new.as_str()));
    }
    edits.sort_by_key(|e| e.0);
    let mut expected = String::new();
    let mut cursor = 0;
    for (start, end, new) in edits {
        expected.push_str(&source[cursor..start]);
        expected.push_str(new);
        cursor = end;
    }
    expected.push_str(&source[cursor..]);
    if candidate == expected {
        return Ok(());
    }
    let suffix = candidate
        .strip_prefix(&expected)
        .ok_or("tag original prose may change only through exact credit mappings")?;
    if !suffix.starts_with('\n') && !expected.ends_with('\n') {
        return Err("tag appended provenance must start on a new line".into());
    }
    let old_value = tagger.rsplit_once("> ").map(|(name, _)| format!("{name}>"));
    if entry.main_credit_changes.is_empty() && !old_value.is_some_and(|v| policy::recognized_ai(&v))
    {
        return Err("tag prose additions require actual main AI provenance".into());
    }
    let mut found = false;
    for line in suffix.lines().filter(|line| !line.is_empty()) {
        let (key, value) = line
            .split_once(':')
            .ok_or("tag appended prose is not a credit")?;
        let key = key.to_ascii_lowercase();
        if key == "co-authored-by" && policy::recognized_ai(value.trim()) {
            found = true;
        } else if key == "ai-credit" {
            found = true;
        } else {
            return Err(
                "tag additions permit only equivalent co-author or compact AI credit".into(),
            );
        }
    }
    if !found {
        return Err("tag trailing prose/newlines are not approved credit changes".into());
    }
    Ok(())
}
fn validate_tags(
    git: &Git,
    manifest: &TagManifest,
    sources: &[Vec<u8>],
    candidates: &[Vec<u8>],
    identity: &Identity,
    receipt: &BTreeMap<String, String>,
) -> Result<()> {
    let mapping: BTreeMap<&str, &str> = manifest
        .entries
        .iter()
        .map(|e| (e.source_oid.as_str(), e.expected_oid.as_str()))
        .collect();
    let mut old_commits = BTreeSet::new();
    let mut old_tags = BTreeSet::new();
    for ((entry, source), candidate) in manifest.entries.iter().zip(sources).zip(candidates) {
        let old = tag(source, git.width)?;
        let new = tag(candidate, git.width)?;
        if old.kind != new.kind
            || old.name != new.name
            || validate::date(old.tagger)? != validate::date(new.tagger)?
        {
            return Err("tag name/type/date must remain exact".into());
        }
        core::check_ident(new.tagger, identity, "candidate tagger")?;
        if let Some(claim) = &entry.tagger_ownership {
            if !claim.owned || claim.old_tagger != old.tagger {
                return Err(
                    "tagger ownership must freeze exact actual old tagger and owned=true".into(),
                );
            }
        } else if core::check_ident(old.tagger, identity, "source tagger").is_err() {
            return Err("noncanonical tagger requires explicit tag ownership".into());
        }
        let target = if old.kind == "commit" {
            old_commits.insert(old.target.to_string());
            receipt
                .get(old.target)
                .map(String::as_str)
                .unwrap_or(old.target)
        } else {
            old_tags.insert(old.target.to_string());
            mapping.get(old.target).copied().unwrap_or(old.target)
        };
        if new.target != target {
            return Err("tag retarget must be unchanged or use a verified exact mapping".into());
        }
        let body = unsigned(old.message, entry.remove_signature)?;
        candidate_unsigned(new.message)?;
        let fake = fake_entry(entry)?;
        validate::tag_attribution(&fake, old.tagger, body, new.message, identity)?;
        transformed_prose(entry, body, new.message, old.tagger)?;
    }
    // Traverse external nested tags by levels, never by one process per object.
    let mut visited = BTreeSet::new();
    for _ in 0..128 {
        if old_tags.is_empty() {
            break;
        }
        let level: Vec<String> = std::mem::take(&mut old_tags).into_iter().collect();
        let objects = git.batch(&level, "tag")?;
        for (oid, raw) in level.iter().zip(&objects) {
            if !visited.insert(oid.clone()) {
                continue;
            }
            let nested = tag(raw, git.width)?;
            if nested.kind == "commit" {
                old_commits.insert(nested.target.into());
            } else {
                old_tags.insert(nested.target.into());
            }
        }
        if visited.len() > MAX_ENTRIES {
            return Err("nested tag scope exceeds safe limit".into());
        }
    }
    if !old_tags.is_empty() {
        return Err("nested tag depth exceeds safe limit".into());
    }
    git.batch(&old_commits.into_iter().collect::<Vec<_>>(), "commit")?;
    Ok(())
}
fn private_receipt(path: &Path) -> Result<Vec<u8>> {
    let bytes = state::read_input(path, MAX_MANIFEST)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        for (target, is_dir) in [
            (path, false),
            (path.parent().ok_or("receipt parent unavailable")?, true),
            (
                path.parent()
                    .and_then(Path::parent)
                    .ok_or("receipt root unavailable")?,
                true,
            ),
        ] {
            let m = fs::symlink_metadata(target).map_err(|_| "receipt metadata unavailable")?;
            if m.uid() != unsafe { libc::geteuid() }
                || m.mode() & 0o777 != if is_dir { 0o700 } else { 0o600 }
                || (!is_dir && (!m.is_file() || m.nlink() != 1))
                || (is_dir && !m.is_dir())
            {
                return Err("commit receipt must be private owned evidence".into());
            }
        }
    }
    #[cfg(not(unix))]
    return Err("tag writer requires private Unix receipt storage".into());
    Ok(bytes)
}
fn receipt(
    git: &Git,
    path: Option<&Path>,
    identity: &Identity,
) -> Result<(BTreeMap<String, String>, Vec<u8>, Vec<u8>)> {
    let Some(path) = path else {
        return Ok((BTreeMap::new(), Vec::new(), Vec::new()));
    };
    let bytes = private_receipt(path)?;
    let report: CommitReceipt =
        serde_json::from_slice(&bytes).map_err(|_| "invalid complete commit receipt")?;
    if report.digest.len() != 64
        || !report
            .digest
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        || report.phase != "complete"
        || report.common_dir != git.common
        || path
            != git
                .common
                .join("commitguard-bulk")
                .join(&report.digest)
                .join("complete.json")
    {
        return Err(
            "commit receipt must come from this repository's completed native operation".into(),
        );
    }
    let dir = path.parent().ok_or("receipt directory missing")?;
    let approval: Approval = serde_json::from_slice(&private_receipt(&dir.join("approval.json"))?)
        .map_err(|_| "invalid retained commit approval")?;
    if serde_json::to_vec(&approval.identity).ok() != serde_json::to_vec(identity).ok()
        || approval.context.len() != 64
        || !approval.context.bytes().all(|b| b.is_ascii_hexdigit())
    {
        return Err("commit receipt authenticated identity or context mismatch".into());
    }
    let raw_manifest = private_receipt(&dir.join("manifest.json"))?;
    let manifest: Manifest =
        serde_json::from_slice(&raw_manifest).map_err(|_| "invalid receipt's retained manifest")?;
    let origin_profile = super::profile(&manifest)?;
    if manifest.common_dir != git.common
        || manifest.entries.is_empty()
        || manifest.entries.len() > MAX_ENTRIES
        || report.mapping.len() != manifest.entries.len()
    {
        return Err("commit receipt manifest scope mismatch".into());
    }
    let source_queries: Vec<String> = manifest
        .entries
        .iter()
        .map(|e| e.source_oid.clone())
        .chain(manifest.boundaries.iter().cloned())
        .collect();
    let sources = git.batch(&source_queries, "commit")?;
    let expected: Vec<String> = manifest
        .entries
        .iter()
        .map(|e| e.expected_oid.clone())
        .collect();
    let candidates = git.batch(&expected, "commit")?;
    let mut hash = Sha256::new();
    feed(&mut hash, super::domain(origin_profile));
    feed(&mut hash, &raw_manifest);
    feed(
        &mut hash,
        &serde_json::to_vec(&approval.identity)
            .map_err(|_| "receipt identity serialization failed")?,
    );
    feed(&mut hash, approval.context.as_bytes());
    for (i, source) in sources.iter().enumerate() {
        if private_receipt(&dir.join(format!("source-{i:06}.commit")))? != *source {
            return Err("commit receipt source snapshot mismatch".into());
        }
        feed(&mut hash, source);
    }
    for candidate in &candidates {
        feed(&mut hash, candidate);
    }
    if format!("{:x}", hash.finalize()) != report.digest {
        return Err("commit receipt digest does not bind retained approval and inputs".into());
    }
    for boundary in &sources[manifest.entries.len()..] {
        validate::parse_profile(boundary, false, git.width, origin_profile)?;
    }
    let mut mapping = BTreeMap::new();
    let mut new_ids = BTreeSet::new();
    for (i, ((entry, item), candidate)) in manifest
        .entries
        .iter()
        .zip(&report.mapping)
        .zip(&candidates)
        .enumerate()
    {
        if item.source_oid != entry.source_oid
            || item.new_oid != entry.expected_oid
            || mapping
                .insert(item.source_oid.clone(), item.new_oid.clone())
                .is_some()
            || !new_ids.insert(item.new_oid.clone())
        {
            return Err("commit receipt mapping is inconsistent or duplicated".into());
        }
        check_hash(&sources[i], &entry.source_sha256)?;
        check_hash(candidate, &entry.candidate_sha256)?;
        if private_receipt(&dir.join(format!("source-{i:06}.commit")))? != sources[i]
            || private_receipt(&dir.join(format!("candidate-{i:06}.commit")))? != *candidate
        {
            return Err("commit receipt actual objects disagree with private snapshots".into());
        }
        core::validate_commit_bytes(&entry.expected_oid, candidate, identity)?;
    }
    validate::all(
        git,
        &manifest,
        &sources[..manifest.entries.len()],
        &candidates,
        identity,
    )?;
    Ok((mapping, bytes, raw_manifest))
}

pub(super) fn run(args: &[String], config: Option<&Config>) -> Result<()> {
    let mut file = None;
    let mut confirm = None;
    let mut index = 0;
    while index < args.len() {
        let value = args
            .get(index + 1)
            .ok_or("bulk tag option requires a value")?;
        match args[index].as_str() {
            "--manifest" if file.is_none() => file = Some(PathBuf::from(value)),
            "--confirm" if confirm.is_none() => confirm = Some(value.clone()),
            _ => return Err("usage: bulk-write-tags --manifest FILE [--confirm DIGEST]".into()),
        };
        index += 2;
    }
    let raw = state::read_input(
        &file.ok_or("bulk-write-tags requires --manifest")?,
        MAX_MANIFEST,
    )?;
    let manifest: TagManifest =
        serde_json::from_slice(&raw).map_err(|_| "invalid bulk tag manifest")?;
    if manifest.schema_version != 1
        || manifest.policy_version != 1
        || manifest.entries.is_empty()
        || manifest.entries.len() > MAX_ENTRIES
    {
        return Err("unsupported bulk tag schema or entry count".into());
    }
    let tools = core::tools(config)?;
    let (identity, context) = auth::account_with_context(&tools)?;
    let git = Git::discover(&tools.git)?;
    if !manifest.common_dir.is_absolute() || manifest.common_dir != git.common {
        return Err("tag common directory must match the canonical repository".into());
    }
    let mut old = BTreeSet::new();
    let mut new = BTreeSet::new();
    for e in &manifest.entries {
        git.oid(&e.source_oid)?;
        git.oid(&e.expected_oid)?;
        if !old.insert(e.source_oid.clone())
            || !new.insert(e.expected_oid.clone())
            || !e.candidate_file.is_absolute()
        {
            return Err(
                "tag source/expected OIDs must be unique and candidate paths absolute".into(),
            );
        }
    }
    let source_queries: Vec<String> = manifest
        .entries
        .iter()
        .map(|e| e.source_oid.clone())
        .collect();
    let sources = git.batch(&source_queries, "tag")?;
    let mut total = raw.len();
    let mut candidates = Vec::new();
    for (e, s) in manifest.entries.iter().zip(&sources) {
        if s.len() > MAX_COMMIT {
            return Err("tag source exceeds safe limit".into());
        }
        check_hash(s, &e.source_sha256)?;
        let c = state::read_input(&e.candidate_file, MAX_COMMIT)?;
        check_hash(&c, &e.candidate_sha256)?;
        total = total
            .checked_add(s.len())
            .and_then(|n| n.checked_add(c.len()))
            .filter(|n| *n <= MAX_BYTES)
            .ok_or("tag aggregate exceeds safe limit")?;
        candidates.push(c);
    }
    let (commit_mapping, raw_receipt, parent_manifest) =
        receipt(&git, manifest.commit_receipt.as_deref(), &identity)?;
    total
        .checked_add(raw_receipt.len())
        .and_then(|n| n.checked_add(parent_manifest.len()))
        .filter(|n| *n <= MAX_BYTES)
        .ok_or("tag receipt aggregate exceeds safe limit")?;
    validate_tags(
        &git,
        &manifest,
        &sources,
        &candidates,
        &identity,
        &commit_mapping,
    )?;
    let mut hash = Sha256::new();
    feed(&mut hash, b"commitguard-bulk-tags-v1");
    feed(&mut hash, &raw);
    feed(&mut hash, &raw_receipt);
    feed(&mut hash, &parent_manifest);
    feed(
        &mut hash,
        &serde_json::to_vec(&identity).map_err(|_| "tag identity serialization failed")?,
    );
    feed(&mut hash, context.as_bytes());
    for s in &sources {
        feed(&mut hash, s);
    }
    for c in &candidates {
        feed(&mut hash, c);
    }
    let digest = format!("{:x}", hash.finalize());
    if confirm.as_ref().is_some_and(|c| c != &digest) {
        return Err("tag confirmation digest changed; preview again".into());
    }
    let operation = state::Operation::open(&git.common, &digest)?;
    operation.snapshot("manifest.json", &raw)?;
    operation.snapshot("commit-receipt.json", &raw_receipt)?;
    operation.snapshot("commit-manifest.json", &parent_manifest)?;
    let mut source_names = Vec::new();
    let mut candidate_names = Vec::new();
    for (i, bytes) in sources.iter().enumerate() {
        let name = format!("source-{i:06}.tag");
        operation.snapshot(&name, bytes)?;
        source_names.push(name);
    }
    for (i, bytes) in candidates.iter().enumerate() {
        let name = format!("candidate-{i:06}.tag");
        operation.snapshot(&name, bytes)?;
        candidate_names.push(name);
    }
    let report = |phase| Report {
        digest: &digest,
        phase,
        common_dir: &git.common,
        mapping: manifest
            .entries
            .iter()
            .map(|e| Mapping {
                source_oid: &e.source_oid,
                new_oid: &e.expected_oid,
            })
            .collect(),
    };
    let dry: Vec<String> = source_names
        .iter()
        .chain(&candidate_names)
        .cloned()
        .collect();
    let expected: Vec<String> = source_queries
        .iter()
        .chain(manifest.entries.iter().map(|e| &e.expected_oid))
        .cloned()
        .collect();
    git.hash_kind("tag", &operation.path, &dry, &expected, false)?;
    if auth::context_fingerprint(&tools)? != context {
        return Err("tag credential context changed".into());
    }
    operation.record("validated.json", &report("preview"))?;
    let phase = if confirm.is_some() {
        operation.record("intent.json", &report("writing"))?;
        let wanted: Vec<String> = manifest
            .entries
            .iter()
            .map(|e| e.expected_oid.clone())
            .collect();
        git.hash_kind("tag", &operation.path, &candidate_names, &wanted, true)?;
        let actual = git.batch(&wanted, "tag")?;
        if actual != candidates {
            return Err("tag written object readback differs from snapshot".into());
        }
        validate_tags(
            &git,
            &manifest,
            &sources,
            &actual,
            &identity,
            &commit_mapping,
        )?;
        if auth::context_fingerprint(&tools)? != context {
            return Err("tag credential changed after write; mapping retained".into());
        }
        operation.record("complete.json", &report("complete"))?;
        "complete"
    } else {
        "preview"
    };
    println!(
        "{}",
        serde_json::to_string(&report(phase)).map_err(|_| "tag report serialization failed")?
    );
    Ok(())
}
