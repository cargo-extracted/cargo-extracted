//! Differential tests for the extracted resolver.
//!
//! Each [`Case`] describes a registry (as sparse-index entries) and a
//! workspace. The case is resolved twice:
//!
//! 1. by the Cargo binary that corresponds to the upstream commit this
//!    repository is pinned to (`cargo generate-lockfile`), reading the index
//!    over HTTP from a local server; and
//! 2. by `cargo_resolver`, reading the same index files directly, with the
//!    workspace packages described by `cargo metadata --no-deps`.
//!
//! The two lock files are then compared structurally. Any difference means
//! the extracted code does not give Cargo's answer for that input.

#![allow(clippy::print_stderr)] // a test harness talks to the person running it

pub mod compare;
pub mod fixture;
pub mod http;
pub mod oracle;
pub mod subject;

pub use compare::{Outcome, Report};
pub use fixture::{Case, Dep, Pkg};

use std::path::Path;

/// Knobs for the subject side, used to prove the comparison has teeth.
#[derive(Debug, Default, Clone, Copy)]
pub struct Options {
    /// Resolve with minimum versions first, which Cargo does not do by
    /// default. A case with more than one candidate version must then differ.
    pub sabotage_minimal_versions: bool,
}

/// Runs a case end to end and returns the comparison.
pub fn run(case: &Case) -> anyhow::Result<Report> {
    run_with(case, Options::default())
}

/// [`run`] with explicit [`Options`].
// The harness is configured from the environment, as Cargo's test support is.
#[allow(clippy::disallowed_methods)]
pub fn run_with(case: &Case, options: Options) -> anyhow::Result<Report> {
    let tmp = tempfile::Builder::new()
        .prefix(&format!("conformance-{}-", case.name))
        .tempdir()?;
    let root = if std::env::var_os("CONFORMANCE_KEEP").is_some() {
        let p = tmp.keep();
        eprintln!("keeping {}", p.display());
        p
    } else {
        tmp.path().to_path_buf()
    };
    run_in(case, &root, options)
}

fn run_in(case: &Case, root: &Path, options: Options) -> anyhow::Result<Report> {
    let index_dir = root.join("index");
    let ws_dir = root.join("ws");
    let home = root.join("home");
    std::fs::create_dir_all(&home)?;
    case.write_index(&index_dir)?;
    case.write_workspace(&ws_dir)?;

    let server = http::Server::start(index_dir.clone())?;
    fixture::write_index_config(&index_dir, &server.base_url)?;

    let toolchain = oracle::toolchain()?;
    let oracle = oracle::generate_lockfile(toolchain, &ws_dir, &home, &server.base_url)?;
    let subject = match oracle::metadata(toolchain, &ws_dir, &home) {
        Ok(metadata) => {
            let rustc = oracle::rustc_version(toolchain)?;
            subject::resolve(&metadata, &index_dir, &rustc, options)?
        }
        // The manifests themselves were rejected; there is nothing to resolve
        // on either side, and the oracle will have failed for the same reason.
        Err(e) if matches!(oracle, Outcome::Failed(_)) => Outcome::Failed(format!("{e:#}")),
        Err(e) => return Err(e),
    };

    Ok(compare::compare(&case.name, &oracle, &subject))
}
