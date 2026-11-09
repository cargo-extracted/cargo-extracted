//! From Cargo's `compiler/compile_kind.rs`: the enums, without the
//! constructors that need `rustc` or configuration.

use std::hash::{Hash, Hasher};
use std::path::Path;
use std::path::PathBuf;

use anyhow::Context as _;
use anyhow::bail;
use serde::Serialize;

use crate::util::StableHasher;
use crate::util::errors::CargoResult;
use crate::util::interning::InternedString;

/// Indicator for how a unit is being compiled.
///
/// This is used primarily for organizing cross compilations vs host
/// compilations, where cross compilations happen at the request of `--target`
/// and host compilations happen for things like build scripts and procedural
/// macros.
#[derive(PartialEq, Eq, Hash, Debug, Clone, Copy, PartialOrd, Ord)]
pub enum CompileKind {
    /// Attached to a unit that is compiled for the "host" system or otherwise
    /// is compiled without a `--target` flag. This is used for procedural
    /// macros and build scripts, or if the `--target` flag isn't passed.
    Host,

    /// Attached to a unit to be compiled for a particular target. This is used
    /// for units when the `--target` flag is passed.
    Target(CompileTarget),
}

impl CompileKind {
    pub fn is_host(&self) -> bool {
        matches!(self, CompileKind::Host)
    }

    /// Hash used for fingerprinting.
    ///
    /// Metadata hashing uses the normal Hash trait, which does not
    /// differentiate on `.json` file contents. The fingerprint hash does
    /// check the contents.
    pub fn fingerprint_hash(&self) -> u64 {
        match self {
            CompileKind::Host => 0,
            CompileKind::Target(target) => target.fingerprint_hash(),
        }
    }
}

impl serde::ser::Serialize for CompileKind {
    fn serialize<S>(&self, s: S) -> Result<S::Ok, S::Error>
    where
        S: serde::ser::Serializer,
    {
        match self {
            CompileKind::Host => None::<&str>.serialize(s),
            CompileKind::Target(t) => Some(t.rustc_target()).serialize(s),
        }
    }
}

/// Abstraction for the representation of a compilation target that Cargo has.
///
/// Compilation targets are one of two things right now:
///
/// 1. A raw target string, like `x86_64-unknown-linux-gnu`.
/// 2. The path to a JSON file, such as `/path/to/my-target.json`.
///
/// Raw target strings are typically dictated by `rustc` itself and represent
/// built-in targets. Custom JSON files are somewhat unstable, but supported
/// here in Cargo. Note that for JSON target files this `CompileTarget` stores a
/// full canonicalized path to the target.
///
/// The main reason for this existence is to handle JSON target files where when
/// we call rustc we pass full paths but when we use it for Cargo's purposes
/// like naming directories or looking up configuration keys we only check the
/// file stem of JSON target files. For built-in rustc targets this is just an
/// uninterpreted string basically.
#[derive(PartialEq, Eq, Hash, Debug, Clone, Copy, PartialOrd, Ord, Serialize)]
pub enum CompileTarget {
    Tuple(InternedString),
    Json {
        short: InternedString,
        path: InternedString,
    },
}

impl CompileTarget {
    pub fn new(name: &str, unstable_json: bool) -> CargoResult<CompileTarget> {
        let name = name.trim();
        if name.is_empty() {
            bail!("target was empty");
        }
        if !name.ends_with(".json") {
            return Ok(CompileTarget::Tuple(name.into()));
        }

        if !unstable_json {
            bail!(
                "`.json` target specs require -Zjson-target-spec to be added to the cargo invocation"
            );
        }

        // If `name` ends in `.json` then it's likely a custom target
        // specification. Canonicalize the path to ensure that different builds
        // with different paths always produce the same result.
        let p = try_canonicalize(Path::new(name))
            .with_context(|| format!("target path `{name}` is not a valid file"))?;
        let path = p
            .to_str()
            .ok_or_else(|| anyhow::format_err!("target path `{name}` is not valid unicode"))?
            .into();
        let short = p.file_stem().unwrap().to_str().unwrap().into();
        Ok(CompileTarget::Json { short, path })
    }

    /// Returns the full unqualified name of this target, suitable for passing
    /// to `rustc` directly.
    ///
    /// Typically this is pretty much the same as `short_name`, but for the case
    /// of JSON target files this will be a full canonicalized path name for the
    /// current filesystem.
    pub fn rustc_target(&self) -> InternedString {
        match self {
            CompileTarget::Tuple(name) => *name,
            CompileTarget::Json { path, .. } => *path,
        }
    }

    /// Returns a "short" version of the target name suitable for usage within
    /// Cargo for configuration and such.
    ///
    /// This is typically the same as `rustc_target`, or the full name, but for
    /// JSON target files this returns just the file stem (e.g. `foo` out of
    /// `foo.json`) instead of the full path.
    pub fn short_name(&self) -> &str {
        match self {
            CompileTarget::Tuple(name) => name,
            CompileTarget::Json { short, .. } => short,
        }
    }

    /// See [`CompileKind::fingerprint_hash`].
    pub fn fingerprint_hash(&self) -> u64 {
        let mut hasher = StableHasher::new();
        match self {
            CompileTarget::Tuple(name) => name.hash(&mut hasher),
            CompileTarget::Json { short, path } => {
                short.hash(&mut hasher);
                match std::fs::read(path.as_str()) {
                    Ok(contents) => contents.hash(&mut hasher),
                    Err(_) => path.hash(&mut hasher),
                }
            }
        }
        Hasher::finish(&hasher)
    }
}

/// From `cargo_util::paths::try_canonicalize`, so that this crate does not
/// depend on `cargo-util`.
#[cfg(not(windows))]
#[inline]
fn try_canonicalize<P: AsRef<Path>>(path: P) -> std::io::Result<PathBuf> {
    std::fs::canonicalize(&path)
}

#[cfg(windows)]
#[inline]
fn try_canonicalize<P: AsRef<Path>>(path: P) -> std::io::Result<PathBuf> {
    use std::io::Error;
    use std::io::ErrorKind;

    // On Windows `canonicalize` may fail, so we fall back to getting an absolute path.
    std::fs::canonicalize(&path).or_else(|_| {
        // Return an error if a file does not exist for better compatibility with `canonicalize`
        if !path.as_ref().try_exists()? {
            return Err(Error::new(ErrorKind::NotFound, "the path was not found"));
        }
        std::path::absolute(&path)
    })
}
