//! Path projection, shell quoting and atomic managed-file replacement.
use crate::Result;
use std::{
    env, fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

pub(super) fn io<T>(result: std::io::Result<T>, action: &str) -> Result<T> {
    result.map_err(|_| format!("cannot {action}"))
}
pub(super) fn absolute(path: &Path) -> Result<PathBuf> {
    Ok(if path.is_absolute() {
        path.into()
    } else {
        io(env::current_dir(), "read working directory")?.join(path)
    })
}
pub(super) fn projected(path: &Path) -> Result<PathBuf> {
    projected_depth(path, 0)
}
fn projected_depth(path: &Path, depth: usize) -> Result<PathBuf> {
    if depth > 128 {
        return Err("setup path has too many symlink or parent levels".into());
    }
    if path.exists() {
        return io(fs::canonicalize(path), "resolve setup path");
    }
    if let Ok(meta) = fs::symlink_metadata(path)
        && meta.file_type().is_symlink()
    {
        let target = io(fs::read_link(path), "resolve setup symlink")?;
        return projected_depth(
            &if target.is_absolute() {
                target
            } else {
                path.parent().unwrap_or(Path::new(".")).join(target)
            },
            depth + 1,
        );
    }
    let parent = path.parent().ok_or("cannot resolve setup parent")?;
    let name = path.file_name().ok_or("cannot resolve setup filename")?;
    Ok(projected_depth(parent, depth + 1)?.join(name))
}
pub(super) fn quote(value: &Path) -> Result<String> {
    let text = value.to_str().ok_or("setup path is not UTF-8")?;
    if text.contains(['\n', '\r']) {
        return Err("setup paths must not contain line breaks".into());
    }
    Ok(format!("'{}'", text.replace('\'', "'\\''")))
}
pub(super) fn mode(path: &Path) -> u32 {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::metadata(path)
            .map(|m| m.permissions().mode() & 0o777)
            .unwrap_or(0o644)
    }
    #[cfg(not(unix))]
    {
        let _ = path;
        0o644
    }
}
pub(super) fn set_mode(path: &Path, value: u32) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        io(
            fs::set_permissions(path, fs::Permissions::from_mode(value)),
            "set managed file permissions",
        )?;
    }
    #[cfg(not(unix))]
    {
        let _ = (path, value);
    }
    Ok(())
}
// Replace executable inodes atomically: Linux refuses writes to a running image.
pub(super) fn replace_file(path: &Path, bytes: &[u8], permissions: u32) -> Result<()> {
    use std::io::Write;
    let parent = path.parent().ok_or("managed target lacks parent")?;
    io(fs::create_dir_all(parent), "create managed directory")?;
    let time = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| "system clock unavailable")?
        .as_nanos();
    let temporary = parent.join(format!(
        ".gh-commit-guard-{}-{time}.tmp",
        std::process::id()
    ));
    let result = (|| -> Result<()> {
        let mut file = io(
            fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&temporary),
            "create managed temporary file",
        )?;
        set_mode(&temporary, permissions)?;
        io(file.write_all(bytes), "write managed temporary file")?;
        io(file.sync_all(), "flush managed temporary file")?;
        io(fs::rename(&temporary, path), "activate managed file")?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn shell_quote() {
        assert_eq!(quote(Path::new("a'b c")).unwrap(), "'a'\\''b c'");
        assert!(quote(Path::new("bad\npath")).is_err());
    }
}
