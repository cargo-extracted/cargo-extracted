# Roadmap

The goal is to expose Cargo's feature resolver and `cargo metadata` as a
library. Measured against Cargo's tree, that is three crates, added in this
order. Each one is built the same way as the resolver: a short patch series
that moves the files, copies and stands in for what they need, and marks
what cannot exist outside Cargo with `#[cfg(any())]`, with a `GlobalContext`
the application fills.

| Crate | What it carries from Cargo | Status |
|---|---|---|
| `cargo-ext-resolver` | `resolver/*` and the types it operates on; no I/O, builds for wasm | published from here |
| `cargo-ext-parser` | `workspace/parser/*` and `workspace/manifest.rs`, through `to_real_manifest`, so it produces Cargo's `Manifest` and `Target`; reads manifest files and nothing else | next |
| `cargo-ext-workspace` | everything that touches the outside world: `Workspace`, `PackageRegistry`, `PackageSet`, path, directory, registry and git sources, source replacement, lock file I/O, `resolve_ws_with_opts` and `ops/cargo_metadata.rs` as the entry point | after the parser |

## Versions, tags and adding a crate later

Every crate published from a release branch is built from the branch's
pinned Cargo commit and carries that commit's version, so `cargo-ext-parser
0.103.0` and `cargo-ext-resolver 0.103.0` are the same Cargo, and the crates
depend on one another with exact versions.

Releases are per crate. Each published crate gets its own tag and GitHub
release, `<crate>-v<version>`, pointing at the commit it was published from,
and the publish workflow decides what to publish by asking crates.io per
crate rather than by looking for a tag. That is what lets a crate join a
version that already shipped: the branch's pin does not move, so a new
crate's patches are applied to the same Cargo commit the resolver was
published from, land on the branch as a pull request, and the crate is
published and tagged alone at the branch's current version. The resolver's
tag and release are untouched. A crate can be added this way to any release
branch that still exists, as far back as wanted, one branch at a time.
CONTRIBUTING.md has the steps.

The branch `wip/all-crates` holds an earlier cut of the parser, registry and
git layers on the 1.99.1 base. The parser there stops at the normalized
manifest; the metadata goal needs `to_real_manifest` kept, which is a smaller
patch, not a larger one. The registry and git crates there fold into the
workspace crate, since no consumer of `cargo metadata` can leave either out.

The parts of the workspace crate that will not be plain moves: `PackageSet`
downloads and the HTTP index backend go through libcurl in Cargo and go
behind the `RegistryData` trait here, with the transport supplied by the
application; `Workspace` reads `[patch]`, `[source]` replacement and
`build.target` from configuration, which comes through the TOML table in
`GlobalContext`; and `RustcTargetData::new`, which runs `rustc`, is the
stand-in the resolver already has, so `--filter-platform` works without a
toolchain.
