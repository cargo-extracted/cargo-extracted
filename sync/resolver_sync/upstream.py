"""The local clone of rust-lang/cargo, its scratch worktree, and the patch series."""

from __future__ import annotations

import os
import re
import shutil
from pathlib import Path

from sync.resolver_sync.common import (
    CLONE,
    PATCHES,
    ROOT,
    SYNC_DIR,
    TARGET_FILE,
    UPSTREAM_PATCHES,
    UPSTREAM_TRAILER,
    WORK,
    SyncError,
    Toml,
)
from sync.resolver_sync.shell import git, log, run


def ensure_clone(repo: str) -> None:
    """Create or refresh the upstream clone under ``.sync/cargo``.

    Remote clones use ``--filter=blob:none``, so only the blobs the checkout
    and ``git am -3`` touch are downloaded. A clone made from a different
    remote is recreated.
    """
    SYNC_DIR.mkdir(exist_ok=True)
    if (CLONE / ".git").exists():
        current = git("remote", "get-url", "origin")
        if current != repo:
            log(f"upstream remote changed ({current} -> {repo}); re-cloning")
            shutil.rmtree(CLONE)
    if not (CLONE / ".git").exists():
        local = Path(repo).expanduser().exists()
        args = ["git", "clone", "--no-checkout"]
        if not local:
            args.append("--filter=blob:none")
        log(f"cloning {repo} into {CLONE}")
        run([*args, repo, str(CLONE)])
    else:
        log("fetching upstream")
        git("fetch", "--prune", "origin")


def resolve_commit(short: str) -> str:
    """Expand a possibly abbreviated commit hash against the clone.

    Falls back to fetching the commit by name, which GitHub allows for any
    reachable commit, so a hash newer than the last fetch still resolves.

    Raises:
        SyncError: If the commit cannot be found.
    """
    full = git("rev-parse", "--verify", "--quiet", f"{short}^{{commit}}", check=False)
    if not full:
        log(f"{short} not found locally; fetching it explicitly")
        git("fetch", "origin", short, check=False)
        full = git("rev-parse", "--verify", "--quiet", f"{short}^{{commit}}", check=False)
    if not full:
        raise SyncError(f"commit {short} not found in upstream repository")
    return full


def prepare_worktree(commit: str) -> None:
    """Check ``commit`` out into a fresh scratch worktree and record it."""
    if WORK.exists():
        git("worktree", "remove", "--force", str(WORK), check=False)
        shutil.rmtree(WORK, ignore_errors=True)
    git("worktree", "prune")
    log(f"checking out {commit[:10]} into {WORK}")
    git("worktree", "add", "--detach", "--force", str(WORK), commit)
    TARGET_FILE.write_text(commit + "\n")


def remove_worktree() -> None:
    """Remove the scratch worktree if it exists."""
    if WORK.exists():
        git("worktree", "remove", "--force", str(WORK), check=False)
        shutil.rmtree(WORK, ignore_errors=True)
    if (CLONE / ".git").exists():
        git("worktree", "prune")


def patch_files() -> list[Path]:
    """Return the patch series in application order.

    Upstream-candidate patches (``patches/upstream/``) come first: they keep
    Cargo building and are what gets sent to rust-lang/cargo. The extraction
    patches follow.

    Raises:
        SyncError: If ``patches/`` is empty.
    """
    patches = sorted(UPSTREAM_PATCHES.glob("*.patch")) + sorted(PATCHES.glob("*.patch"))
    if not patches:
        raise SyncError(f"no patches found in {PATCHES}")
    return patches


def apply_patches(three_way: bool = True) -> Toml | None:
    """Replay the patch series onto the scratch worktree with ``git am``.

    Args:
        three_way: Fall back to a three-way merge when a hunk's context has
            moved (``git am -3``). ``sync`` uses this to carry the series onto
            a newer upstream commit. ``verify`` does not: the committed series
            is expected to be rebased onto the pinned commit by
            :func:`export_patches`, so it has to apply as plain patches.

    Returns:
        ``None`` on success. On failure, a dict with the failing patch's
        ``patch_number`` and ``patch_subject``, the conflicted ``files`` and
        git's ``output``. The worktree is left mid-``am`` for a human to
        resolve.
    """
    patches = patch_files()
    flags = ["-3"] if three_way else []
    log(f"applying {len(patches)} patches with git am {' '.join(flags)}".rstrip())
    proc = run(["git", "am", *flags, "--no-gpg-sign", *map(str, patches)], cwd=WORK, check=False)
    if proc.returncode == 0:
        return None
    output = proc.stdout + proc.stderr
    m = re.search(r"Patch failed at (\d+) (.*)", output)
    conflicted = git("diff", "--name-only", "--diff-filter=U", cwd=WORK).splitlines()
    return {
        "patch_number": int(m[1]) if m else None,
        "patch_subject": m[2].strip() if m else None,
        "files": conflicted,
        "output": output.strip(),
    }


def export_patches() -> list[Path]:
    """Rewrite ``patches/`` from the commits on top of the scratch worktree.

    ``sync`` calls this after every successful apply, so the series is always
    rebased onto the commit in ``upstream.toml``; ``export-patches`` calls it
    after a human has finished ``git am``. Commits with the
    ``Cargo-Upstream: candidate`` trailer go to ``patches/upstream/`` and must
    form a prefix of the series.

    Returns:
        The patch files written, in application order.

    Raises:
        SyncError: If there is no worktree, ``git am`` is unfinished, or the
            upstream-candidate commits are not a prefix of the series.
    """
    if not WORK.exists() or not TARGET_FILE.exists():
        raise SyncError("no scratch worktree; run `sync` first")
    if am_in_progress():
        raise SyncError(
            f"`git am` is still in progress in {WORK}; finish it with `git am --continue`"
        )
    base = TARGET_FILE.read_text().strip()
    commits = git("rev-list", "--reverse", f"{base}..HEAD", cwd=WORK).split()
    if not commits:
        raise SyncError("worktree has no commits on top of the upstream commit")
    upstream = [UPSTREAM_TRAILER in git("log", "-1", "--format=%B", c, cwd=WORK) for c in commits]
    n_upstream = upstream.count(True)
    if upstream[:n_upstream] != [True] * n_upstream:
        raise SyncError(
            f"commits marked `{UPSTREAM_TRAILER}` must come before the extraction commits"
        )
    log(f"exporting {len(commits)} commits as patches ({n_upstream} upstream candidates)")
    for directory in (UPSTREAM_PATCHES, PATCHES):
        for old in directory.glob("*.patch"):
            old.unlink()
    ranges = []
    if n_upstream:
        ranges.append((UPSTREAM_PATCHES, f"{base}..{commits[n_upstream - 1]}"))
    if n_upstream < len(commits):
        start = commits[n_upstream - 1] if n_upstream else base
        ranges.append((PATCHES, f"{start}..HEAD"))
    written: list[Path] = []
    for directory, rev_range in ranges:
        directory.mkdir(parents=True, exist_ok=True)
        out = git(
            "format-patch",
            "-M",
            "--zero-commit",
            "--no-signature",
            "-o",
            str(directory),
            rev_range,
            cwd=WORK,
        )
        written.extend(Path(line) for line in out.splitlines())
    return written


def changed_patches() -> list[str]:
    """Return the patch files that differ from the repository's committed series."""
    args = ["git", "status", "--porcelain", "--untracked-files=all", "--", str(PATCHES)]
    out = run(args, cwd=ROOT).stdout
    return sorted(line[3:] for line in out.splitlines() if len(line) > 3)


def am_in_progress() -> bool:
    """Return whether the scratch worktree is still in the middle of ``git am``."""
    path = git("rev-parse", "--git-path", "rebase-apply", cwd=WORK)
    return (WORK / path).exists() if not os.path.isabs(path) else Path(path).exists()


def moved_origins() -> list[str]:
    """Return the upstream paths that the move patch relocates.

    Upstream edits to these files are carried forward by git's rename
    detection, as opposed to the hand-maintained files listed under
    ``[drift]`` in the configuration.
    """
    prefix = "rename from "
    return [
        line[len(prefix) :]
        for patch in patch_files()
        for line in patch.read_text().splitlines()
        if line.startswith(prefix)
    ]


def upstream_log(old: str, new: str, paths: list[str]) -> list[Toml]:
    """List upstream commits in ``old..new`` that touch any of ``paths``.

    Returns:
        One dict per commit with ``sha``, ``subject`` and the touched ``files``.
    """
    if not paths:
        return []
    out = git("log", "--format=%x00%h %s", "--name-only", f"{old}..{new}", "--", *paths)
    commits = []
    for block in out.split("\x00"):
        block = block.strip()
        if not block:
            continue
        head, *files = block.splitlines()
        sha, _, subject = head.partition(" ")
        commits.append({"sha": sha, "subject": subject, "files": [f for f in files if f]})
    return commits
