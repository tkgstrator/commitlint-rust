//! Private immutable evidence and OS-lifetime operation locks.
use crate::Result;
use serde::Serialize;
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
};

fn no_symlinks(path: &Path) -> Result<()> {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|_| "bulk current directory unavailable")?
            .join(path)
    };
    #[cfg(target_os = "macos")]
    let mut absolute = absolute;
    #[cfg(target_os = "macos")]
    for alias in ["/var", "/tmp", "/etc"] {
        if let Ok(rest) = absolute.strip_prefix(alias) {
            use std::os::unix::fs::MetadataExt;
            let metadata = fs::symlink_metadata(alias).map_err(|_| "system alias unavailable")?;
            let expected = PathBuf::from(format!("/private{alias}"));
            let target = fs::read_link(alias).map_err(|_| "system alias unavailable")?;
            let target = if target.is_absolute() {
                target
            } else {
                Path::new("/").join(target)
            };
            if metadata.uid() != 0 || !metadata.file_type().is_symlink() || target != expected {
                return Err("untrusted system path alias".into());
            }
            absolute = expected.join(rest);
            break;
        }
    }
    let mut prefix = PathBuf::new();
    for component in absolute.components() {
        if matches!(component, std::path::Component::ParentDir) {
            return Err("bulk input paths must not contain parent traversal".into());
        }
        prefix.push(component);
        let metadata = fs::symlink_metadata(&prefix).map_err(|_| "bulk input path unavailable")?;
        if metadata.file_type().is_symlink() {
            return Err("bulk paths must not contain symlinks".into());
        }
    }
    Ok(())
}
fn open_read(path: &Path) -> Result<File> {
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_CLOEXEC);
    }
    options
        .open(path)
        .map_err(|_| "bulk file cannot be opened safely".into())
}
fn read_file(file: &mut File, limit: usize) -> Result<Vec<u8>> {
    let metadata = file
        .metadata()
        .map_err(|_| "bulk file metadata unavailable")?;
    if !metadata.is_file() || metadata.len() > limit as u64 {
        return Err("bulk input must be a bounded regular file".into());
    }
    let mut bytes = Vec::new();
    file.take(limit as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "bulk file read failed")?;
    if bytes.len() > limit {
        return Err("bulk input exceeds safe limit".into());
    }
    Ok(bytes)
}
pub(super) fn read_input(path: &Path, limit: usize) -> Result<Vec<u8>> {
    no_symlinks(path)?;
    read_file(&mut open_read(path)?, limit)
}
#[cfg(unix)]
fn private(metadata: &fs::Metadata, directory: bool) -> Result<()> {
    use std::os::unix::fs::MetadataExt;
    if metadata.uid() != unsafe { libc::geteuid() }
        || metadata.mode() & 0o777 != if directory { 0o700 } else { 0o600 }
        || (!directory && metadata.nlink() != 1)
        || (directory && !metadata.is_dir())
        || (!directory && !metadata.is_file())
    {
        return Err("bulk evidence must be private, owned and non-symlinked".into());
    }
    Ok(())
}
#[cfg(not(unix))]
fn private(_: &fs::Metadata, _: bool) -> Result<()> {
    Err("bulk writer requires Unix private files and OS locks".into())
}
fn directory(path: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        let mut builder = fs::DirBuilder::new();
        builder.mode(0o700);
        match builder.create(path) {
            Ok(()) => (),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => (),
            Err(_) => return Err("cannot create private bulk operation directory".into()),
        }
    }
    #[cfg(not(unix))]
    return Err("bulk writer requires Unix private directories".into());
    #[cfg(unix)]
    {
        private(
            &fs::symlink_metadata(path).map_err(|_| "bulk directory metadata unavailable")?,
            true,
        )?;
        no_symlinks(path)?;
        sync_dir(path.parent().ok_or("bulk directory has no parent")?)
    }
}
fn sync_dir(path: &Path) -> Result<()> {
    File::open(path)
        .and_then(|file| file.sync_all())
        .map_err(|_| "bulk directory sync failed".into())
}
fn create(path: &Path) -> std::io::Result<File> {
    let mut options = OpenOptions::new();
    options.write(true).read(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC);
    }
    options.open(path)
}
pub(super) struct Operation {
    pub path: PathBuf,
    _lock: File,
}
impl Operation {
    pub fn open(common: &Path, digest: &str) -> Result<Self> {
        let root = common.join("commitguard-bulk");
        directory(&root)?;
        let path = root.join(digest);
        directory(&path)?;
        let lock_path = path.join("lock");
        let lock = match create(&lock_path) {
            Ok(file) => {
                file.sync_all().map_err(|_| "bulk lock sync failed")?;
                sync_dir(&path)?;
                file
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                open_read(&lock_path)?
            }
            Err(_) => return Err("bulk lock creation failed".into()),
        };
        private(
            &lock
                .metadata()
                .map_err(|_| "bulk lock metadata unavailable")?,
            false,
        )?;
        #[cfg(unix)]
        {
            use std::os::fd::AsRawFd;
            if unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
                return Err("bulk operation is already locked".into());
            }
        }
        #[cfg(not(unix))]
        return Err("bulk writer requires OS lifetime locks".into());
        #[cfg(unix)]
        Ok(Self { path, _lock: lock })
    }
    pub fn snapshot(&self, name: &str, bytes: &[u8]) -> Result<()> {
        let path = self.path.join(name);
        match fs::symlink_metadata(&path) {
            Ok(_) => return self.compare(&path, bytes),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
            Err(_) => return Err("bulk snapshot metadata unavailable".into()),
        }
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let temporary = self.path.join(format!(
            ".{name}.tmp-{}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_err(|_| "bulk temporary clock unavailable")?
                .as_nanos(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        let mut file =
            create(&temporary).map_err(|_| "bulk exclusive temporary snapshot creation failed")?;
        private(
            &file
                .metadata()
                .map_err(|_| "bulk snapshot metadata unavailable")?,
            false,
        )?;
        file.write_all(bytes)
            .and_then(|_| file.sync_all())
            .map_err(|_| "bulk temporary snapshot write failed; evidence retained")?;
        match rename_exclusive(&temporary, &path) {
            Ok(()) => sync_dir(&self.path)?,
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                self.compare(&path, bytes)?
            }
            Err(_) => {
                return Err("bulk atomic snapshot publication failed; evidence retained".into());
            }
        }
        Ok(())
    }
    fn compare(&self, path: &Path, bytes: &[u8]) -> Result<()> {
        let mut file = open_read(path)?;
        private(
            &file
                .metadata()
                .map_err(|_| "bulk snapshot metadata unavailable")?,
            false,
        )?;
        if read_file(&mut file, bytes.len())? != bytes {
            return Err(
                "bulk retained snapshot does not match current digest; evidence retained".into(),
            );
        }
        Ok(())
    }
    pub fn record(&self, name: &str, value: &impl Serialize) -> Result<()> {
        self.snapshot(
            name,
            &serde_json::to_vec(value).map_err(|_| "bulk record serialization failed")?,
        )
    }
}

#[cfg(unix)]
fn rename_exclusive(from: &Path, to: &Path) -> std::io::Result<()> {
    use std::{ffi::CString, os::unix::ffi::OsStrExt};
    let from =
        CString::new(from.as_os_str().as_bytes()).map_err(|_| std::io::ErrorKind::InvalidInput)?;
    let to =
        CString::new(to.as_os_str().as_bytes()).map_err(|_| std::io::ErrorKind::InvalidInput)?;
    #[cfg(target_os = "macos")]
    let result = unsafe {
        libc::renameatx_np(
            libc::AT_FDCWD,
            from.as_ptr(),
            libc::AT_FDCWD,
            to.as_ptr(),
            libc::RENAME_EXCL,
        ) as libc::c_long
    };
    #[cfg(target_os = "linux")]
    let result = unsafe {
        libc::syscall(
            libc::SYS_renameat2,
            libc::AT_FDCWD,
            from.as_ptr(),
            libc::AT_FDCWD,
            to.as_ptr(),
            libc::RENAME_NOREPLACE,
        )
    };
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    return Err(std::io::ErrorKind::Unsupported.into());
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    if result == 0 {
        Ok(())
    } else {
        Err(std::io::Error::last_os_error())
    }
}
#[cfg(not(unix))]
fn rename_exclusive(_: &Path, _: &Path) -> std::io::Result<()> {
    Err(std::io::ErrorKind::Unsupported.into())
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    struct Temp(PathBuf);
    impl Temp {
        fn new() -> Self {
            static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
            let root = std::env::temp_dir().canonicalize().unwrap().join(format!(
                "bulk-state-{}-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos(),
                NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            ));
            fs::create_dir(&root).unwrap();
            Self(root)
        }
    }
    impl Drop for Temp {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.0).unwrap();
        }
    }
    #[test]
    fn os_lock_releases_with_owner_and_never_requires_stale_pid_cleanup() {
        let temp = Temp::new();
        let digest = "a".repeat(64);
        let first = Operation::open(&temp.0, &digest).unwrap();
        assert!(Operation::open(&temp.0, &digest).is_err());
        drop(first);
        assert!(Operation::open(&temp.0, &digest).is_ok());
    }
    #[test]
    fn interrupted_temporary_copy_does_not_publish_or_block_verified_snapshot() {
        let temp = Temp::new();
        let operation = Operation::open(&temp.0, &"b".repeat(64)).unwrap();
        let old = operation.path.join(".candidate-000000.commit.tmp-old");
        let mut partial = create(&old).unwrap();
        partial.write_all(b"partial").unwrap();
        partial.sync_all().unwrap();
        let target = operation.path.join("candidate-000000.commit");
        assert!(!target.exists());
        operation
            .snapshot("candidate-000000.commit", b"complete verified bytes")
            .unwrap();
        assert_eq!(fs::read(&target).unwrap(), b"complete verified bytes");
        assert_eq!(fs::read(&old).unwrap(), b"partial");
        operation
            .snapshot("candidate-000000.commit", b"complete verified bytes")
            .unwrap();
        fs::write(&target, b"tampered evidence").unwrap();
        assert!(
            operation
                .snapshot("candidate-000000.commit", b"complete verified bytes")
                .is_err()
        );
        assert_eq!(fs::read(&target).unwrap(), b"tampered evidence");
    }
    #[test]
    fn atomic_publication_cannot_overwrite_an_existing_snapshot() {
        let temp = Temp::new();
        let from = temp.0.join("from");
        let to = temp.0.join("to");
        fs::write(&from, b"new").unwrap();
        fs::write(&to, b"retained").unwrap();
        assert_eq!(
            rename_exclusive(&from, &to).unwrap_err().kind(),
            std::io::ErrorKind::AlreadyExists
        );
        assert_eq!(fs::read(&from).unwrap(), b"new");
        assert_eq!(fs::read(&to).unwrap(), b"retained");
    }
}
