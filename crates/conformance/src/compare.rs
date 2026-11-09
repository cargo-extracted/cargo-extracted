//! Structural comparison of lock files.

use std::fmt::Write as _;

use cargo_util_schemas::lockfile::TomlLockfile;

/// What one side produced.
pub enum Outcome {
    Resolved(TomlLockfile),
    Failed(String),
}

impl Outcome {
    pub fn from_lockfile(text: &str) -> anyhow::Result<Outcome> {
        Ok(Outcome::Resolved(toml::from_str(text)?))
    }
}

/// One package in normalized form.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct Pkg {
    name: String,
    version: String,
    source: Option<String>,
    checksum: Option<String>,
    deps: Vec<String>,
}

fn normalize(lock: &TomlLockfile) -> (Option<u32>, Vec<Pkg>) {
    let mut pkgs: Vec<Pkg> = lock
        .package
        .iter()
        .flatten()
        .chain(lock.root.iter())
        .map(|p| Pkg {
            name: p.name.clone(),
            version: p.version.clone(),
            source: p.source.as_ref().map(|s| s.source_str().clone()),
            checksum: p.checksum.clone(),
            deps: {
                let mut d: Vec<String> = p
                    .dependencies
                    .iter()
                    .flatten()
                    .map(|d| {
                        let mut s = d.name.clone();
                        if let Some(v) = &d.version {
                            s.push(' ');
                            s.push_str(v);
                        }
                        if let Some(src) = &d.source {
                            s.push_str(" (");
                            s.push_str(src.source_str());
                            s.push(')');
                        }
                        s
                    })
                    .collect();
                d.sort();
                d
            },
        })
        .collect();
    pkgs.sort();
    (lock.version, pkgs)
}

/// The result of comparing the oracle with the subject.
pub struct Report {
    pub case: String,
    pub ok: bool,
    pub text: String,
}

impl Report {
    /// Panics with the report if the sides disagree.
    pub fn assert_ok(&self) {
        eprint!("{}", self.text);
        assert!(self.ok, "{}", self.text);
    }
}

pub fn compare(case: &str, oracle: &Outcome, subject: &Outcome) -> Report {
    let mut text = String::new();
    let ok = match (oracle, subject) {
        (Outcome::Failed(o), Outcome::Failed(s)) => {
            let _ = writeln!(
                text,
                "[{case}] both sides failed to resolve\n--- cargo:\n{o}\n--- cargo_resolver:\n{s}"
            );
            true
        }
        (Outcome::Failed(o), Outcome::Resolved(_)) => {
            let _ = writeln!(
                text,
                "[{case}] cargo failed but cargo_resolver resolved\n--- cargo:\n{o}"
            );
            false
        }
        (Outcome::Resolved(_), Outcome::Failed(s)) => {
            let _ = writeln!(
                text,
                "[{case}] cargo resolved but cargo_resolver failed\n--- cargo_resolver:\n{s}"
            );
            false
        }
        (Outcome::Resolved(o), Outcome::Resolved(s)) => {
            let (ov, op) = normalize(o);
            let (sv, sp) = normalize(s);
            let mut ok = true;
            if ov != sv {
                ok = false;
                let _ = writeln!(
                    text,
                    "[{case}] lock file version: cargo {ov:?}, cargo_resolver {sv:?}"
                );
            }
            for p in &op {
                if !sp.contains(p) {
                    ok = false;
                    let _ = writeln!(text, "[{case}] only cargo has: {p:?}");
                }
            }
            for p in &sp {
                if !op.contains(p) {
                    ok = false;
                    let _ = writeln!(text, "[{case}] only cargo_resolver has: {p:?}");
                }
            }
            if ok {
                let _ = writeln!(
                    text,
                    "[{case}] identical: {} packages, lock file v{}",
                    op.len(),
                    ov.unwrap_or(1)
                );
            }
            ok
        }
    };
    Report {
        case: case.into(),
        ok,
        text,
    }
}
