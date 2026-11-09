//! Cargo's dependency resolver and feature resolver.
//!
//! > This crate is maintained by the Cargo team, primarily for use by Cargo
//! > and not intended for external use (except as a transitive dependency). This
//! > crate may make major changes to its APIs or be deprecated without warning.
//!
//! Published as `cargo-ext-resolver`; the library is `cargo_resolver`, after
//! Cargo's module. The files are Cargo's, at the paths they have in Cargo's
//! `src/`: [`resolver`], the types it operates on in [`workspace`], the helpers
//! in [`util`]. The rest of the module tree is glue, so the moved code is
//! unchanged: re-exports, and stand-ins with Cargo's method names for what
//! Cargo has and this crate does not ([`GlobalContext`], [`Workspace`],
//! [`Package`], [`PackageSet`], [`Target`], [`RustcTargetData`]), which the
//! embedding application fills from what it knows.
//!
//! The entry point is [`resolve`]. It takes the [`Summary`] of each root
//! package with [`ResolveOpts`], a [`Registry`] to query, [`VersionPreferences`]
//! and a [`GlobalContext`], and produces a [`Resolve`]; [`encode`] converts
//! between that and `Cargo.lock`. [`FeatureResolver`] is the second pass.
//!
//! The crate performs no I/O; nothing is read from disk or the environment
//! unless the application puts it in the [`GlobalContext`]. Errors are
//! [`anyhow::Error`] through the [`CargoResult`] alias.

// Cargo's files are kept verbatim. Imports and items that Cargo uses elsewhere
// may be unused here, and their docs link to items that stayed in Cargo.
#![allow(unused_imports, dead_code, rustdoc::broken_intra_doc_links)]

pub use crate::util::errors::InternalError;
pub use crate::util::{CargoResult, GlobalContext, indented_lines};
pub use crate::version::version;

pub mod compiler;
pub mod context;
pub mod resolver;
pub mod shell;
pub mod sources;
pub mod util;
mod version;
pub mod workspace;

// The names consumers reach for, as in Cargo's `cargo::workspace::*`.
pub use self::compiler::{CompileKind, CompileTarget, Rustc, RustcTargetData};
pub use self::context::{ConfigKeyPath, Definition, OptValue, Value};
pub use self::resolver::encode;
pub use self::resolver::errors;
pub use self::resolver::errors::{ActivateError, ActivateResult, ResolveError};
pub use self::resolver::features::{
    ActivateMap, CliFeatures, DiffMap, FeatureOpts, FeatureResolver, FeaturesFor, ForceAllTargets,
    HasDevUnits, PackageFeaturesKey, RequestedFeatures, ResolvedFeatures,
};
pub use self::resolver::resolve;
pub use self::resolver::resolve::{Resolve, ResolveVersion};
pub use self::resolver::types::{FeaturesSet, ResolveBehavior, ResolveOpts};
pub use self::resolver::version_prefs::{
    PublishAgePolicy, PublishAgeViolation, VersionOrdering, VersionPreferences,
};
pub use self::shell::{ColorChoice, Shell, Verbosity};
pub use self::sources::source::QueryKind;
pub use self::sources::{
    CRATES_IO_DOMAIN, CRATES_IO_HTTP_INDEX, CRATES_IO_INDEX, CRATES_IO_REGISTRY, IndexSummary,
};
pub use self::util::edit_distance;
pub use self::util::graph::Graph;
pub use self::util::interning;
pub use self::util::interning::InternedString;
pub use self::util::into_url::IntoUrl;
pub use self::util::semver_ext::*;
pub use self::util::{CanonicalUrl, StableHasher};
pub use self::version::VersionInfo;
pub use self::workspace::dependency::{
    Artifact, ArtifactKind, ArtifactTarget, DepKind, Dependency, Patch, PatchLocation,
    SerializedDependency,
};
pub use self::workspace::features::{CliUnstable, Edition, Feature, Features};
pub use self::workspace::manifest::Target;
pub use self::workspace::package::{Package, PackageSet};
pub use self::workspace::package_id::{PackageId, PackageIdStableHash};
pub use self::workspace::package_id_spec::PackageIdSpecQuery;
pub use self::workspace::registry::Registry;
pub use self::workspace::source_id::{SourceId, SourceIdAsUrl};
pub use self::workspace::summary::{FeatureMap, FeatureValue, MissingDependencyError, Summary};
pub use self::workspace::workspace::Workspace;
pub use self::workspace::{GitReference, PackageIdSpec, SourceKind};
