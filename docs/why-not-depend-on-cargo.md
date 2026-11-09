# Why not depend on the `cargo` crate?

Cargo publishes its library as the `cargo` crate, so the obvious question is
why this repository does not just depend on it.

## What the Cargo team says

The Cargo team publishes the crate because the binary is built from it, and
the crate's own description says what to expect:

> This crate is maintained by the Cargo team, primarily for use by Cargo and
> not intended for external use (except as a transitive dependency). This
> crate may make major changes to its APIs or be deprecated without warning.

That is different from the crates the team does mean to be reused, which
have their own release cadence and semver guarantees: `cargo-util-schemas`
(the manifest, lock file and index formats), `cargo-platform` (`cfg()`
expressions), `cargo-util`, `cargo-util-terminal` and `cargo-credential`.
Those are used here as published and nothing is copied from them. The
extracted crates follow the same pattern one layer up and carry the same
disclaimer.

## The focused parts of Cargo

Inside the `cargo` crate, several modules are self-contained bodies of logic
with a clear input and output, and they are the parts other tools want:

| Module | What it decides |
|---|---|
| `resolver` | which versions a set of requirements resolves to, and which features are active for which package on which platform; the lock file encoding |
| `workspace::parser` | how a `Cargo.toml` is read and normalized: inheritance, target discovery, dependency normalization, editions and `cargo-features` |
| `workspace` and `sources` | how a workspace is loaded, how an index entry, a git repository or a path becomes a package, and `cargo metadata` on top of that |
| `sources::registry` | reading a registry index, the on-disk cache, verifying and unpacking a `.crate` |
| `sources::git` | Cargo's layout under `<cargo_home>/git` and how it fetches |

Each of these is what a tool that wants to agree with Cargo has to agree
with. Reimplementing any of them drifts, and the drift shows up as builds
Cargo accepts and the tool does not. These are the candidate crates;
`roadmap.md` has the order.

## What linking all of Cargo costs

The modules are not separable through the `cargo` crate, because the crate
serves the binary:

- The dependency set is the binary's. libcurl, OpenSSL, libgit2, gitoxide
  and a bundled SQLite come along whichever module is used. For the resolver
  or the parser they are the entire cost of the build, and they rule out
  targets such as `wasm32-wasip1`.
- Everything reaches configuration through `GlobalContext`, which discovers
  its inputs from `config.toml` files up the directory tree, from Cargo's
  home and from the `CARGO_*` environment. A caller can point it at an empty
  home, but the inputs stay implicit.
- Some entry points need a toolchain. `FeatureResolver::resolve` takes a
  `RustcTargetData` whose only constructor runs `rustc`, with private
  fields, so there is no way to supply the same information from elsewhere.
- Using the crate for the parts that fit and keeping the rest elsewhere does
  not work either, since the modules share types (`SourceId`, `Summary`,
  `Dependency`, `PackageId`) and two copies cannot be mixed.

None of this is a defect; it is the crate doing its job for the binary. It
is also why this repository exists: the modules above, moved out as they
are, with a `GlobalContext` the application fills and stand-ins for the few
Cargo types they hold, and nothing changed in what they decide. Any of the
following would make the corresponding crate here unnecessary: Cargo
publishing the module with a stability promise, a way to construct
`RustcTargetData` without `rustc`, or an explicit context in place of a
discovered one. The patch series shows, per crate, what standing alone took,
in case that is useful to the Cargo team.
