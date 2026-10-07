//! Bounded, private and durable repository-local repair state.
use super::Journal;
use crate::Result;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
};

const LIMIT: u64 = 4 * 1024 * 1024;

/// Refuse a repair before acquiring its lock if the largest successful journal
/// cannot fit. Replacement objects have exactly these unsigned headers; their
/// only unknown fields are fixed-width OIDs. Reserve separately for a failure
/// diagnostic, phase and the eventual worktree Git directory.
pub(super) fn journal_capacity(journal: &Journal) -> Result<()> {
    let mut future = journal.clone();
    let oid = "f".repeat(journal.original_tip.len());
    future.mapping.clear();
    future.verified.clear();
    for (source, candidate) in journal.receipt.sources.iter().zip(&journal.candidates) {
        let author = format!(
            "{} <{}> {}",
            journal.receipt.identity.login,
            journal.receipt.identity.email,
            super::replay::author_date(&source.author)?
        );
        let committer = format!(
            "{} <{}> {}",
            journal.receipt.identity.login,
            journal.receipt.identity.email,
            journal.committer_date.trim_start_matches('@')
        );
        future.mapping.push(super::Mapping {
            source_oid: source.source_oid.clone(),
            new_oid: oid.clone(),
        });
        future.verified.push(super::Source {
            source_oid: oid.clone(),
            parent: oid.clone(),
            tree: source.tree.clone(),
            author: author.clone(),
            committer: committer.clone(),
            message: candidate.message.clone(),
            raw: format!(
                "tree {}\nparent {oid}\nauthor {author}\ncommitter {committer}\n\n{}",
                source.tree, candidate.message
            ),
        });
    }
    // Include the checksum envelope, then keep 128 KiB for JSON-escaped
    // diagnostics (bounded to 16 KiB), phase, directory and scalar growth.
    let checked = CheckedJournal {
        journal: future,
        checksum: "f".repeat(64),
    };
    ensure_capacity(&checked, 128 * 1024)
}
fn ensure_capacity<T: Serialize>(value: &T, reserve: usize) -> Result<()> {
    if canonical(value)?
        .len()
        .checked_add(reserve)
        .is_none_or(|size| size > LIMIT as usize)
    {
        return Err("repair journal would exceed 4 MiB; choose a smaller unpublished suffix before applying".into());
    }
    Ok(())
}

/// Canonical JSON: recursively sorted object keys, compact encoding, no newline.
fn canonical<T: Serialize>(value: &T) -> Result<Vec<u8>> {
    fn sorted(value: serde_json::Value) -> serde_json::Value {
        match value {
            serde_json::Value::Object(map) => {
                let entries: std::collections::BTreeMap<_, _> = map
                    .into_iter()
                    .map(|(key, value)| (key, sorted(value)))
                    .collect();
                serde_json::Value::Object(entries.into_iter().collect())
            }
            serde_json::Value::Array(values) => {
                serde_json::Value::Array(values.into_iter().map(sorted).collect())
            }
            other => other,
        }
    }
    serde_json::to_vec(&sorted(
        serde_json::to_value(value).map_err(|e| e.to_string())?,
    ))
    .map_err(|e| e.to_string())
}
pub(super) fn hash<T: Serialize>(value: &T) -> Result<String> {
    Ok(format!("{:x}", Sha256::digest(canonical(value)?)))
}

#[cfg(unix)]
fn parents(path: &Path, private: bool) -> Result<()> {
    use std::os::unix::fs::MetadataExt;
    if !path.is_absolute()
        || path
            .components()
            .any(|c| matches!(c, std::path::Component::ParentDir))
    {
        return Err("repair storage requires an absolute normalized path".into());
    }
    let parent = path.parent().ok_or("repair storage has no parent")?;
    let mut current = PathBuf::new();
    for component in parent.components() {
        current.push(component);
        let meta =
            fs::symlink_metadata(&current).map_err(|_| "cannot inspect repair storage parent")?;
        if !meta.is_dir() || meta.file_type().is_symlink() {
            return Err("repair storage parent is not a real directory".into());
        }
        // A root-owned sticky temporary directory is an acceptable ancestor,
        // never an acceptable private storage directory itself.
        if meta.mode() & 0o022 != 0 && !(meta.uid() == 0 && meta.mode() & 0o1000 != 0) {
            return Err("repair storage parent is writable by another user".into());
        }
        if private
            && current == parent
            && (meta.mode() & 0o777 != 0o700 || meta.uid() != unsafe { libc::geteuid() })
        {
            return Err("repair storage directory must be owned and private (0700)".into());
        }
    }
    Ok(())
}
#[cfg(not(unix))]
fn parents(_: &Path, _: bool) -> Result<()> {
    Err("repair storage requires Unix".into())
}

pub(super) fn root(common: &Path) -> Result<PathBuf> {
    let path = common.join("commitguard-fix");
    parents(&path, false)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        match fs::DirBuilder::new().mode(0o700).create(&path) {
            Ok(()) => sync_dir(common)?,
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(_) => return Err("cannot create repair storage directory".into()),
        }
    }
    parents(&path.join("probe"), true)?;
    Ok(path)
}
pub(super) fn operation_root(common: &Path, id: &str) -> Result<PathBuf> {
    if id.len() != 64
        || !id
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err("invalid repair operation ID".into());
    }
    let base = root(common)?;
    let path = base.join(format!("operation-{id}"));
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        match fs::DirBuilder::new().mode(0o700).create(&path) {
            Ok(()) => sync_dir(&base)?,
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(_) => return Err("cannot create private operation directory".into()),
        }
    }
    parents(&path.join("probe"), true)?;
    Ok(path)
}
fn sync_dir(path: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC)
            .open(path)
            .and_then(|file| file.sync_all())
            .map_err(|_| "cannot sync repair directory".into())
    }
    #[cfg(not(unix))]
    {
        let _ = path;
        Err("repair storage requires Unix".into())
    }
}
#[cfg(unix)]
fn options() -> OpenOptions {
    use std::os::unix::fs::OpenOptionsExt;
    let mut options = OpenOptions::new();
    options
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC | libc::O_NONBLOCK);
    options
}
#[cfg(not(unix))]
fn options() -> OpenOptions {
    OpenOptions::new()
}

// serde_json::Value normally accepts duplicate keys. Reject them recursively
// before deserializing typed receipts, proposals and flattened journals.
struct UniqueValue(serde_json::Value);
impl<'de> Deserialize<'de> for UniqueValue {
    fn deserialize<D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> std::result::Result<Self, D::Error> {
        struct Visitor;
        impl<'de> serde::de::Visitor<'de> for Visitor {
            type Value = UniqueValue;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("JSON without duplicate keys")
            }
            fn visit_bool<E: serde::de::Error>(
                self,
                v: bool,
            ) -> std::result::Result<Self::Value, E> {
                Ok(UniqueValue(v.into()))
            }
            fn visit_i64<E: serde::de::Error>(self, v: i64) -> std::result::Result<Self::Value, E> {
                Ok(UniqueValue(v.into()))
            }
            fn visit_u64<E: serde::de::Error>(self, v: u64) -> std::result::Result<Self::Value, E> {
                Ok(UniqueValue(v.into()))
            }
            fn visit_f64<E: serde::de::Error>(self, v: f64) -> std::result::Result<Self::Value, E> {
                serde_json::Number::from_f64(v)
                    .map(|v| UniqueValue(v.into()))
                    .ok_or_else(|| E::custom("invalid JSON number"))
            }
            fn visit_str<E: serde::de::Error>(
                self,
                v: &str,
            ) -> std::result::Result<Self::Value, E> {
                Ok(UniqueValue(v.into()))
            }
            fn visit_string<E: serde::de::Error>(
                self,
                v: String,
            ) -> std::result::Result<Self::Value, E> {
                Ok(UniqueValue(v.into()))
            }
            fn visit_unit<E: serde::de::Error>(self) -> std::result::Result<Self::Value, E> {
                Ok(UniqueValue(serde_json::Value::Null))
            }
            fn visit_seq<A: serde::de::SeqAccess<'de>>(
                self,
                mut seq: A,
            ) -> std::result::Result<Self::Value, A::Error> {
                let mut values = Vec::new();
                while let Some(value) = seq.next_element::<UniqueValue>()? {
                    values.push(value.0);
                }
                Ok(UniqueValue(serde_json::Value::Array(values)))
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                mut map: A,
            ) -> std::result::Result<Self::Value, A::Error> {
                let mut values = serde_json::Map::new();
                while let Some(key) = map.next_key::<String>()? {
                    if values.contains_key(&key) {
                        return Err(serde::de::Error::custom(format!(
                            "duplicate JSON field: {key}"
                        )));
                    }
                    values.insert(key, map.next_value::<UniqueValue>()?.0);
                }
                Ok(UniqueValue(serde_json::Value::Object(values)))
            }
        }
        deserializer.deserialize_any(Visitor)
    }
}

pub(super) fn read<T: DeserializeOwned>(path: &Path, private: bool) -> Result<T> {
    parents(path, private)?;
    let mut file = options()
        .read(true)
        .open(path)
        .map_err(|_| "cannot open repair input without following symlinks")?;
    let meta = file.metadata().map_err(|_| "cannot inspect repair input")?;
    if !meta.is_file() || meta.len() > LIMIT {
        return Err("repair input must be a regular file at most 4 MiB".into());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if private
            && (meta.mode() & 0o777 != 0o600
                || meta.uid() != unsafe { libc::geteuid() }
                || meta.nlink() != 1)
        {
            return Err("native repair input must be owned, unlinked and private (0600)".into());
        }
    }
    let mut bytes = Vec::new();
    (&mut file)
        .take(LIMIT + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "cannot read repair input")?;
    if bytes.len() as u64 > LIMIT {
        return Err("repair input exceeds 4 MiB".into());
    }
    let value: UniqueValue =
        serde_json::from_slice(&bytes).map_err(|e| format!("invalid repair JSON: {e}"))?;
    serde_json::from_value(value.0).map_err(|e| format!("invalid repair JSON: {e}"))
}

pub(super) fn create<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    parents(path, false)?;
    let bytes = canonical(value)?;
    if bytes.len() as u64 > LIMIT {
        return Err("repair output exceeds 4 MiB".into());
    }
    let mut file = options()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|_| "repair output already exists or cannot be created safely")?;
    file.write_all(&bytes)
        .and_then(|_| file.sync_all())
        .map_err(|_| "cannot durably write repair output")?;
    sync_dir(path.parent().ok_or("repair output has no parent")?)
}

// Flattening retains backup_ref and mapping at the journal's top level.
#[derive(Serialize)]
struct CheckedJournal {
    #[serde(flatten)]
    journal: Journal,
    checksum: String,
}
impl<'de> serde::Deserialize<'de> for CheckedJournal {
    fn deserialize<D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> std::result::Result<Self, D::Error> {
        let mut object = serde_json::Value::deserialize(deserializer)?;
        let map = object
            .as_object_mut()
            .ok_or_else(|| serde::de::Error::custom("journal must be an object"))?;
        let checksum = map
            .remove("checksum")
            .and_then(|value| value.as_str().map(str::to_string))
            .ok_or_else(|| serde::de::Error::custom("journal checksum missing or invalid"))?;
        let journal = serde_json::from_value(object).map_err(serde::de::Error::custom)?;
        Ok(Self { journal, checksum })
    }
}
fn validate_journal(journal: &Journal) -> Result<()> {
    if journal.schema_version != 1
        || journal.receipt.schema_version != 2
        || journal.receipt.policy_version != 2
        || journal.receipt.auth_context.len() != 64
        || !journal
            .receipt
            .auth_context
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        || journal.plan_id != hash(&journal.receipt)?
        || journal.apply_digest != hash(&(journal.plan_id.as_str(), &journal.candidates))?
        || journal.original_tip != journal.receipt.tip
        || journal.candidates.len() != journal.receipt.sources.len()
        || journal
            .candidates
            .iter()
            .zip(&journal.receipt.sources)
            .any(|(c, s)| c.source_oid != s.source_oid)
        || journal.backup_ref != format!("refs/commitguard/backups/{}", journal.plan_id)
        || journal.staging
            != journal
                .receipt
                .common_dir
                .join("commitguard-fix")
                .join(format!("operation-{}", journal.plan_id))
                .join(format!("staging-{}", journal.plan_id))
        || journal.editor_index > journal.candidates.len()
        || journal.mapping.len() > journal.receipt.sources.len()
        || journal.verified.len() != journal.mapping.len()
        || journal
            .verified
            .iter()
            .zip(&journal.mapping)
            .any(|(s, m)| s.source_oid != m.new_oid)
        || journal
            .mapping
            .iter()
            .zip(&journal.receipt.sources)
            .any(|(m, s)| m.source_oid != s.source_oid || !crate::core::oid_valid(&m.new_oid))
    {
        return Err("journal immutable receipt/candidate binding is invalid".into());
    }
    Ok(())
}
pub(super) fn save_journal(root: &Path, journal: &Journal) -> Result<()> {
    validate_journal(journal)?;
    let target = root.join("journal.json");
    parents(&target, true)?;
    if target.exists() || fs::symlink_metadata(&target).is_ok() {
        let previous = read_journal(root)?;
        if previous.plan_id != journal.plan_id || previous.apply_digest != journal.apply_digest {
            return Err("conflicting existing repair journal".into());
        }
    }
    let mut persisted = journal.clone();
    if let Some(failure) = &mut persisted.failure {
        // A hook's diagnostic must never crowd durable mapping records out.
        let mut end = failure.len().min(16 * 1024);
        while !failure.is_char_boundary(end) {
            end -= 1;
        }
        failure.truncate(end);
    }
    let checked = CheckedJournal {
        checksum: hash(&persisted)?,
        journal: persisted,
    };
    // An unfinished temporary write is evidence of an interrupted operation.
    let temporary = root.join("journal.json.tmp");
    create(&temporary, &checked)?;
    fs::rename(&temporary, &target).map_err(|_| "cannot promote repair journal")?;
    sync_dir(root)
}
pub(super) fn read_journal(root: &Path) -> Result<Journal> {
    let checked: CheckedJournal = read(&root.join("journal.json"), true)?;
    if checked.checksum != hash(&checked.journal)? {
        return Err("repair journal checksum mismatch".into());
    }
    validate_journal(&checked.journal)?;
    Ok(checked.journal)
}
pub(super) fn lock(root: &Path, id: &str) -> Result<()> {
    if id.len() != 64
        || !id
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err("invalid repair lock operation ID".into());
    }
    let path = root.join("lock");
    parents(&path, true)?;
    if fs::symlink_metadata(&path).is_ok() {
        let operation: String = read(&path, true)?;
        return Err(format!(
            "repair lock exists for {operation}; inspect journal and actual refs; never automatically break it"
        ));
    }
    create(&path, &id)
}
pub(super) fn release_lock(root: &Path) -> Result<()> {
    let path = root.join("lock");
    let _: String = read(&path, true)?;
    fs::remove_file(path).map_err(|_| "cannot release repair lock")?;
    sync_dir(root)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn canonical_objects_are_sorted_and_arrays_keep_order() {
        assert_eq!(
            canonical(&serde_json::json!({"z": {"b": 2, "a": 1}, "a": [2, 1]})).unwrap(),
            br#"{"a":[2,1],"z":{"a":1,"b":2}}"#
        );
        assert_eq!(
            hash(&serde_json::json!({"a":1,"b":2})).unwrap(),
            hash(&serde_json::json!({"b":2,"a":1})).unwrap()
        );
        assert_eq!(
            hash(&serde_json::json!({"a":1,"b":2})).unwrap(),
            "43258cff783fe7036d8a43033f830adfc60ec037382473548ac742b888292777"
        );
        assert_ne!(hash(&[1, 2]).unwrap(), hash(&[2, 1]).unwrap());
    }
    #[test]
    fn duplicate_fields_are_rejected_recursively() {
        assert!(serde_json::from_str::<UniqueValue>(r#"{"a":1,"a":2}"#).is_err());
        assert!(serde_json::from_str::<UniqueValue>(r#"{"a":[{"b":1,"b":2}]}"#).is_err());
        assert!(serde_json::from_str::<UniqueValue>(r#"{"a":1} trailing"#).is_err());
    }
    #[test]
    fn capacity_reserves_future_journal_growth_at_the_encoded_boundary() {
        assert!(ensure_capacity(&"x".repeat(LIMIT as usize - 2 - 128 * 1024), 128 * 1024).is_ok());
        assert!(ensure_capacity(&"x".repeat(LIMIT as usize - 1 - 128 * 1024), 128 * 1024).is_err());
        // JSON escaping counts, not the unencoded string's byte length.
        assert!(ensure_capacity(&"\0".repeat(LIMIT as usize / 6), 128 * 1024).is_err());
    }
    #[test]
    fn journal_that_fits_initially_is_rejected_when_verified_records_will_not_fit() {
        use super::super::{Candidate, Receipt, Source};
        let oid = "a".repeat(40);
        let author = "alice <123+alice@users.noreply.github.com> 1700000000 +0000";
        let message = "fix: source\n";
        let source = Source {
            source_oid: oid.clone(),
            parent: oid.clone(),
            tree: oid.clone(),
            author: author.into(),
            committer: author.into(),
            message: message.into(),
            raw: format!(
                "tree {oid}\nparent {oid}\nauthor {author}\ncommitter {author}\n\n{message}"
            ),
        };
        let receipt = Receipt {
            schema_version: 2,
            policy_version: 2,
            operation: "repair".into(),
            proposal_path: "/private/plan.json".into(),
            common_dir: "/repo/.git".into(),
            git_dir: "/repo/.git".into(),
            source_root: "/repo".into(),
            branch: "refs/heads/main".into(),
            object_format: "sha1".into(),
            tip: oid.clone(),
            base: oid.clone(),
            sources: vec![source; 4000],
            identity: crate::Identity {
                login: "alice".into(),
                email: "123+alice@users.noreply.github.com".into(),
            },
            auth_context: "f".repeat(64),
            ownership: Vec::new(),
            hooks_dir: "/hooks".into(),
            fingerprint: "f".repeat(64),
            destinations: Vec::new(),
        };
        let journal = Journal {
            schema_version: 1,
            plan_id: "f".repeat(64),
            apply_digest: "f".repeat(64),
            receipt,
            candidates: vec![
                Candidate {
                    source_oid: oid.clone(),
                    message: message.into()
                };
                4000
            ],
            original_tip: oid,
            backup_ref: "refs/commitguard/backups/test".into(),
            staging: "/repo/.git/commitguard-fix/staging".into(),
            staging_git_dir: None,
            phase: "prepared".into(),
            editor_index: 0,
            committer_date: "1800000000 +0000".into(),
            mapping: Vec::new(),
            verified: Vec::new(),
            failure: None,
        };
        let initial = CheckedJournal {
            checksum: "f".repeat(64),
            journal: journal.clone(),
        };
        assert!(
            canonical(&initial).unwrap().len() < LIMIT as usize,
            "fixture must fit before replacements exist"
        );
        assert!(
            journal_capacity(&journal)
                .unwrap_err()
                .contains("smaller unpublished suffix")
        );
        let mut small = journal;
        small.receipt.sources.truncate(1);
        small.candidates.truncate(1);
        assert!(journal_capacity(&small).is_ok());
    }
    #[cfg(unix)]
    #[test]
    fn fifo_input_is_rejected_without_waiting_for_a_writer() {
        use std::{
            ffi::CString,
            os::unix::{ffi::OsStrExt, fs::DirBuilderExt},
            time::Duration,
        };
        let directory = std::env::temp_dir()
            .canonicalize()
            .unwrap()
            .join(format!("commitguard-fifo-{}", std::process::id()));
        fs::DirBuilder::new()
            .mode(0o700)
            .create(&directory)
            .unwrap();
        let fifo = directory.join("input.json");
        let encoded = CString::new(fifo.as_os_str().as_bytes()).unwrap();
        assert_eq!(unsafe { libc::mkfifo(encoded.as_ptr(), 0o600) }, 0);
        let (send, receive) = std::sync::mpsc::channel();
        let reader = std::thread::spawn(move || {
            let _ = send.send(read::<String>(&fifo, true));
        });
        let result = receive.recv_timeout(Duration::from_secs(2));
        fs::remove_dir_all(directory).unwrap();
        assert!(
            result
                .expect("opening FIFO blocked instead of rejecting it")
                .unwrap_err()
                .contains("regular file")
        );
        reader.join().unwrap();
    }
    #[cfg(unix)]
    #[test]
    fn bounded_private_storage_rejects_symlinks_and_unsafe_modes() {
        use std::os::unix::fs::{DirBuilderExt, PermissionsExt, symlink};
        let directory = std::env::temp_dir()
            .canonicalize()
            .unwrap()
            .join(format!("commitguard-storage-{}", std::process::id()));
        fs::DirBuilder::new()
            .mode(0o700)
            .create(&directory)
            .unwrap();
        let path = directory.join("input.json");
        // A JSON string needs two quote bytes: this exact encoded boundary fits.
        create(&path, &"x".repeat(LIMIT as usize - 2)).unwrap();
        assert_eq!(
            read::<String>(&path, true).unwrap().len(),
            LIMIT as usize - 2
        );
        let oversized = directory.join("oversized.json");
        assert!(create(&oversized, &"x".repeat(LIMIT as usize - 1)).is_err());
        assert!(!oversized.exists());
        assert!(create(&path, &"overwrite").is_err());
        let link = directory.join("link.json");
        symlink(&path, &link).unwrap();
        assert!(read::<String>(&link, false).is_err());
        fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
        assert!(read::<String>(&path, true).is_err());
        assert!(read::<String>(&path, false).is_ok());
        fs::remove_dir_all(directory).unwrap();
    }
}
