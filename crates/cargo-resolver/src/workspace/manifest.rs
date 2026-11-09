//! Stand-in for Cargo's `Target`, as far as the resolver looks at one.
//!
//! The feature resolver asks whether a package's library is a proc-macro, and
//! the lock file code derives an `extern crate` name from a target's name.
//! Cargo's `Target` carries much more (source path, edition, crate types,
//! which commands build it); none of that reaches the resolver.

use crate::util::interning::InternedString;

/// A build target of a package: the part of Cargo's `Target` the resolver uses.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Target {
    name: InternedString,
    is_lib: bool,
    proc_macro: bool,
}

impl Target {
    /// A library target, `proc_macro` if it is a `proc-macro = true` one.
    pub fn lib(name: impl Into<InternedString>, proc_macro: bool) -> Target {
        Target {
            name: name.into(),
            is_lib: true,
            proc_macro,
        }
    }

    /// Any other target (a binary, example, test, bench or build script).
    ///
    /// Examples can be proc-macros too; the feature resolver counts them.
    pub fn other(name: impl Into<InternedString>, proc_macro: bool) -> Target {
        Target {
            name: name.into(),
            is_lib: false,
            proc_macro,
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn crate_name(&self) -> String {
        self.name().replace("-", "_")
    }

    pub fn is_lib(&self) -> bool {
        self.is_lib
    }

    pub fn proc_macro(&self) -> bool {
        self.proc_macro
    }
}
