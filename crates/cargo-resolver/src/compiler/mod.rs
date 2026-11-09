//! The paths Cargo's `compiler` module provides to the moved files.
//!
//! [`CompileKind`] and [`CompileTarget`] are copied from Cargo's
//! `compiler/compile_kind.rs`, without the constructors that run `rustc` or
//! read configuration. [`RustcTargetData`] stands in for Cargo's, which asks
//! `rustc` for the `cfg` values of each target; here the embedding
//! application supplies them.

pub mod compile_kind;
pub mod target_info;

pub use self::compile_kind::{CompileKind, CompileTarget};
pub use self::target_info::{Rustc, RustcTargetData};
