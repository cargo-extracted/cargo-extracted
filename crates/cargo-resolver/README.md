# cargo-ext-resolver

Cargo's dependency resolver and feature resolver as a standalone crate: the
files of Cargo's `resolver` module and the types they operate on, moved out of
the `cargo` crate and left as Cargo wrote them.

> This crate is maintained by the Cargo team, primarily for use by Cargo and
> not intended for external use (except as a transitive dependency). This
> crate may make major changes to its APIs or be deprecated without warning.

That disclaimer is Cargo's, and it applies here just the same. The code is
Cargo's; nothing in what it decides is changed, and behavior that differs
from Cargo is a bug in this crate, not a feature. Changes to the resolver
itself belong in rust-lang/cargo and arrive here with the next sync.

The version follows the `cargo` crate: `cargo-ext-resolver 0.101.0` is the
resolver of `cargo 0.101.0`, which ships with Rust 1.100. The library is
`cargo_resolver`, after Cargo's module, and the package name carries an `ext`
marker so it is not mistaken for a crate the Cargo team publishes. Alias it,
so code reads as it does upstream:

```toml
[dependencies]
cargo-resolver = { package = "cargo-ext-resolver", version = "0.101.0" }
```

The entry point is `resolve`, as in Cargo: the `Summary` of each root package
with its `ResolveOpts`, a `Registry` to query candidates from,
`VersionPreferences`, and a `GlobalContext`. `FeatureResolver` is the second
pass, and `encode` converts a `Resolve` to and from `Cargo.lock`.
`GlobalContext` stands in for Cargo's, with Cargo's method names: the
configuration Cargo would have read, as a TOML table you fill, and a `Shell`
to print to. Where the moved code holds a `Workspace`, `Package`,
`PackageSet`, `Target` or `RustcTargetData`, stand-ins with the methods it
calls take their place, built from what you know. The crate performs no I/O.

How the crate is produced, and the rest of the series it belongs to, is
described at https://github.com/cargo-extracted/cargo-extracted.

License: MIT OR Apache-2.0, Cargo's.
