"""Tooling that keeps this repository in sync with Cargo's resolver.

The model is simple: Cargo's sources are checked out at a chosen commit, the
patch series in ``patches/`` is replayed on top with ``git am -3``, and the
resulting crate directories are copied into this repository together with a
generated workspace manifest and a pin file recording what was synced.

Modules:

- :mod:`common`: paths, exit codes and the error type shared by everything.
- :mod:`shell`: thin wrappers around ``subprocess`` and ``git``.
- :mod:`config`: reading ``sync/config.toml`` and ``upstream.toml``, TOML output.
- :mod:`channels`: mapping ``nightly``/``stable``/versions to Cargo commits.
- :mod:`upstream`: the local clone of rust-lang/cargo and the patch series.
- :mod:`generate`: producing the generated tree and comparing it to the repo.
- :mod:`report`: drift reports and pull request bodies.
- :mod:`cli`: the subcommands behind ``sync/sync.py``.

Import paths are rooted at the repository, e.g. ``sync.resolver_sync.common``.
"""
