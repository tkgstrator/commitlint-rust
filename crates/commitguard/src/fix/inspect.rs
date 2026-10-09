//! Read-only, fail-closed inspection of the source repository.
mod exemptions;
mod fingerprint;
mod lifecycle;
mod ownership;
mod paths;
mod publication;
mod repository;
mod shared_state;
mod source;

#[cfg(all(test, unix))]
mod test_support;

pub(super) use lifecycle::{inspect, postcheck, recheck};
pub(super) use paths::safe_path;
pub(super) use source::source;
