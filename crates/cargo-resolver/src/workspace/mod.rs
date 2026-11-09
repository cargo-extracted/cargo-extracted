//! The paths Cargo's `workspace` module provides to the moved files.
//!
//! `dependency`, `features`, `package_id`, `package_id_spec`, `source_id` and
//! `summary` are Cargo's files. `registry` is the [`Registry`] trait, copied
//! from Cargo's `workspace/registry.rs` without the `PackageRegistry` that
//! implements it over Cargo's sources. `manifest`, `package` and `workspace`
//! are stand-ins for Cargo's `Target`, `Package`, `PackageSet` and
//! `Workspace`: the methods the moved code calls, over data the embedding
//! application supplies. `parser` holds the one parser function `SourceId`
//! calls.

pub use self::dependency::{Dependency, Patch, PatchLocation, SerializedDependency};
pub use self::features::{CliUnstable, Edition, Feature, Features};
pub use self::manifest::Target;
pub use self::package::{Package, PackageSet};
pub use self::package_id::PackageId;
pub use self::package_id_spec::PackageIdSpecQuery;
pub use self::registry::Registry;
pub use self::source_id::SourceId;
pub use self::summary::{FeatureMap, FeatureValue, Summary};
pub use self::workspace::Workspace;

pub use cargo_util_schemas::core::{GitReference, PackageIdSpec, SourceKind};

pub mod dependency;
pub mod features;
pub mod manifest;
pub mod package;
pub mod package_id;
pub mod package_id_spec;
pub mod parser;
pub mod registry;
pub mod source_id;
pub mod summary;
pub mod workspace;
