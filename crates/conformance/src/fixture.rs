//! Registry and workspace fixtures.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use serde::Serialize;

/// One dependency of an index entry, in the index's own JSON shape.
#[derive(Clone, Serialize)]
pub struct Dep {
    name: String,
    req: String,
    features: Vec<String>,
    optional: bool,
    default_features: bool,
    target: Option<String>,
    kind: Option<String>,
    registry: Option<String>,
    package: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    public: Option<bool>,
}

impl Dep {
    pub fn new(name: &str, req: &str) -> Dep {
        Dep {
            name: name.into(),
            req: req.into(),
            features: Vec::new(),
            optional: false,
            default_features: true,
            target: None,
            kind: None,
            registry: None,
            package: None,
            public: None,
        }
    }
    pub fn optional(mut self) -> Dep {
        self.optional = true;
        self
    }
    pub fn no_default_features(mut self) -> Dep {
        self.default_features = false;
        self
    }
    pub fn features(mut self, f: &[&str]) -> Dep {
        self.features = f.iter().map(|s| s.to_string()).collect();
        self
    }
    pub fn target(mut self, t: &str) -> Dep {
        self.target = Some(t.into());
        self
    }
    pub fn dev(mut self) -> Dep {
        self.kind = Some("dev".into());
        self
    }
    pub fn build(mut self) -> Dep {
        self.kind = Some("build".into());
        self
    }
    /// The index's `package` field: `name` is the alias, `package` the real crate.
    pub fn package(mut self, real: &str) -> Dep {
        self.package = Some(real.into());
        self
    }
}

/// One index entry: a published version of a crate.
#[derive(Clone, Serialize)]
pub struct Pkg {
    name: String,
    vers: String,
    deps: Vec<Dep>,
    cksum: String,
    features: BTreeMap<String, Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    features2: Option<BTreeMap<String, Vec<String>>>,
    yanked: bool,
    links: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    rust_version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    v: Option<u32>,
}

impl Pkg {
    pub fn new(name: &str, vers: &str) -> Pkg {
        Pkg {
            name: name.into(),
            vers: vers.into(),
            deps: Vec::new(),
            cksum: fake_checksum(name, vers),
            features: BTreeMap::new(),
            features2: None,
            yanked: false,
            links: None,
            rust_version: None,
            v: None,
        }
    }
    pub fn dep(mut self, dep: Dep) -> Pkg {
        self.deps.push(dep);
        self
    }
    /// Plain feature in the `features` table (index `v` 1).
    pub fn feature(mut self, name: &str, enables: &[&str]) -> Pkg {
        self.features
            .insert(name.into(), enables.iter().map(|s| s.to_string()).collect());
        self
    }
    /// Feature placed in `features2`, as crates.io does for `dep:`/`?` syntax; marks the entry `v = 2`.
    pub fn feature2(mut self, name: &str, enables: &[&str]) -> Pkg {
        self.features2
            .get_or_insert_with(BTreeMap::new)
            .insert(name.into(), enables.iter().map(|s| s.to_string()).collect());
        self.v = Some(self.v.unwrap_or(1).max(2));
        self
    }
    pub fn yanked(mut self) -> Pkg {
        self.yanked = true;
        self
    }
    pub fn links(mut self, l: &str) -> Pkg {
        self.links = Some(l.into());
        self
    }
    pub fn rust_version(mut self, rv: &str) -> Pkg {
        self.rust_version = Some(rv.into());
        self
    }
    /// Force the index schema version, e.g. 3 for an entry this Cargo must skip.
    pub fn schema_version(mut self, v: u32) -> Pkg {
        self.v = Some(v);
        self
    }
    pub fn name(&self) -> &str {
        &self.name
    }
}

/// A deterministic 64-hex-digit string; Cargo only records it in the lock file.
fn fake_checksum(name: &str, vers: &str) -> String {
    use std::hash::{Hash, Hasher};
    let mut out = String::new();
    for seed in 0u64..4 {
        let mut h = std::collections::hash_map::DefaultHasher::new();
        (seed, name, vers).hash(&mut h);
        out.push_str(&format!("{:016x}", h.finish()));
    }
    out
}

/// A workspace member: a directory with a manifest (and generated sources).
pub struct Member {
    pub dir: String,
    pub manifest: String,
}

/// A differential test case.
pub struct Case {
    pub name: String,
    pub index: Vec<Pkg>,
    /// A root `Cargo.toml` for a virtual workspace, if the case has one.
    pub root_manifest: Option<String>,
    pub members: Vec<Member>,
}

impl Case {
    pub fn new(name: &str) -> Case {
        Case {
            name: name.into(),
            index: Vec::new(),
            root_manifest: None,
            members: Vec::new(),
        }
    }
    pub fn pkg(mut self, pkg: Pkg) -> Case {
        self.index.push(pkg);
        self
    }
    /// A single-package workspace living at the root.
    pub fn package(mut self, manifest: &str) -> Case {
        self.members.push(Member {
            dir: ".".into(),
            manifest: manifest.into(),
        });
        self
    }
    pub fn virtual_root(mut self, manifest: &str) -> Case {
        self.root_manifest = Some(manifest.into());
        self
    }
    pub fn member(mut self, dir: &str, manifest: &str) -> Case {
        self.members.push(Member {
            dir: dir.into(),
            manifest: manifest.into(),
        });
        self
    }

    /// Writes the sparse-index layout (`1/a`, `2/ab`, `3/a/abc`, `ab/cd/name`).
    pub fn write_index(&self, dir: &Path) -> anyhow::Result<()> {
        let mut by_name: BTreeMap<&str, Vec<&Pkg>> = BTreeMap::new();
        for p in &self.index {
            by_name.entry(&p.name).or_default().push(p);
        }
        for (name, pkgs) in by_name {
            let path = dir.join(index_path(name));
            fs::create_dir_all(path.parent().unwrap())?;
            let mut body = String::new();
            for p in pkgs {
                body.push_str(&serde_json::to_string(p)?);
                body.push('\n');
            }
            fs::write(path, body)?;
        }
        Ok(())
    }

    pub fn write_workspace(&self, dir: &Path) -> anyhow::Result<()> {
        fs::create_dir_all(dir)?;
        if let Some(root) = &self.root_manifest {
            fs::write(dir.join("Cargo.toml"), root)?;
        }
        for m in &self.members {
            let d = dir.join(&m.dir);
            fs::create_dir_all(d.join("src"))?;
            fs::write(d.join("Cargo.toml"), &m.manifest)?;
            fs::write(d.join("src/lib.rs"), "")?;
            if m.manifest.contains("links") {
                fs::write(d.join("build.rs"), "fn main() {}\n")?;
            }
        }
        Ok(())
    }
}

/// Relative path of a crate's index file.
pub fn index_path(name: &str) -> String {
    let lower = name.to_lowercase();
    match lower.len() {
        1 => format!("1/{lower}"),
        2 => format!("2/{lower}"),
        3 => format!("3/{}/{lower}", &lower[..1]),
        _ => format!("{}/{}/{lower}", &lower[..2], &lower[2..4]),
    }
}

/// `config.json` at the index root, required by the sparse protocol.
pub fn write_index_config(dir: &Path, base_url: &str) -> anyhow::Result<()> {
    let config =
        format!(r#"{{"dl":"{base_url}dl/{{crate}}/{{version}}/download","api":"{base_url}"}}"#);
    fs::write(dir.join("config.json"), config)?;
    Ok(())
}
