# cargo-extracted

Cargo's dependency resolver and feature resolver as a standalone crate,
`cargo-ext-resolver`.

Cargo does not publish the resolver on its own. It lives inside the `cargo`
library crate with everything else the binary needs, and the Cargo team is
explicit that the crate is not for external use. This repository moves the
files that make up the resolver into a crate of its own, changes as little
as possible to make them compile apart from Cargo, and tracks upstream
commit for commit. [`docs/why-not-depend-on-cargo.md`](docs/why-not-depend-on-cargo.md)
has the reasoning.

This is not a fork. The only changes made here are the ones needed to lift a
module out of the `cargo` crate and have it compile on its own: moving files,
deleting what cannot exist outside Cargo, making things `pub`, and adding the
glue around them. No feature, fix or change in behavior is made to Cargo's
code, and none will be accepted; those belong in rust-lang/cargo, and they
arrive here with the next sync. Behavior that differs from Cargo is a bug
here.

## The crate

`cargo-ext-resolver` is the dependency resolver and the feature resolver,
with the types they operate on (`SourceId`, `PackageId`, `Summary`,
`Dependency`, `Resolve`) and the lock file encoding. It performs no I/O. The
library is `cargo_resolver`, after Cargo's
module, and the package name carries an `ext` marker so it is not mistaken
for a crate the Cargo team publishes. Alias it, so code reads as it does
upstream:

```toml
[dependencies]
cargo-resolver = { package = "cargo-ext-resolver", version = "0.100.0" }
```

The crate reaches your application through a `GlobalContext` that stands in
for Cargo's, with Cargo's method names: what Cargo reads from configuration,
as a TOML table you fill, and a `Shell` to print to. Where the moved code
holds a `Workspace`, `Package`, `PackageSet`, `Target` or `RustcTargetData`,
stand-ins with the methods it calls take their place, built from what you
know. The sibling crates the Cargo team publishes (`cargo-platform`,
`cargo-util-schemas`) are used as published.

The manifest parser and a workspace crate, enough for `cargo metadata`, are
next, in the same shape; [`docs/roadmap.md`](docs/roadmap.md) has the plan.

## How it is built

Nothing here is maintained by hand except the patch series in `patches/`.
A sync checks out a commit of rust-lang/cargo, replays the series with
`git am -3`, re-exports it rebased onto that commit, and copies the crate
out with a generated workspace manifest and `upstream.toml`, which records
the commit. CI regenerates the tree on every push and pull request and fails
on any difference, so an edit to a generated file cannot land. Inside
Cargo's files the series only moves, marks the few items that cannot exist
outside Cargo with `#[cfg(any())]`, and makes things `pub`; everything else
is a new file. That is what keeps a sync cheap when upstream moves. [CONTRIBUTING.md](CONTRIBUTING.md) has the rules and the
commands.

The branches mirror Cargo's. `main` tracks Cargo's `master` through the
nightlies, a day behind, from the dated release manifest so every sync
names a commit a published toolchain was built with. When a Rust release
ships, `rust-1.XX.0` is cut from `main`, with the patch series as it was
just before Cargo's branch point, and synced to the release; point releases
land on the same branch. The sync is run from
the Actions tab for now. The crate's version
is that of the upstream `cargo` crate: `0.102.0` is the code that ships with
Rust 1.101. Nightly syncs carry a `-nightly` suffix and are never
published.

## License

Cargo's license: MIT OR Apache-2.0. See [LICENSE](LICENSE).
