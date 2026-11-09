//! Stand-in for Cargo's `RustcTargetData`.
//!
//! Cargo learns the `cfg` values of the host and of each `--target` by
//! running `rustc --print cfg`, and the feature resolver asks it whether a
//! dependency's `[target.'cfg(..)']` platform applies to a compile kind. Here
//! the embedding application supplies the same information, and the question
//! is answered the way Cargo answers it.

use crate::util::data_structures::HashMap;

use cargo_platform::Cfg;

use crate::compiler::{CompileKind, CompileTarget};
use crate::util::errors::CargoResult;
use crate::util::interning::InternedString;
use crate::workspace::Dependency;

/// What the feature resolver reads of Cargo's `Rustc`: the host triple.
#[derive(Clone, Debug)]
pub struct Rustc {
    /// The host triple, what `rustc -vV` reports as `host:`.
    pub host: InternedString,
}

/// The `cfg` values of the host and of each compile target.
pub struct RustcTargetData<'gctx> {
    /// Information about `rustc` itself.
    pub rustc: Rustc,
    host_cfg: Vec<Cfg>,
    target_cfg: HashMap<CompileTarget, Vec<Cfg>>,
    _marker: std::marker::PhantomData<&'gctx ()>,
}

impl<'gctx> RustcTargetData<'gctx> {
    /// Target data for a host with the given triple and `cfg` values.
    ///
    /// `host_cfg` is what `rustc --print cfg` prints without `--target`.
    pub fn new(host: impl Into<InternedString>, host_cfg: Vec<Cfg>) -> RustcTargetData<'gctx> {
        RustcTargetData {
            rustc: Rustc { host: host.into() },
            host_cfg,
            target_cfg: HashMap::default(),
            _marker: std::marker::PhantomData,
        }
    }

    /// Adds a compile target with its `cfg` values, what
    /// `rustc --print cfg --target <target>` prints.
    pub fn add_target(&mut self, target: CompileTarget, cfg: Vec<Cfg>) {
        self.target_cfg.insert(target, cfg);
    }

    /// Cargo asks `rustc` about a target the first time an artifact
    /// dependency names it. Here a target not added with
    /// [`RustcTargetData::add_target`] is known by its name alone, with no
    /// `cfg` values.
    pub fn merge_compile_kind(&mut self, kind: CompileKind) -> CargoResult<()> {
        if let CompileKind::Target(target) = kind {
            self.target_cfg.entry(target).or_default();
        }
        Ok(())
    }

    /// Gets the short name of the compile kind, like `x86_64-unknown-linux-gnu`.
    pub fn short_name<'a>(&'a self, kind: &'a CompileKind) -> &'a str {
        match kind {
            CompileKind::Host => &self.rustc.host,
            CompileKind::Target(target) => target.short_name(),
        }
    }

    /// Gets the list of `cfg`s printed out from the compiler for the specified kind.
    pub fn cfg(&self, kind: CompileKind) -> &[Cfg] {
        match kind {
            CompileKind::Host => &self.host_cfg,
            CompileKind::Target(target) => self.target_cfg.get(&target).map_or(&[], |c| c),
        }
    }

    /// Whether a dependency's `[target.'cfg(..)']` platform (if any) applies to
    /// `kind`, exactly as Cargo decides it.
    pub fn dep_platform_activated(&self, dep: &Dependency, kind: CompileKind) -> bool {
        // If this dependency is only available for certain platforms,
        // make sure we're only enabling it for that platform.
        let Some(platform) = dep.platform() else {
            return true;
        };
        let name = self.short_name(&kind);
        platform.matches(name, self.cfg(kind))
    }
}
