//! Resolving a case with `cargo_resolver`.
//!
//! The workspace side of the input comes from `cargo metadata --no-deps`,
//! which reports each member's normalized dependency declarations without
//! resolving anything. The registry side is read straight from the sparse
//! index files, converted to [`Summary`]s the way Cargo's registry source
//! does (see `index_package_to_summary` in Cargo's `sources/registry/index`).

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{Context as _, bail};
use cargo_platform::Platform;
use cargo_resolver::interning::InternedString;
use cargo_resolver::util::data_structures::HashMap;
use cargo_resolver::{
    Artifact, CargoResult, DepKind, Dependency, IndexSummary, IntoUrl, OptVersionReq, PackageId,
    QueryKind, Registry, ResolveOpts, ResolveVersion, SourceId, Summary, VersionOrdering,
    VersionPreferences,
};
use cargo_util_schemas::core::PartialVersion;
use cargo_util_schemas::index::{IndexPackage, RegistryDependency};
use cargo_util_schemas::manifest::RustVersion;
use serde::Deserialize;

use crate::compare::Outcome;
use crate::fixture::index_path;

/// Highest index schema version this Cargo understands (`INDEX_V_MAX`).
const INDEX_V_MAX: u32 = 2;

/// Cargo's `Dependency::matches` in default mode, written out here because
/// its signature differs between the Cargo versions the branches pin.
fn dep_matches(dep: &Dependency, summary: &Summary) -> bool {
    let id = summary.package_id();
    if dep.package_name() != id.name() || dep.source_id() != id.source_id() {
        return false;
    }
    let version = id.version();
    match dep.version_req() {
        OptVersionReq::Any => true,
        OptVersionReq::Req(req) => req.matches(version),
        OptVersionReq::Locked(v, _) => v == version,
        OptVersionReq::Precise(v, _) => {
            v.major == version.major
                && v.minor == version.minor
                && v.patch == version.patch
                && v.pre == version.pre
                && (v.build.is_empty() || v.build == version.build)
        }
    }
}

#[derive(Deserialize)]
struct Metadata {
    packages: Vec<MetaPackage>,
    workspace_root: PathBuf,
}

#[derive(Deserialize)]
struct MetaPackage {
    name: String,
    version: semver::Version,
    manifest_path: PathBuf,
    dependencies: Vec<MetaDep>,
    #[serde(default)]
    features: BTreeMap<String, Vec<String>>,
    links: Option<String>,
    rust_version: Option<String>,
    edition: String,
}

#[derive(Deserialize)]
struct MetaDep {
    name: String,
    source: Option<String>,
    req: String,
    kind: Option<String>,
    rename: Option<String>,
    #[serde(default)]
    optional: bool,
    #[serde(default = "default_true")]
    uses_default_features: bool,
    #[serde(default)]
    features: Vec<String>,
    target: Option<String>,
    registry: Option<String>,
    path: Option<PathBuf>,
}

fn default_true() -> bool {
    true
}

/// Builds the root [`Summary`] for one workspace member.
fn member_summary(p: &MetaPackage) -> anyhow::Result<Summary> {
    let dir = p
        .manifest_path
        .parent()
        .context("manifest path has no parent")?;
    let source_id = SourceId::for_path(dir)?;
    let pkg_id = PackageId::new(InternedString::new(&p.name), p.version.clone(), source_id);
    let mut deps = Vec::new();
    for d in &p.dependencies {
        let dep_source = match (&d.source, &d.path) {
            (Some(s), _) => SourceId::from_url(s)?,
            (None, Some(path)) => SourceId::for_path(path)?,
            (None, None) => bail!("dependency {} has neither source nor path", d.name),
        };
        if d.registry.is_some() {
            bail!("alternative registries are not supported by the harness yet");
        }
        let mut dep = Dependency::parse(d.name.as_str(), Some(&d.req), dep_source)?;
        if let Some(rename) = &d.rename {
            dep.set_explicit_name_in_toml(rename.as_str());
        }
        let kind = match d.kind.as_deref() {
            Some("dev") => DepKind::Development,
            Some("build") => DepKind::Build,
            _ => DepKind::Normal,
        };
        let platform = match &d.target {
            Some(t) => Some(t.parse::<Platform>()?),
            None => None,
        };
        dep.set_optional(d.optional)
            .set_default_features(d.uses_default_features)
            .set_features(d.features.iter().map(String::as_str))
            .set_platform(platform)
            .set_kind(kind);
        deps.push(dep);
    }
    let features: BTreeMap<InternedString, Vec<InternedString>> = p
        .features
        .iter()
        .map(|(k, v)| {
            (
                InternedString::new(k),
                v.iter().map(|s| InternedString::new(s)).collect(),
            )
        })
        .collect();
    let rust_version = p
        .rust_version
        .as_deref()
        .map(str::parse::<RustVersion>)
        .transpose()?;
    Ok(Summary::new(
        pkg_id,
        deps,
        &features,
        p.links.as_deref(),
        rust_version,
    )?)
}

/// Port of Cargo's `index_package_to_summary`.
fn index_package_to_summary(pkg: IndexPackage<'_>, source_id: SourceId) -> CargoResult<Summary> {
    let pkgid = PackageId::new(pkg.name.as_ref().into(), pkg.vers, source_id);
    let deps = pkg
        .deps
        .into_iter()
        .map(|dep| registry_dependency_into_dep(dep, source_id))
        .collect::<CargoResult<Vec<_>>>()?;
    let mut features = pkg.features;
    if let Some(features2) = pkg.features2 {
        for (name, values) in features2 {
            features.entry(name).or_default().extend(values);
        }
    }
    let features = features
        .into_iter()
        .map(|(name, values)| (name.into(), values.into_iter().map(|v| v.into()).collect()))
        .collect::<BTreeMap<_, _>>();
    let links: Option<InternedString> = pkg.links.as_ref().map(|l| l.as_ref().into());
    let mut summary = Summary::new(pkgid, deps, &features, links, pkg.rust_version)?;
    summary.set_checksum(pkg.cksum);
    if let Some(pubtime) = pkg.pubtime {
        summary.set_pubtime(pubtime);
    }
    Ok(summary)
}

/// Port of Cargo's `registry_dependency_into_dep`.
fn registry_dependency_into_dep(
    dep: RegistryDependency<'_>,
    default: SourceId,
) -> CargoResult<Dependency> {
    let RegistryDependency {
        name,
        req,
        mut features,
        optional,
        default_features,
        target,
        kind,
        registry,
        package,
        public,
        artifact,
        bindep_target,
        lib,
    } = dep;

    let id = if let Some(registry) = &registry {
        SourceId::for_registry(&registry.into_url()?)?
    } else {
        default
    };

    let interned_name = InternedString::new(package.as_ref().unwrap_or(&name));
    let mut dep = Dependency::parse(interned_name, Some(&req), id)?;
    if package.is_some() {
        dep.set_explicit_name_in_toml(name);
    }
    let kind = match kind.as_deref().unwrap_or("") {
        "dev" => DepKind::Development,
        "build" => DepKind::Build,
        _ => DepKind::Normal,
    };

    let platform = match target {
        Some(target) => Some(target.parse()?),
        None => None,
    };

    let public = public.unwrap_or(false);

    features.retain(|s| !s.is_empty());

    if !id.is_crates_io() {
        dep.set_registry_id(id);
    }

    if let Some(artifacts) = artifact {
        let artifact = Artifact::parse(&artifacts, lib, bindep_target.as_deref(), false)?;
        dep.set_artifact(artifact);
    }

    dep.set_optional(optional)
        .set_default_features(default_features)
        .set_features(features)
        .set_platform(platform)
        .set_kind(kind)
        .set_public(public);

    Ok(dep)
}

/// A [`Registry`] over the sparse index directory plus the workspace members.
struct IndexRegistry {
    index_dir: PathBuf,
    crates_io: SourceId,
    members: Vec<Summary>,
    cache: RefCell<HashMap<String, Vec<IndexSummary>>>,
}

impl IndexRegistry {
    fn load(&self, name: &str) -> CargoResult<Vec<IndexSummary>> {
        if let Some(v) = self.cache.borrow().get(name) {
            return Ok(v.clone());
        }
        let path = self.index_dir.join(index_path(name));
        let mut out = Vec::new();
        if let Ok(text) = std::fs::read_to_string(&path) {
            for line in text.lines().filter(|l| !l.trim().is_empty()) {
                let pkg: IndexPackage<'_> = serde_json::from_str(line)
                    .with_context(|| format!("parsing index line for {name}: {line}"))?;
                let v = pkg.v.unwrap_or(1);
                let yanked = pkg.yanked.unwrap_or(false);
                let summary = index_package_to_summary(pkg, self.crates_io)?;
                out.push(if v > INDEX_V_MAX {
                    IndexSummary::Unsupported(summary, v)
                } else if yanked {
                    IndexSummary::Yanked(summary)
                } else {
                    IndexSummary::Candidate(summary)
                });
            }
        }
        self.cache
            .borrow_mut()
            .insert(name.to_string(), out.clone());
        Ok(out)
    }
}

impl Registry for IndexRegistry {
    async fn query(
        &self,
        dep: &Dependency,
        kind: QueryKind,
        f: &mut dyn FnMut(IndexSummary),
    ) -> CargoResult<()> {
        let source = dep.source_id();
        if source.is_path() {
            for m in &self.members {
                if m.source_id() == source && dep_matches(dep, m) {
                    f(IndexSummary::Candidate(m.clone()));
                }
            }
            return Ok(());
        }
        if source != self.crates_io {
            bail!("the harness only serves crates.io; got {source}");
        }
        for s in self.load(dep.package_name().as_str())? {
            let matches = match kind {
                QueryKind::Exact => dep_matches(dep, s.as_summary_unchecked()),
                QueryKind::RejectedVersions => true,
                _ => false,
            };
            if matches {
                f(s);
            }
        }
        Ok(())
    }

    fn describe_source(&self, source: SourceId) -> String {
        format!("{source}")
    }

    fn is_replaced(&self, _source: SourceId) -> bool {
        false
    }
}

/// The workspace's resolver version (1, 2 or 3), as Cargo would determine it.
fn resolver_version(meta: &Metadata) -> anyhow::Result<u8> {
    let root_path = meta.workspace_root.join("Cargo.toml");
    let root: toml::Table = std::fs::read_to_string(&root_path)?.parse()?;
    let from_key = |t: Option<&toml::Value>| -> Option<u8> {
        t.and_then(|t| t.get("resolver"))
            .and_then(|v| v.as_str())
            .and_then(|s| s.parse().ok())
    };
    if let Some(v) = from_key(root.get("workspace")) {
        return Ok(v);
    }
    if let Some(pkg) = root.get("package") {
        if let Some(v) = from_key(Some(pkg)) {
            return Ok(v);
        }
        let root_pkg = meta
            .packages
            .iter()
            .find(|p| p.manifest_path == root_path)
            .context("root package not in metadata")?;
        return Ok(edition_default_resolver(&root_pkg.edition));
    }
    Ok(1)
}

fn edition_default_resolver(edition: &str) -> u8 {
    match edition {
        "2015" | "2018" => 1,
        "2021" => 2,
        _ => 3,
    }
}

/// Resolves the workspace described by `metadata` against `index_dir`.
pub fn resolve(
    metadata: &serde_json::Value,
    index_dir: &Path,
    rustc: &semver::Version,
    options: crate::Options,
) -> anyhow::Result<Outcome> {
    let meta: Metadata = serde_json::from_value(metadata.clone())?;
    let members = meta
        .packages
        .iter()
        .map(member_summary)
        .collect::<anyhow::Result<Vec<_>>>()?;

    let mut version_prefs = VersionPreferences::default();
    if options.sabotage_minimal_versions {
        version_prefs.version_ordering(VersionOrdering::MinimumVersionsFirst);
    }
    if resolver_version(&meta)? == 3 {
        let mut rust_versions: Vec<PartialVersion> = members
            .iter()
            .filter_map(|m| m.rust_version().map(|rv| rv.to_partial()))
            .collect();
        if rust_versions.is_empty() {
            rust_versions.push(rustc.clone().into());
        }
        version_prefs.rust_versions(rust_versions);
    }
    let lowest_rust_version = members.iter().filter_map(|m| m.rust_version()).min();
    let resolve_version = ResolveVersion::with_rust_version(lowest_rust_version);

    let registry = IndexRegistry {
        index_dir: index_dir.to_path_buf(),
        crates_io: SourceId::for_registry(&cargo_resolver::CRATES_IO_INDEX.into_url()?)?,
        members: members.clone(),
        cache: RefCell::new(HashMap::default()),
    };
    let summaries: Vec<(Summary, ResolveOpts)> = members
        .into_iter()
        .map(|m| (m, ResolveOpts::everything()))
        .collect();

    let gctx = cargo_resolver::GlobalContext::default()?;
    match cargo_resolver::resolve(
        &summaries,
        &[],
        &registry,
        &version_prefs,
        resolve_version,
        &gctx,
    ) {
        Ok(resolve) => {
            let text = toml::to_string(&resolve)?;
            Outcome::from_lockfile(&text)
        }
        Err(e) => Ok(Outcome::Failed(format!("{e:?}"))),
    }
}
