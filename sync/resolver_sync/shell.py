"""Thin wrappers around ``subprocess`` and ``git``."""

from __future__ import annotations

import os
import subprocess
import sys
from pathlib import Path

from sync.resolver_sync.common import CLONE


def log(msg: str) -> None:
    """Print a progress or diagnostic message to stderr."""
    print(f"sync: {msg}", file=sys.stderr, flush=True)


def git_env() -> dict[str, str]:
    """Return the environment for child processes with a git identity filled in.

    ``git am`` needs a committer identity. Any identity already configured in
    the environment wins; otherwise a neutral one is supplied so the tool works
    on CI runners without global git configuration.
    """
    env = dict(os.environ)
    env.setdefault("GIT_COMMITTER_NAME", "cargo-extracted-bot")
    env.setdefault("GIT_COMMITTER_EMAIL", "cargo-extracted-bot@protonmail.com")
    env.setdefault("GIT_AUTHOR_NAME", env["GIT_COMMITTER_NAME"])
    env.setdefault("GIT_AUTHOR_EMAIL", env["GIT_COMMITTER_EMAIL"])
    return env


def run(
    args: list[str], cwd: Path | None = None, check: bool = True
) -> subprocess.CompletedProcess[str]:
    """Run a command, capturing its output as text.

    Args:
        args: The command and its arguments.
        cwd: Working directory, or the current one if ``None``.
        check: Raise :class:`subprocess.CalledProcessError` on a non-zero exit.
    """
    return subprocess.run(args, cwd=cwd, check=check, text=True, capture_output=True, env=git_env())


def git(*args: str, cwd: Path = CLONE, check: bool = True) -> str:
    """Run ``git`` and return its stripped standard output.

    Args:
        *args: Arguments to ``git``.
        cwd: Repository to operate on; defaults to the upstream clone.
        check: Raise :class:`subprocess.CalledProcessError` on a non-zero exit.
    """
    return run(["git", *args], cwd=cwd, check=check).stdout.strip()
