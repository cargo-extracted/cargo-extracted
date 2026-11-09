//! Stand-ins for Cargo's path sources.
//!
//! When a path dependency cannot be found, Cargo scans the directory for
//! packages to suggest. That reads manifests from disk, which this crate
//! does not do, so [`RecursivePathSource`] finds nothing and the error is
//! reported without suggestions.

use std::path::Path;

use crate::context::GlobalContext;
use crate::util::errors::CargoResult;
use crate::workspace::{Package, SourceId};

/// Cargo's `PathSource`. Only named by an import here.
#[doc(hidden)]
pub struct PathSource;

/// Cargo's `RecursivePathSource`, reduced to finding nothing.
pub struct RecursivePathSource<'gctx> {
    source_id: SourceId,
    path: std::path::PathBuf,
    gctx: &'gctx GlobalContext,
}

impl<'gctx> RecursivePathSource<'gctx> {
    pub fn new(root: &Path, source_id: SourceId, gctx: &'gctx GlobalContext) -> Self {
        Self {
            source_id,
            path: root.to_path_buf(),
            gctx,
        }
    }

    /// The packages under the root. Always empty here; see the module docs.
    pub fn read_packages(&self) -> CargoResult<Vec<Package>> {
        Ok(Vec::new())
    }
}
