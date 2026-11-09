//! Running the pinned Cargo binary.

use std::path::Path;
use std::process::Command;
use std::sync::OnceLock;

use anyhow::{Context as _, bail};

use crate::compare::Outcome;

/// The toolchain whose `cargo` is the oracle.
#[derive(Debug)]
pub struct Toolchain {
    /// A `rustup` toolchain name, e.g. `nightly-2026-10-14`.
    pub name: String,
    /// Output of `cargo --version`.
    pub cargo_version: String,
}

/// The upstream commit and channel date recorded in `upstream.toml`.
fn pin() -> anyhow::Result<(String, Option<String>)> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../upstream.toml");
    let text =
        std::fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
    let pin: toml::Table = text.parse()?;
    let commit = pin["commit"]
        .as_str()
        .context("upstream.toml: commit")?
        .to_string();
    let channel = pin.get("channel").and_then(|v| v.as_str()).unwrap_or("");
    let date = pin
        .get("channel_date")
        .and_then(|v| v.as_str())
        .map(String::from);
    let dated = match (channel, date) {
        ("nightly" | "beta", Some(d)) => Some(format!("{channel}-{d}")),
        ("stable", _) => pin
            .get("rust_version")
            .and_then(|v| v.as_str())
            .and_then(|v| v.split_whitespace().next())
            .map(String::from),
        _ => None,
    };
    Ok((commit, dated))
}

/// Finds a toolchain whose `cargo` was built from the pinned upstream commit.
///
/// Candidates, in order: `$CONFORMANCE_TOOLCHAIN`, the dated channel from
/// `upstream.toml` (e.g. `nightly-2026-10-14`), then `nightly`, `beta`,
/// `stable`. The first whose `cargo --version` names the pinned commit wins.
/// Set `CONFORMANCE_ALLOW_MISMATCH=1` to accept the first candidate that runs.
pub fn toolchain() -> anyhow::Result<&'static Toolchain> {
    static TC: OnceLock<Result<Toolchain, String>> = OnceLock::new();
    match TC.get_or_init(|| detect().map_err(|e| format!("{e:#}"))) {
        Ok(tc) => Ok(tc),
        Err(e) => bail!("{e}"),
    }
}

// The harness is configured from the environment, as Cargo's test support is.
#[allow(clippy::disallowed_methods)]
fn detect() -> anyhow::Result<Toolchain> {
    let (commit, dated) = pin()?;
    let short = &commit[..9];
    let mut candidates: Vec<String> = Vec::new();
    if let Ok(tc) = std::env::var("CONFORMANCE_TOOLCHAIN") {
        candidates.push(tc);
    }
    candidates.extend(dated.clone());
    candidates.extend(["nightly", "beta", "stable"].map(String::from));
    let allow_mismatch = std::env::var_os("CONFORMANCE_ALLOW_MISMATCH").is_some();
    let mut tried = Vec::new();
    for name in candidates {
        let out = Command::new("cargo")
            .arg(format!("+{name}"))
            .arg("--version")
            .output();
        let Ok(out) = out else { continue };
        if !out.status.success() {
            tried.push(format!("{name}: not installed"));
            continue;
        }
        let cargo_version = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if cargo_version.contains(short) || allow_mismatch {
            return Ok(Toolchain {
                name,
                cargo_version,
            });
        }
        tried.push(format!("{name}: {cargo_version}"));
    }
    bail!(
        "no installed toolchain has cargo built from the pinned commit {short}.\n\
         Tried: {}\n\
         Install it with `rustup toolchain install {} --profile minimal`, \
         point CONFORMANCE_TOOLCHAIN at one, or set CONFORMANCE_ALLOW_MISMATCH=1.",
        tried.join("; "),
        dated.as_deref().unwrap_or("<channel>")
    )
}

fn cargo(tc: &Toolchain, home: &Path) -> Command {
    let mut cmd = Command::new("cargo");
    cmd.arg(format!("+{}", tc.name));
    cmd.env("CARGO_HOME", home);
    cmd.env_remove("RUSTFLAGS");
    cmd.env("CARGO_NET_RETRY", "0");
    cmd.env("CARGO_TERM_COLOR", "never");
    cmd
}

/// Writes a `config.toml` in `home` that replaces crates.io with the local index.
fn write_config(home: &Path, index_url: &str) -> anyhow::Result<()> {
    std::fs::create_dir_all(home)?;
    let config = format!(
        "[source.crates-io]\nreplace-with = \"conformance\"\n\n\
         [source.conformance]\nregistry = \"sparse+{index_url}\"\n"
    );
    std::fs::write(home.join("config.toml"), config)?;
    Ok(())
}

/// `cargo generate-lockfile` in `ws`, returning the lock file or Cargo's error.
pub fn generate_lockfile(
    tc: &Toolchain,
    ws: &Path,
    home: &Path,
    index_url: &str,
) -> anyhow::Result<Outcome> {
    write_config(home, index_url)?;
    let out = cargo(tc, home)
        .arg("generate-lockfile")
        .arg("--manifest-path")
        .arg(ws.join("Cargo.toml"))
        .output()
        .context("running cargo generate-lockfile")?;
    if !out.status.success() {
        return Ok(Outcome::Failed(
            String::from_utf8_lossy(&out.stderr).into_owned(),
        ));
    }
    let text = std::fs::read_to_string(ws.join("Cargo.lock"))?;
    Outcome::from_lockfile(&text)
}

/// `cargo metadata --no-deps` for `ws`, as JSON.
pub fn metadata(tc: &Toolchain, ws: &Path, home: &Path) -> anyhow::Result<serde_json::Value> {
    let out = cargo(tc, home)
        .args([
            "metadata",
            "--no-deps",
            "--format-version",
            "1",
            "--offline",
        ])
        .arg("--manifest-path")
        .arg(ws.join("Cargo.toml"))
        .output()
        .context("running cargo metadata")?;
    if !out.status.success() {
        bail!(
            "cargo metadata failed:\n{}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
    Ok(serde_json::from_slice(&out.stdout)?)
}

/// The toolchain's `rustc` version, as Cargo would use for MSRV fallback.
pub fn rustc_version(tc: &Toolchain) -> anyhow::Result<semver::Version> {
    let out = Command::new("rustc")
        .arg(format!("+{}", tc.name))
        .arg("--version")
        .output()
        .context("running rustc --version")?;
    let text = String::from_utf8_lossy(&out.stdout);
    let v = text
        .split_whitespace()
        .nth(1)
        .context("unexpected rustc --version output")?;
    Ok(v.parse()?)
}
