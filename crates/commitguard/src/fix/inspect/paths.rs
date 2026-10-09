//! Repair path resolution and symlink policy.
use super::repository::{dirs, worktrees};
use crate::Result;
use std::{
    env, fs,
    path::{Component, Path, PathBuf},
};

pub(super) fn unix() -> Result<()> {
    if !cfg!(unix) {
        return Err("commitguard fix requires Unix".into());
    }
    Ok(())
}

pub(super) fn absolute(path: &Path) -> Result<PathBuf> {
    path.canonicalize()
        .map_err(|_| "cannot resolve repository path".into())
}

/// Resolve the existing parent after rejecting user-controlled symlinks, then
/// bind the real absolute destination without following a final file symlink.
/// All worktree checks use this resolved destination.
pub(in crate::fix) fn safe_path(path: &Path, git: &Path) -> Result<PathBuf> {
    unix()?;
    let path = if path.is_absolute() {
        path.to_path_buf()
    } else {
        env::current_dir()
            .map_err(|_| "cannot locate current directory")?
            .join(path)
    };
    reject_symlinks(&path)?;
    let name = path.file_name().ok_or("file path required")?;
    let parent = absolute(path.parent().ok_or("file parent required")?)?;
    let resolved = parent.join(name);
    reject_symlinks(&resolved)?;
    let (common, _, _) = dirs(git)?;
    if !resolved.starts_with(&common) {
        for worktree in worktrees(git)? {
            if resolved.starts_with(worktree.path) {
                return Err("proposal/ownership files must be outside every worktree or under the common Git directory".into());
            }
        }
    }
    Ok(resolved)
}

/// macOS exposes these root-owned aliases on its protected root filesystem.
/// Accept only their exact system targets; aliases elsewhere are never trusted.
fn trusted_system_alias(path: &Path, metadata: &fs::Metadata) -> bool {
    #[cfg(target_os = "macos")]
    {
        use std::os::unix::fs::MetadataExt;
        let target = match path.to_str() {
            Some("/var") => Path::new("/private/var"),
            Some("/tmp") => Path::new("/private/tmp"),
            Some("/etc") => Path::new("/private/etc"),
            _ => return false,
        };
        if metadata.uid() != 0 {
            return false;
        }
        // These directories control both the alias entry and its destination.
        // /private/tmp itself is intentionally writable and is not an owner
        // of either root-level directory entry.
        for parent in [Path::new("/"), Path::new("/private")] {
            let Ok(m) = fs::symlink_metadata(parent) else {
                return false;
            };
            if !m.is_dir() || m.uid() != 0 || m.mode() & 0o022 != 0 {
                return false;
            }
        }
        let Ok(link) = fs::read_link(path) else {
            return false;
        };
        let link = if link.is_absolute() {
            link
        } else {
            Path::new("/").join(link)
        };
        link == target && fs::symlink_metadata(target).is_ok_and(|m| m.is_dir() && m.uid() == 0)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (path, metadata);
        false
    }
}

pub(super) fn reject_symlinks(path: &Path) -> Result<()> {
    let mut current = PathBuf::new();
    for part in path.components() {
        match part {
            Component::ParentDir => {
                return Err("parent traversal is unsupported in repair paths".into());
            }
            Component::CurDir => continue,
            _ => current.push(part.as_os_str()),
        }
        match fs::symlink_metadata(&current) {
            Ok(metadata)
                if metadata.file_type().is_symlink()
                    && !trusted_system_alias(&current, &metadata) =>
            {
                return Err("symlink repair path is forbidden".into());
            }
            Ok(_) => (),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
            Err(_) => return Err("cannot inspect repair path".into()),
        }
    }
    Ok(())
}

#[cfg(all(test, unix))]
mod tests {
    use super::reject_symlinks;
    use crate::fix::inspect::test_support::TestDirectory;
    #[cfg(target_os = "macos")]
    use std::path::Path;
    use std::{fs, os::unix::fs::symlink};

    #[test]
    fn repair_paths_reject_user_symlink_parents_and_leaves() {
        let root = TestDirectory::new();
        fs::create_dir(root.0.join("real")).unwrap();
        symlink(root.0.join("real"), root.0.join("alias")).unwrap();
        assert!(reject_symlinks(&root.0.join("alias/new.json")).is_err());
        fs::write(root.0.join("real/source.json"), b"{}").unwrap();
        symlink(root.0.join("real/source.json"), root.0.join("leaf.json")).unwrap();
        assert!(reject_symlinks(&root.0.join("leaf.json")).is_err());
        assert!(reject_symlinks(&root.0.join("real/new.json")).is_ok());
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn repair_paths_accept_only_protected_macos_system_aliases() {
        for alias in ["/var", "/tmp", "/etc"] {
            assert!(reject_symlinks(Path::new(alias)).is_ok(), "{alias}");
        }
        let root = TestDirectory::new();
        assert!(reject_symlinks(&root.0.join("new.json")).is_ok());
        // A user-owned alias to a trusted target still must not gain trust.
        symlink("/private/tmp", root.0.join("tmp")).unwrap();
        assert!(reject_symlinks(&root.0.join("tmp/new.json")).is_err());
    }
}
