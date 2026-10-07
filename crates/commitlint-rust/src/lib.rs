//! Message-only Conventional Commit linting. No account, identity,
//! attribution, hook or configuration concerns live in this crate.
pub mod cli;
pub mod git;
pub mod policy;

pub use policy::lint_message;
pub type Result<T> = std::result::Result<T, String>;
