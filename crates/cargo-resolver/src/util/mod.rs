//! The paths Cargo's `util` module provides to the moved files.
//!
//! The files declared here are Cargo's. The re-exports follow Cargo's
//! `util/mod.rs`; `errors` and `network` are the pieces of it the moved code
//! uses, copied.

pub use self::canonical_url::CanonicalUrl;
pub use self::edit_distance::{closest, closest_msg, edit_distance};
pub use self::errors::{CargoResult, InternalError, internal};
pub use self::graph::Graph;
pub use self::hasher::StableHasher;
pub use self::hex::{hash_u64, short_hash, to_hex};
pub use self::into_url::IntoUrl;
pub use self::semver_ext::*;
pub use crate::context::GlobalContext;

pub mod canonical_url;
pub mod data_structures;
pub mod edit_distance;
pub mod errors;
pub mod graph;
mod hasher;
pub mod hex;
pub mod interning;
pub mod into_url;
mod local_poll_adapter;
pub use local_poll_adapter::LocalPollAdapter;
pub mod network;
pub mod semver_eval_ext;
pub mod semver_ext;
pub mod time_span;

pub(crate) use futures::executor::block_on;
pub(crate) use futures::executor::block_on_stream;

/// From Cargo's `util/mod.rs`.
pub fn indented_lines(text: &str) -> String {
    text.lines()
        .map(|line| {
            if line.is_empty() {
                String::from("\n")
            } else {
                format!("  {}\n", line)
            }
        })
        .collect()
}
