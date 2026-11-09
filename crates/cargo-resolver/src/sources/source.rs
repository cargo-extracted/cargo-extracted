//! From Cargo's `sources/source.rs`: how a dependency query is performed.

/// Cargo's `Source` trait, the interface of every place packages come from.
/// Only named by an import here.
#[doc(hidden)]
pub trait Source {}

/// Defines how a dependency query will be performed for a [`Source`].
#[derive(Copy, Clone, PartialEq, Eq)]
pub enum QueryKind {
    /// A query for packages exactly matching the given dependency requirement.
    ///
    /// Each source gets to define what `exact` means for it.
    Exact,
    /// A query for packages close to the given dependency requirement.
    ///
    /// Each source gets to define what `close` means for it.
    ///
    /// Path/Git sources may return all dependencies that are at that URI,
    /// whereas an `Registry` source may return dependencies that are yanked or invalid.
    RejectedVersions,
    /// A query for packages close to the given dependency requirement.
    ///
    /// Each source gets to define what `close` means for it.
    ///
    /// Path/Git sources may return all dependencies that are at that URI,
    /// whereas an `Registry` source may return dependencies that have the same
    /// canonicalization.
    AlternativeNames,
    /// Match a dependency in all ways and will normalize the package name.
    /// Each source defines what normalizing means.
    Normalized,
}
