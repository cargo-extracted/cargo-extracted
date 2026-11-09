//! From Cargo's `version.rs`: what `crate::version()` reports.
//!
//! Cargo fills this in from its build script and the bootstrap environment.
//! The moved code only asks for the release channel, to decide whether
//! unstable features are allowed by default and how to word a message about
//! them, and for a version to print beside it.

use std::fmt;

/// Information about the version of this crate, in the shape of Cargo's.
#[derive(Clone, Debug)]
pub struct VersionInfo {
    /// This crate's version.
    pub version: String,
    /// The release channel Cargo would consider itself built for.
    ///
    /// `CFG_RELEASE_CHANNEL` at build time if set, as for Cargo; `None`
    /// otherwise, which Cargo's code treats as `dev`. That is also what a Cargo
    /// built from source outside of bootstrap reports, and like such a Cargo
    /// it allows unstable features. [`GlobalContext::nightly_features_allowed`]
    /// is where an embedding application decides that for itself.
    ///
    /// [`GlobalContext::nightly_features_allowed`]: crate::GlobalContext::nightly_features_allowed
    pub release_channel: Option<String>,
}

impl fmt::Display for VersionInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.version)
    }
}

/// Returns information about this crate's version.
pub fn version() -> VersionInfo {
    VersionInfo {
        version: env!("CARGO_PKG_VERSION").to_string(),
        release_channel: option_env!("CFG_RELEASE_CHANNEL").map(|s| s.to_string()),
    }
}
