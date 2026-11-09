//! Stand-in for Cargo's `Workspace`, as far as the resolver looks at one.
//!
//! The lock file code walks the workspace members and their path
//! dependencies to tell which lock entries are path packages, and reads the
//! `[patch]` and `[replace]` tables. The feature resolver asks for the
//! resolver behavior and which members (with which features) to resolve for.
//! Cargo's `Workspace` discovers all of that from manifests on disk; here the
//! embedding application supplies it.

use crate::util::data_structures::HashMap;
use std::path::Path;

use url::Url;

use crate::context::GlobalContext;
use crate::resolver::{CliFeatures, ResolveBehavior};
use crate::util::errors::CargoResult;
use crate::workspace::dependency::Patch;
use crate::workspace::package_id_spec::PackageIdSpecQuery;
use crate::workspace::{Dependency, Package, PackageIdSpec};

/// A workspace: the part of Cargo's `Workspace` the resolver uses.
pub struct Workspace<'gctx> {
    gctx: &'gctx GlobalContext,
    members: Vec<Package>,
    /// Path packages that are not members but may be reached through path
    /// dependencies, by manifest path; see [`Workspace::load`].
    path_packages: HashMap<std::path::PathBuf, Package>,
    root_patch: HashMap<Url, Vec<Patch>>,
    root_replace: Vec<(PackageIdSpec, Dependency)>,
    resolve_behavior: ResolveBehavior,
}

impl<'gctx> Workspace<'gctx> {
    /// A workspace with the given members.
    ///
    /// The resolver behavior defaults to that of the newest edition, as it
    /// does when a manifest does not set `resolver`; see
    /// [`Workspace::set_resolve_behavior`].
    pub fn new(gctx: &'gctx GlobalContext, members: Vec<Package>) -> Workspace<'gctx> {
        Workspace {
            gctx,
            members,
            path_packages: HashMap::default(),
            root_patch: HashMap::default(),
            root_replace: Vec::new(),
            resolve_behavior: ResolveBehavior::V2,
        }
    }

    pub fn gctx(&self) -> &'gctx GlobalContext {
        self.gctx
    }

    /// Sets what Cargo derives from the root manifest's `resolver` key and
    /// edition.
    pub fn set_resolve_behavior(&mut self, behavior: ResolveBehavior) {
        self.resolve_behavior = behavior;
    }

    /// Sets the root manifest's `[patch]` table, keyed by the URL of the
    /// patched source as Cargo keys it.
    pub fn set_root_patch(&mut self, patch: HashMap<Url, Vec<Patch>>) {
        self.root_patch = patch;
    }

    /// Sets the root manifest's `[replace]` table.
    pub fn set_root_replace(&mut self, replace: Vec<(PackageIdSpec, Dependency)>) {
        self.root_replace = replace;
    }

    /// Makes a path package outside the members known, so that
    /// [`Workspace::load`] finds it when the lock file code follows a path
    /// dependency to it.
    pub fn add_path_package(&mut self, package: Package) {
        self.path_packages
            .insert(package.manifest_path().to_path_buf(), package);
    }

    /// Returns an iterator over all packages in this workspace
    pub fn members(&self) -> impl Iterator<Item = &Package> {
        self.members.iter()
    }

    pub fn root_patch(&self) -> CargoResult<HashMap<Url, Vec<Patch>>> {
        Ok(self.root_patch.clone())
    }

    pub fn root_replace(&self) -> &[(PackageIdSpec, Dependency)] {
        &self.root_replace
    }

    pub fn resolve_behavior(&self) -> ResolveBehavior {
        self.resolve_behavior
    }

    /// Loads the package at `manifest_path`.
    ///
    /// Cargo reads the manifest. Here the package must have been supplied, as
    /// a member or through [`Workspace::add_path_package`]; otherwise this
    /// fails, and the lock file code treats the path dependency as absent,
    /// which drops its entry from the lock file.
    pub fn load(&self, manifest_path: &Path) -> CargoResult<Package> {
        if let Some(pkg) = self.path_packages.get(manifest_path) {
            return Ok(pkg.clone());
        }
        if let Some(pkg) = self
            .members
            .iter()
            .find(|p| p.manifest_path() == manifest_path)
        {
            return Ok(pkg.clone());
        }
        anyhow::bail!(
            "package at `{}` is not known to this workspace",
            manifest_path.display()
        )
    }

    /// Returns the members to resolve features for, with the features
    /// requested of each.
    ///
    /// Cargo's version also handles the `--features` syntax that names a
    /// member in a virtual workspace and the error cases around it. Here:
    /// with no `specs` every member gets `cli_features`; with `specs`, the
    /// members matching one do.
    pub fn members_with_features(
        &self,
        specs: &[PackageIdSpec],
        cli_features: &CliFeatures,
    ) -> CargoResult<Vec<(&Package, CliFeatures)>> {
        Ok(self
            .members()
            .filter(|pkg| specs.is_empty() || specs.iter().any(|s| s.matches(pkg.package_id())))
            .map(|pkg| (pkg, cli_features.clone()))
            .collect())
    }
}
