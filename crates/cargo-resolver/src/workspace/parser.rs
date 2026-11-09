//! From Cargo's `workspace/parser/mod.rs`: the one function `SourceId` uses.

use std::ffi::OsStr;
use std::path::Path;

/// Whether a manifest path is an embedded manifest (a `.rs` file or a file
/// without extension) rather than a `Cargo.toml`.
pub fn is_embedded(path: &Path) -> bool {
    let ext = path.extension();
    ext == Some(OsStr::new("rs")) || ext.is_none()
}
