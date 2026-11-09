//! From Cargo's `workspace/registry.rs`: the [`Registry`] trait, the
//! resolver's view of the world of packages.
//!
//! Cargo implements it for its `PackageRegistry`, which aggregates every
//! source a workspace may pull packages from and applies `[patch]` and
//! `[replace]`; that stays in Cargo. Tests implement it with an in-memory
//! list of [`Summary`] values.

use crate::sources::IndexSummary;
use crate::sources::source::QueryKind;
use crate::util::errors::CargoResult;
use crate::workspace::{Dependency, SourceId, Summary};

/// An abstraction provides a set of methods for querying source information
/// about a group of packages, without leaking too much implementation details
/// of the actual registry.
///
/// As of 2024-04, only [`PackageRegistry`] and `MyRegistry` in resolver-tests
/// are found implementing this.
///
/// See also the [`Source`](crate::sources::source::Source) trait, as many of
/// the methods here mirror and abstract over its functionalities.
#[allow(async_fn_in_trait)]
pub trait Registry {
    /// Attempt to find the packages that match a dependency request.
    async fn query(
        &self,
        dep: &Dependency,
        kind: QueryKind,
        f: &mut dyn FnMut(IndexSummary),
    ) -> CargoResult<()>;

    /// Gathers the result from [`Registry::query`] as a list of [`IndexSummary`] items
    /// when they become available.
    async fn query_vec(&self, dep: &Dependency, kind: QueryKind) -> CargoResult<Vec<IndexSummary>> {
        let mut ret = Vec::new();
        self.query(dep, kind, &mut |s| ret.push(s))
            .await
            .map(|()| ret)
    }

    /// Gets the description of a source, to provide useful messages.
    fn describe_source(&self, source: SourceId) -> String;

    /// Checks if a source is replaced with some other source.
    fn is_replaced(&self, source: SourceId) -> bool;
}
