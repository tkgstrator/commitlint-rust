pub mod auth;
pub mod bulk;
pub mod cli;
pub mod core;
pub mod fix;
pub mod hooks;
pub mod install;
pub mod policy;
pub mod provenance;
pub mod util;
pub mod wrapper;
pub use commitlint_rust;
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};
pub type Result<T> = std::result::Result<T, String>;
fn schema_version() -> u32 {
    1
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    #[serde(default = "schema_version")]
    pub schema_version: u32,
    pub git: PathBuf,
    pub gh: PathBuf,
    pub root: PathBuf,
    #[serde(default)]
    pub previous_hooks: Option<PathBuf>,
    #[serde(default)]
    pub repo_hooks: BTreeMap<String, String>,
    #[serde(default)]
    pub verify_programs: BTreeMap<String, String>,
}
impl Config {
    pub fn hooks(&self) -> PathBuf {
        self.root.join("hooks")
    }
    pub fn cli(&self) -> PathBuf {
        self.root.join("bin").join(if cfg!(windows) {
            "gh-commit-guard.exe"
        } else {
            "gh-commit-guard"
        })
    }
    pub fn canonical_cli(&self) -> PathBuf {
        self.root.join("bin").join(if cfg!(windows) {
            "commitguard.exe"
        } else {
            "commitguard"
        })
    }
    pub fn read(path: &Path) -> Result<Self> {
        let bytes =
            std::fs::read(path).map_err(|_| "cannot read guard configuration".to_string())?;
        let cfg: Self = serde_json::from_slice(&bytes)
            .map_err(|_| "invalid guard configuration".to_string())?;
        if cfg.schema_version != 1 {
            return Err("unsupported guard configuration version".into());
        }
        if !cfg.git.is_absolute() || !cfg.gh.is_absolute() || !cfg.root.is_absolute() {
            return Err("guard configuration paths must be absolute".into());
        }
        Ok(cfg)
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Identity {
    pub login: String,
    pub email: String,
}
