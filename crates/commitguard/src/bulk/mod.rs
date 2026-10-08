//! Scoped object-only writer. Ref promotion and publication belong to the caller.
mod git;
mod state;
mod tags;
mod validate;

use crate::{Config, Result, auth, core};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{collections::BTreeSet, path::PathBuf};

const MAX_BYTES: usize = 64 * 1024 * 1024;
const MAX_COMMIT: usize = 4 * 1024 * 1024;
const MAX_MANIFEST: usize = 32 * 1024 * 1024;
const MAX_ENTRIES: usize = 100_000;

pub fn run_tags(args: &[String], config: Option<&Config>) -> Result<()> {
    tags::run(args, config)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    schema_version: u32,
    policy_version: u32,
    common_dir: PathBuf,
    #[serde(default, deserialize_with = "present")]
    provenance_profile: Option<String>,
    boundaries: Vec<String>,
    entries: Vec<Entry>,
}
/// Absent is `None`; an explicit JSON null is an error, never an implicit default.
fn present<'de, D, T>(deserializer: D) -> std::result::Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    T::deserialize(deserializer).map(Some)
}
#[derive(Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
enum SourceProvenance {
    Add,
    Preserve,
}
const V1_DOMAIN: &[u8] = b"commitguard-bulk-v1";
const V2_DOMAIN: &[u8] = b"commitguard-bulk-v2-source-sha256-v1";
const ORIGIN_PROFILE: &str = "source-sha256-v1";
/// Central schema/policy/profile gate. Returns whether the origin profile is on.
fn profile(manifest: &Manifest) -> Result<bool> {
    if manifest.schema_version != 1 {
        return Err("unsupported bulk schema or policy version".into());
    }
    match manifest.policy_version {
        1 => {
            if manifest.provenance_profile.is_some()
                || manifest
                    .entries
                    .iter()
                    .any(|e| e.source_provenance.is_some())
            {
                return Err("bulk provenance declarations require policy version 2".into());
            }
            Ok(false)
        }
        2 if manifest.provenance_profile.as_deref() == Some(ORIGIN_PROFILE) => Ok(true),
        _ => Err("unsupported bulk schema, policy or provenance profile".into()),
    }
}
fn domain(origin_profile: bool) -> &'static [u8] {
    if origin_profile { V2_DOMAIN } else { V1_DOMAIN }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Entry {
    source_oid: String,
    source_sha256: String,
    expected_oid: String,
    candidate_file: PathBuf,
    candidate_sha256: String,
    ownership: Option<Ownership>,
    committer_ownership: Option<CommitterOwnership>,
    #[serde(default)]
    main_credit_changes: Vec<MainCreditChange>,
    #[serde(default)]
    credit_changes: Vec<CreditChange>,
    #[serde(default)]
    remove_headers: Vec<String>,
    #[serde(default)]
    gitlinks: Vec<Gitlink>,
    #[serde(default, deserialize_with = "present")]
    source_provenance: Option<SourceProvenance>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Ownership {
    old_author: String,
    owned: bool,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CommitterOwnership {
    old_committer: String,
    owned: bool,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct MainCreditChange {
    role: String,
    old_identity: String,
    new: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CreditChange {
    #[serde(default)]
    old: String,
    #[serde(default)]
    source_blocks: Vec<String>,
    new: String,
    #[serde(default)]
    owned: bool,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Gitlink {
    path_hex: String,
    old_oid: String,
    new_oid: String,
}
#[derive(Serialize)]
struct Mapping<'a> {
    source_oid: &'a str,
    new_oid: &'a str,
}
#[derive(Serialize)]
struct Report<'a> {
    digest: &'a str,
    phase: &'a str,
    common_dir: &'a std::path::Path,
    mapping: Vec<Mapping<'a>>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Approval {
    identity: crate::Identity,
    context: String,
}
fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn feed(hash: &mut Sha256, bytes: &[u8]) {
    hash.update((bytes.len() as u64).to_be_bytes());
    hash.update(bytes);
}
fn check_hash(bytes: &[u8], expected: &str) -> Result<()> {
    if expected.len() != 64 || sha256(bytes) != expected {
        return Err("bulk byte SHA-256 mismatch".into());
    }
    Ok(())
}

pub fn run(args: &[String], config: Option<&Config>) -> Result<()> {
    let mut manifest_file = None;
    let mut confirm = None;
    let mut index = 0;
    while index < args.len() {
        let value = args
            .get(index + 1)
            .ok_or("bulk-write option requires a value")?;
        match args[index].as_str() {
            "--manifest" if manifest_file.is_none() => manifest_file = Some(PathBuf::from(value)),
            "--confirm" if confirm.is_none() => confirm = Some(value.clone()),
            _ => {
                return Err(
                    "usage: commitguard [--strict] bulk-write --manifest FILE [--confirm DIGEST]"
                        .into(),
                );
            }
        }
        index += 2;
    }
    let manifest_file = manifest_file.ok_or("bulk-write requires --manifest FILE")?;
    if let Some(digest) = &confirm
        && (digest.len() != 64
            || !digest
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)))
    {
        return Err("invalid bulk confirmation digest".into());
    }
    let raw_manifest = state::read_input(&manifest_file, MAX_MANIFEST)?;
    let manifest: Manifest = serde_json::from_slice(&raw_manifest)
        .map_err(|_| "invalid bulk manifest (unknown, duplicate or malformed fields)")?;
    let origin_profile = profile(&manifest)?;
    if manifest.entries.is_empty() || manifest.entries.len() > MAX_ENTRIES {
        return Err("bulk entry count outside safe bounds".into());
    }
    let tools = core::tools(config)?;
    let (identity, context) = auth::account_with_context(&tools)?;
    let git = git::Git::discover(&tools.git)?;
    if !manifest.common_dir.is_absolute() || manifest.common_dir != git.common {
        return Err(
            "bulk common_dir must match the canonical repository common Git directory".into(),
        );
    }
    let mut sources = BTreeSet::new();
    let mut expected = BTreeSet::new();
    for entry in &manifest.entries {
        git.oid(&entry.source_oid)?;
        git.oid(&entry.expected_oid)?;
        if !sources.insert(entry.source_oid.clone()) || !expected.insert(entry.expected_oid.clone())
        {
            return Err("duplicate bulk source or expected OID".into());
        }
        if !entry.candidate_file.is_absolute() {
            return Err("bulk candidate path must be absolute".into());
        }
    }
    if manifest.boundaries.len() > MAX_ENTRIES {
        return Err("bulk boundary count outside safe bounds".into());
    }
    let mut boundaries = BTreeSet::new();
    for boundary in &manifest.boundaries {
        git.oid(boundary)?;
        if sources.contains(boundary) || !boundaries.insert(boundary.clone()) {
            return Err("duplicate or in-scope bulk boundary".into());
        }
    }
    let queries: Vec<String> = manifest
        .entries
        .iter()
        .map(|e| e.source_oid.clone())
        .chain(manifest.boundaries.iter().cloned())
        .collect();
    let objects = git.batch(&queries, "commit")?;
    let mut total = objects
        .iter()
        .try_fold(raw_manifest.len(), |total, bytes| {
            if bytes.len() > MAX_COMMIT {
                return Err("bulk source or boundary commit exceeds safe limit");
            }
            total
                .checked_add(bytes.len())
                .filter(|n| *n <= MAX_BYTES)
                .ok_or("bulk aggregate exceeds safe limit")
        })?;
    let mut candidates = Vec::with_capacity(manifest.entries.len());
    for (entry, source) in manifest.entries.iter().zip(&objects) {
        if source.len() > MAX_COMMIT {
            return Err("bulk source commit exceeds safe limit".into());
        }
        check_hash(source, &entry.source_sha256)?;
        let candidate = state::read_input(&entry.candidate_file, MAX_COMMIT)?;
        check_hash(&candidate, &entry.candidate_sha256)?;
        total = total
            .checked_add(candidate.len())
            .ok_or("bulk aggregate overflow")?;
        if total > MAX_BYTES {
            return Err("bulk aggregate exceeds safe limit".into());
        }
        candidates.push(candidate);
    }
    // Boundary objects are real commits, not merely syntactically valid OIDs.
    for boundary in &objects[manifest.entries.len()..] {
        validate::parse_profile(boundary, false, git.width, origin_profile)?;
    }
    validate::all(
        &git,
        &manifest,
        &objects[..manifest.entries.len()],
        &candidates,
        &identity,
    )?;
    let mut hash = Sha256::new();
    feed(&mut hash, domain(origin_profile));
    feed(&mut hash, &raw_manifest);
    feed(
        &mut hash,
        &serde_json::to_vec(&identity).map_err(|_| "identity serialization failed")?,
    );
    feed(&mut hash, context.as_bytes());
    for source in &objects {
        feed(&mut hash, source);
    }
    for candidate in &candidates {
        feed(&mut hash, candidate);
    }
    let digest = format!("{:x}", hash.finalize());
    if confirm.as_ref().is_some_and(|value| value != &digest) {
        return Err("bulk confirmation digest changed; preview again".into());
    }
    let operation = state::Operation::open(&git.common, &digest)?;
    operation.snapshot("manifest.json", &raw_manifest)?;
    operation.snapshot(
        "approval.json",
        &serde_json::to_vec(&Approval {
            identity: identity.clone(),
            context: context.clone(),
        })
        .map_err(|_| "bulk approval serialization failed")?,
    )?;
    let mut source_names = Vec::with_capacity(objects.len());
    for (index, bytes) in objects.iter().enumerate() {
        let name = format!("source-{index:06}.commit");
        operation.snapshot(&name, bytes)?;
        source_names.push(name);
    }
    let mut candidate_names = Vec::with_capacity(candidates.len());
    for (index, bytes) in candidates.iter().enumerate() {
        let name = format!("candidate-{index:06}.commit");
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
    let dry_names: Vec<String> = source_names
        .iter()
        .chain(&candidate_names)
        .cloned()
        .collect();
    let dry_expected: Vec<String> = queries
        .iter()
        .chain(manifest.entries.iter().map(|e| &e.expected_oid))
        .cloned()
        .collect();
    git.hash(&operation.path, &dry_names, &dry_expected, false)?;
    if auth::context_fingerprint(&tools)? != context {
        return Err("bulk credential context changed".into());
    }
    operation.record("validated.json", &report("preview"))?;
    let phase = if confirm.is_some() {
        // Durable intent precedes the only object-writing command. Repeating it
        // writes the same immutable objects, never resets refs or staging state.
        operation.record("intent.json", &report("writing"))?;
        let wanted: Vec<String> = manifest
            .entries
            .iter()
            .map(|e| e.expected_oid.clone())
            .collect();
        git.hash(&operation.path, &candidate_names, &wanted, true)?;
        let actual = git.batch(&wanted, "commit")?;
        if actual != candidates {
            return Err("bulk written object readback differs from snapshot".into());
        }
        validate::all(
            &git,
            &manifest,
            &objects[..manifest.entries.len()],
            &actual,
            &identity,
        )?;
        if auth::context_fingerprint(&tools)? != context {
            return Err(
                "bulk credential context changed after object write; mapping retained".into(),
            );
        }
        operation.record("complete.json", &report("complete"))?;
        "complete"
    } else {
        "preview"
    };
    println!(
        "{}",
        serde_json::to_string(&report(phase)).map_err(|_| "bulk report serialization failed")?
    );
    Ok(())
}
