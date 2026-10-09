//! Backup-first managed writes with reverse-order rollback.
use super::{
    assets::NAME,
    paths::{absolute, io, mode, projected, replace_file, set_mode},
};
use crate::Result;
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

pub(super) struct Transaction {
    saved: BTreeMap<PathBuf, Option<(Vec<u8>, u32)>>,
    order: Vec<PathBuf>,
    backup: PathBuf,
}
impl Transaction {
    pub(super) fn new(home: &Path) -> Result<Self> {
        let time = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| "system clock unavailable")?
            .as_nanos();
        Ok(Self {
            saved: BTreeMap::new(),
            order: Vec::new(),
            backup: home
                .join(".local/share")
                .join(NAME)
                .join("backups")
                .join(format!("{time}-{}", std::process::id())),
        })
    }
    pub(super) fn save(&mut self, path: &Path) -> Result<()> {
        let path = projected(&absolute(path)?)?;
        if self.saved.contains_key(&path) {
            return Ok(());
        }
        let previous = if path.exists() {
            if !path.is_file() {
                return Err("managed target is not a regular file".into());
            }
            Some((io(fs::read(&path), "read managed file")?, mode(&path)))
        } else {
            None
        };
        if let Some((bytes, _)) = &previous {
            io(fs::create_dir_all(&self.backup), "create backup directory")?;
            set_mode(&self.backup, 0o700)?;
            let dest = self.backup.join(format!("{}.bak", self.order.len()));
            io(fs::write(&dest, bytes), "save backup file")?;
            set_mode(&dest, 0o600)?;
        }
        self.order.push(path.clone());
        self.saved.insert(path, previous);
        if self.backup.exists() {
            let manifest = serde_json::to_vec_pretty(&self.order)
                .map_err(|_| "cannot serialize backup paths")?;
            let path = self.backup.join("paths.json");
            io(fs::write(&path, manifest), "write backup manifest")?;
            set_mode(&path, 0o600)?;
        }
        Ok(())
    }
    pub(super) fn write(&mut self, path: &Path, bytes: &[u8], permissions: u32) -> Result<()> {
        let path = projected(&absolute(path)?)?;
        if fs::read(&path).ok().as_deref() == Some(bytes) && mode(&path) == permissions {
            return Ok(());
        }
        self.save(&path)?;
        replace_file(&path, bytes, permissions)
    }
    pub(super) fn rollback(&self) -> bool {
        let mut success = true;
        for path in self.order.iter().rev() {
            match &self.saved[path] {
                Some((data, m)) => {
                    if replace_file(path, data, *m).is_err() {
                        success = false
                    }
                }
                None => {
                    if path.exists() && fs::remove_file(path).is_err() {
                        success = false
                    }
                }
            }
        }
        success
    }
}
