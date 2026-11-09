"""Paths, exit codes and the error type shared across the sync tooling."""

from __future__ import annotations

from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parent.parent.parent
"""Repository root (the parent of ``sync/``)."""
SYNC_DIR = ROOT / ".sync"
"""Scratch space; ignored by git."""
CLONE = SYNC_DIR / "cargo"
"""Local clone of rust-lang/cargo."""
WORK = SYNC_DIR / "work"
"""Worktree of the upstream commit with the patch series applied."""
VERIFY = SYNC_DIR / "verify"
"""Where ``verify`` regenerates the tree before comparing it to the repository."""
TARGET_FILE = SYNC_DIR / "target"
"""Records which upstream commit ``WORK`` was created from."""
PATCHES = ROOT / "patches"
UPSTREAM_PATCHES = PATCHES / "upstream"
"""Patches that keep Cargo building and are candidates for upstream pull requests.

They are applied before the extraction patches, so that when one lands upstream
it can simply be deleted here.
"""
UPSTREAM_TRAILER = "Cargo-Upstream: candidate"
"""Commit message trailer that marks a commit for ``UPSTREAM_PATCHES`` on export."""
"""The patch series, the only hand-maintained input."""
PIN = ROOT / "upstream.toml"
"""Pin file describing what the repository was generated from."""
CONFIG = ROOT / "sync" / "config.toml"
"""Tool configuration."""
DIST = "https://static.rust-lang.org/dist"
"""Rust release manifests live here."""

GENERATED_FILES = ["Cargo.toml", "Cargo.lock", *["rustfmt.toml", "clippy.toml"]]
"""Generated top-level files.

The directories listed under ``[export]`` in the configuration are generated
too. ``rustfmt.toml`` and ``clippy.toml`` are copied from upstream, so the
crates are formatted and linted with Cargo's settings.
"""
UPSTREAM_TOOLING_FILES = ["rustfmt.toml", "clippy.toml"]
"""Files copied from the upstream root as they are."""

EXIT_OK = 0
"""Success."""
EXIT_ERROR = 1
"""A failure unrelated to the patch series or the generated tree."""
EXIT_CONFLICT = 2
"""The patch series does not apply cleanly to the target commit."""
EXIT_UNCHANGED = 3
"""``sync`` found the repository already at the target commit."""
EXIT_DEVIATION = 4
"""``verify`` found the repository differs from upstream plus patches."""

Toml = dict[str, Any]
"""A parsed TOML document or table."""


class SyncError(Exception):
    """An error with a message meant for the user, reported without a traceback."""
