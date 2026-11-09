//! Stand-ins for Cargo's `Package` and `PackageSet`, as far as the resolver
//! looks at them.
//!
//! Cargo's `Package` is a parsed manifest with its path; its `PackageSet`
//! downloads packages on demand. The resolver only reads a package's id, its
//! dependencies and its targets, so a [`Package`] here is a [`Summary`] plus
//! a manifest path and the targets, and a [`PackageSet`] is a map of them.

use crate::util::data_structures::HashMap;
use std::path::{Path, PathBuf};

use crate::util::errors::CargoResult;
use crate::util::interning::InternedString;
use crate::workspace::manifest::Target;
use crate::workspace::{Dependency, PackageId, Summary};

/// A package: the part of Cargo's `Package` the resolver uses.
#[derive(Clone, Debug)]
pub struct Package {
    summary: Summary,
    manifest_path: PathBuf,
    targets: Vec<Target>,
}

impl Package {
    /// A package from its [`Summary`], the path of its `Cargo.toml`, and its
    /// build targets.
    pub fn new(
        summary: Summary,
        manifest_path: impl Into<PathBuf>,
        targets: Vec<Target>,
    ) -> Package {
        Package {
            summary,
            manifest_path: manifest_path.into(),
            targets,
        }
    }

    /// Gets the summary this package was built from.
    pub fn summary(&self) -> &Summary {
        &self.summary
    }

    /// Gets the package's dependencies.
    pub fn dependencies(&self) -> &[Dependency] {
        self.summary.dependencies()
    }

    /// Gets the path to the manifest.
    pub fn manifest_path(&self) -> &Path {
        &self.manifest_path
    }

    /// Gets the name of the package.
    pub fn name(&self) -> InternedString {
        self.package_id().name()
    }

    /// Gets the `PackageId` object for this package (fully defines a package).
    pub fn package_id(&self) -> PackageId {
        self.summary.package_id()
    }

    /// Gets the root folder of the package.
    pub fn root(&self) -> &Path {
        self.manifest_path().parent().unwrap()
    }

    /// Gets the targets specified in the manifest.
    pub fn targets(&self) -> &[Target] {
        &self.targets
    }

    /// Gets the library target, if any.
    pub fn library(&self) -> Option<&Target> {
        self.targets().iter().find(|t| t.is_lib())
    }

    /// Returns `true` if any of the package's targets is a proc-macro.
    pub fn proc_macro(&self) -> bool {
        self.targets().iter().any(|target| target.proc_macro())
    }
}

/// A set of packages, looked up by id: the part of Cargo's `PackageSet` the
/// feature resolver uses.
#[derive(Debug)]
pub struct PackageSet<'gctx> {
    packages: HashMap<PackageId, Package>,
    _marker: std::marker::PhantomData<&'gctx ()>,
}

impl<'gctx> PackageSet<'gctx> {
    /// A set holding `packages`.
    pub fn new(packages: impl IntoIterator<Item = Package>) -> PackageSet<'gctx> {
        PackageSet {
            packages: packages.into_iter().map(|p| (p.package_id(), p)).collect(),
            _marker: std::marker::PhantomData,
        }
    }

    /// Adds a package to the set, replacing any with the same id.
    pub fn insert(&mut self, package: Package) {
        self.packages.insert(package.package_id(), package);
    }

    pub fn package_ids(&self) -> impl Iterator<Item = PackageId> + '_ {
        self.packages.keys().cloned()
    }

    pub fn packages(&self) -> impl Iterator<Item = &Package> {
        self.packages.values()
    }

    pub fn get_one(&self, id: PackageId) -> CargoResult<&Package> {
        self.packages
            .get(&id)
            .ok_or_else(|| anyhow::format_err!("package `{id}` is not in the package set"))
    }
}
