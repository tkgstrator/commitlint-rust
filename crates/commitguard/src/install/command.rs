//! Isolated native setup command runner.
use super::paths::io;
use crate::Result;
use std::{
    path::{Path, PathBuf},
    process::Command,
};

pub(super) struct Setup {
    pub(super) home: PathBuf,
    pub(super) repo: Option<PathBuf>,
    pub(super) global: PathBuf,
}
impl Setup {
    pub(super) fn command(
        &self,
        git: &Path,
        args: &[&str],
        local: bool,
        optional: bool,
    ) -> Result<Vec<u8>> {
        let mut cmd = Command::new(git);
        cmd.args(args)
            .env("HOME", &self.home)
            .env("GIT_CONFIG_GLOBAL", &self.global)
            .env_remove("GH_DEBUG");
        if local {
            cmd.current_dir(self.repo.as_ref().ok_or("missing repository")?);
        }
        let output = io(cmd.output(), "run native setup command")?;
        if !output.status.success() && !optional {
            return Err("native setup command failed; activation will be rolled back".into());
        }
        Ok(if output.status.success() {
            output.stdout
        } else {
            Vec::new()
        })
    }
}
