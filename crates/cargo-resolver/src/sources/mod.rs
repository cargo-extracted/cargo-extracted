//! The paths Cargo's `sources` module provides to the moved files.
//!
//! Cargo's sources stay in Cargo. What the resolver uses from here is the
//! crates.io constants, [`IndexSummary`] and [`QueryKind`], which are copied,
//! and names it imports but no longer calls, which are empty stand-ins so the
//! import lines resolve.

pub mod path;
pub mod registry;
pub mod source;

pub use self::path::{PathSource, RecursivePathSource};
pub use self::registry::{
    CRATES_IO_DOMAIN, CRATES_IO_HTTP_INDEX, CRATES_IO_INDEX, CRATES_IO_REGISTRY, IndexSummary,
    RegistrySource,
};
pub use self::source::{QueryKind, Source};

/// Cargo's `DirectorySource`. Only named by an import here; see the module docs.
#[doc(hidden)]
pub struct DirectorySource;

/// Cargo's `GitSource`. Only named by an import here; see the module docs.
#[doc(hidden)]
pub struct GitSource;
