//! Bounded native Git plumbing; no caller-supplied filenames reach Git.
use super::MAX_BYTES;
use crate::{Result, core, util};
use std::{
    path::{Path, PathBuf},
    time::Duration,
};

pub(super) struct Git {
    program: PathBuf,
    pub common: PathBuf,
    pub width: usize,
}
fn capture(program: &Path, args: &[String], input: Option<&[u8]>) -> Result<Vec<u8>> {
    // Remove every inherited Git override, including numbered config entries,
    // alternate databases, namespaces, tracing and SSH helpers. Native plumbing
    // reads only the repository discovered in the caller's actual working dir.
    let mut removed: Vec<String> = std::env::vars_os()
        .filter_map(|(key, _)| {
            let key = key.to_str()?;
            (key.starts_with("GIT_")
                || key.starts_with("SSH_")
                || matches!(key, "GH_DEBUG" | "DEBUG"))
            .then(|| key.to_string())
        })
        .collect();
    removed.sort();
    let mut environment: Vec<(&str, Option<&str>)> =
        removed.iter().map(|key| (key.as_str(), None)).collect();
    environment.extend([
        ("GIT_NO_REPLACE_OBJECTS", Some("1")),
        ("GIT_NO_LAZY_FETCH", Some("1")),
        ("GIT_TERMINAL_PROMPT", Some("0")),
        ("GIT_CONFIG_NOSYSTEM", Some("1")),
        ("GIT_CONFIG_GLOBAL", Some("/dev/null")),
        ("GIT_OPTIONAL_LOCKS", Some("0")),
    ]);
    let output = util::capture_env_bounded(
        program,
        args,
        input,
        Duration::from_secs(180),
        &environment,
        MAX_BYTES,
    )?;
    if output.code != 0 {
        return Err("bulk native Git plumbing failed".into());
    }
    Ok(output.stdout)
}
impl Git {
    pub fn discover(program: &Path) -> Result<Self> {
        let bytes = capture(
            program,
            &[
                "rev-parse",
                "--path-format=absolute",
                "--git-common-dir",
                "--show-object-format",
            ]
            .map(str::to_string),
            None,
        )?;
        let lines: Vec<&str> = core::text(&bytes)?.lines().collect();
        if lines.len() != 2 {
            return Err(
                "bulk cannot discover repository common directory and object format".into(),
            );
        }
        let path = PathBuf::from(lines[0]);
        if !path.is_absolute() {
            return Err("bulk common directory is not absolute".into());
        }
        let common = path
            .canonicalize()
            .map_err(|_| "bulk common directory unavailable")?;
        let width = match lines[1] {
            "sha1" => 40,
            "sha256" => 64,
            _ => return Err("unsupported bulk object format".into()),
        };
        Ok(Self {
            program: program.into(),
            common,
            width,
        })
    }
    pub fn oid(&self, value: &str) -> Result<()> {
        if value.len() != self.width || !core::oid_valid(value) {
            return Err("bulk requires full OIDs of the repository object format".into());
        }
        Ok(())
    }
    fn args(&self) -> Result<Vec<String>> {
        Ok(vec![
            "--git-dir".into(),
            self.common
                .to_str()
                .ok_or("bulk Git directory must be UTF-8")?
                .into(),
        ])
    }
    pub fn batch(&self, oids: &[String], kind: &str) -> Result<Vec<Vec<u8>>> {
        if oids.is_empty() {
            return Ok(Vec::new());
        }
        for oid in oids {
            self.oid(oid)?;
        }
        let mut args = self.args()?;
        args.extend(["cat-file".into(), "--batch".into()]);
        let input = format!("{}\n", oids.join("\n"));
        let bytes = capture(&self.program, &args, Some(input.as_bytes()))?;
        let mut cursor: usize = 0;
        let mut result = Vec::with_capacity(oids.len());
        for oid in oids {
            let end = bytes
                .get(cursor..)
                .and_then(|b| b.iter().position(|v| *v == b'\n'))
                .map(|n| cursor + n)
                .ok_or("truncated bulk batch header")?;
            let header = core::text(&bytes[cursor..end])?;
            let fields: Vec<&str> = header.split(' ').collect();
            if fields.len() != 3 || fields[0] != oid || fields[1] != kind {
                return Err("bulk batch object missing, wrong type or wrong OID".into());
            }
            let size: usize = fields[2].parse().map_err(|_| "invalid bulk batch size")?;
            if size > MAX_BYTES {
                return Err("bulk batch object exceeds safe limit".into());
            }
            let start = end + 1;
            let finish = start.checked_add(size).ok_or("bulk batch size overflow")?;
            if bytes.get(finish) != Some(&b'\n') {
                return Err("truncated bulk batch object".into());
            }
            result.push(
                bytes
                    .get(start..finish)
                    .ok_or("truncated bulk batch bytes")?
                    .to_vec(),
            );
            cursor = finish + 1;
        }
        if cursor != bytes.len() {
            return Err("unexpected trailing bulk batch output".into());
        }
        Ok(result)
    }
    pub fn hash(
        &self,
        operation: &Path,
        names: &[String],
        expected: &[String],
        write: bool,
    ) -> Result<()> {
        self.hash_kind("commit", operation, names, expected, write)
    }
    pub fn hash_kind(
        &self,
        kind: &str,
        operation: &Path,
        names: &[String],
        expected: &[String],
        write: bool,
    ) -> Result<()> {
        if !matches!(kind, "commit" | "tag") {
            return Err("unsupported bulk hash object type".into());
        }
        if names.len() != expected.len() {
            return Err("bulk internal hash list mismatch".into());
        }
        for name in names {
            if name.is_empty()
                || !name
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'.')
            {
                return Err("unsafe internal bulk snapshot filename".into());
            }
        }
        let mut args = vec![
            "-C".into(),
            operation
                .to_str()
                .ok_or("bulk operation path must be UTF-8")?
                .into(),
        ];
        args.extend(self.args()?);
        args.extend(
            ["hash-object", "-t", kind, "--stdin-paths", "--no-filters"].map(str::to_string),
        );
        if write {
            // Make the object data durable before a durable complete record.
            args.splice(
                0..0,
                [
                    "-c".into(),
                    "core.fsync=loose-object".into(),
                    "-c".into(),
                    "core.fsyncMethod=fsync".into(),
                ],
            );
            args.push("-w".into());
        }
        let input = format!("{}\n", names.join("\n"));
        let output = capture(&self.program, &args, Some(input.as_bytes()))?;
        let wanted = format!("{}\n", expected.join("\n"));
        if output != wanted.as_bytes() {
            return Err(if write {
                "bulk object write OID mismatch; intent mapping retained"
            } else {
                "bulk dry hash does not match source and expected OIDs"
            }
            .into());
        }
        Ok(())
    }
}
