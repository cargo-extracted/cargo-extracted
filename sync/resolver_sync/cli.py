"""Sync the crates extracted from Cargo into this repository.

The model: Cargo's sources are checked out at a chosen commit, the patch
series in `patches/` is replayed on top with `git am -3`, the series is
re-exported so it is rebased onto that commit, and the resulting crate
directories are copied into this repository together with a generated
workspace manifest and a pin file recording what was synced. See README.md.

Subcommands:
  resolve <target>          Show which Cargo commit a target refers to.
  sync <target>             Sync this repository to a target.
  verify                    Check the tree equals upstream plus patches.
  export-patches            Regenerate patches/ after resolving conflicts.
  drift <old> <new>         Report upstream changes between two commits.
  branch-point <commit>     Print the commit whose patch series a release
                            branch for an upstream commit starts from.
  plan                      Print, as JSON, which branch to sync to what for
                            each channel; the scheduled workflow's matrix.
  markdown <report.json>    Render a sync report for a pull request body.
  clean                     Remove the scratch worktree.

Targets: `nightly`, `beta`, `stable`, `<channel>-YYYY-MM-DD`, a Rust version
such as `1.99.0`, or a Cargo commit hash. With `--lag N`, `nightly` and `beta`
mean the dated manifest N days ago (the latest one at or before it), which is
immutable where the channel pointer is not.

Exit codes for `sync`: 0 synced, 1 error, 2 patch conflict, 3 already in sync.
Exit codes for `verify`: 0 identical, 1 error, 2 patch conflict, 4 deviation.
"""

from __future__ import annotations

import argparse
import json
import os
import re
import shutil
import subprocess
import sys
from pathlib import Path

from sync.resolver_sync.channels import plan, resolve_target
from sync.resolver_sync.common import (
    CLONE,
    EXIT_CONFLICT,
    EXIT_DEVIATION,
    EXIT_ERROR,
    EXIT_OK,
    EXIT_UNCHANGED,
    PIN,
    ROOT,
    VERIFY,
    WORK,
    SyncError,
    Toml,
)
from sync.resolver_sync.config import load_config, load_pin
from sync.resolver_sync.generate import (
    add_build_aliases,
    compare_trees,
    kept_paths,
    regenerate,
    write_pin,
)
from sync.resolver_sync.report import drift, format_drift, format_report, write_report
from sync.resolver_sync.shell import git, log, run
from sync.resolver_sync.upstream import (
    apply_patches,
    changed_patches,
    ensure_clone,
    export_patches,
    patch_files,
    prepare_worktree,
    remove_worktree,
    resolve_commit,
)


def upstream_repo(args: argparse.Namespace, cfg: Toml) -> str:
    """Pick the upstream repository: ``--repo``, then the environment, then config."""
    repo = args.repo or os.environ.get("CARGO_EXTRACTED_UPSTREAM") or cfg["upstream"]["repo"]
    return str(repo)


def report_conflict(conflict: Toml) -> None:
    """Explain a failed ``git am`` and how to recover from it."""
    log("patch series did not apply cleanly")
    log(f"  failed patch: {conflict['patch_number']} {conflict['patch_subject']}")
    for f in conflict["files"]:
        log(f"  conflict: {f}")
    log(f"resolve the conflicts in {WORK}, run `git am --continue` there,")
    log("then `sync/sync.py export-patches` and re-run this command.")


def cmd_resolve(args: argparse.Namespace) -> int:
    """Print, as JSON, which Cargo commit a target refers to."""
    cfg = load_config()
    info = resolve_target(args.target, args.lag)
    if not args.no_fetch:
        ensure_clone(upstream_repo(args, cfg))
        info["commit"] = resolve_commit(info["commit"])
    print(json.dumps(info, indent=2))
    return EXIT_OK


def cmd_sync(args: argparse.Namespace) -> int:
    """Regenerate the repository from a target commit plus the patch series."""
    cfg = load_config()
    pin = load_pin()

    info = resolve_target(args.target, args.lag)
    ensure_clone(upstream_repo(args, cfg))
    commit = resolve_commit(info["commit"])
    info["commit"] = commit
    report: Toml = {
        "target": args.target,
        "commit": commit,
        "previous_commit": pin["commit"] if pin else None,
        "channel": info["channel"],
        "changed": False,
        "conflict": None,
    }

    if pin and pin["commit"] == commit and not args.force:
        log(f"already synced to {commit[:10]}; nothing to do (use --force to regenerate)")
        write_report(args.report, report)
        return EXIT_UNCHANGED

    prepare_worktree(commit)
    conflict = apply_patches()
    if conflict:
        report["conflict"] = conflict
        write_report(args.report, report)
        report_conflict(conflict)
        return EXIT_CONFLICT

    # Rebase the series onto the new commit, so that `patches/` always applies
    # to the commit in upstream.toml as plain patches and the diff of the
    # series shows how upstream moved under it.
    export_patches()
    report["patches_changed"] = changed_patches()
    if args.keep_work:
        add_build_aliases(cfg)

    crate_version, upstream_version, siblings = regenerate(
        cfg, info, commit, args.version, ROOT, lock=not args.no_lock
    )
    new_pin = write_pin(cfg, info, commit, crate_version, upstream_version, siblings)

    report.update(
        {
            "changed": True,
            "crate_version": crate_version,
            "cargo_version": upstream_version,
            "rust_version": info.get("rust_version"),
            "channel_date": info.get("channel_date"),
            "commit_date": new_pin["commit_date"],
            "pin": new_pin,
        }
    )
    if pin and pin.get("commit"):
        try:
            report["drift"] = drift(cfg, pin["commit"], commit)
        except subprocess.CalledProcessError as e:
            log(f"warning: could not compute drift report: {e.stderr.strip()}")
    write_report(args.report, report)

    log(f"synced to cargo@{commit[:10]} ({info['channel']}), crate version {crate_version}")
    if report["patches_changed"]:
        log(f"rebased patch series: {len(report['patches_changed'])} patch files changed")
    if "drift" in report:
        d = report["drift"]
        log(
            f"upstream commits since last sync: {d['total_commits']} "
            f"({len(d['merged'])} touched moved files, "
            f"{len(d['manual'])} touched hand-maintained files)"
        )
    if not args.keep_work:
        remove_worktree()
    return EXIT_OK


def cmd_verify(args: argparse.Namespace) -> int:
    """Regenerate from the pinned commit into a scratch directory and compare.

    This is what CI runs to guarantee every edit lives in the patch series.
    """
    cfg = load_config()
    pin = load_pin()
    if pin is None:
        raise SyncError(f"{PIN} does not exist; nothing to verify against")
    ensure_clone(upstream_repo(args, cfg))
    commit = resolve_commit(str(pin["commit"]))
    info: Toml = {"target": commit, "channel": pin["channel"], "commit": commit}

    prepare_worktree(commit)
    # No three-way fallback: the committed series has to be rebased onto the
    # pinned commit, which is what `sync` guarantees by re-exporting it.
    conflict = apply_patches(three_way=False)
    if conflict:
        log("patch series does not apply to the pinned commit as plain patches")
        log(f"  failed patch: {conflict['patch_number']} {conflict['patch_subject']}")
        log("re-export the series against the pinned commit with")
        log(f"  python3 sync/sync.py sync {commit[:10]} --force")
        if not args.keep_work:
            remove_worktree()
        return EXIT_CONFLICT

    shutil.rmtree(VERIFY, ignore_errors=True)
    VERIFY.mkdir(parents=True)
    # Workspace members maintained by hand are part of the lock file, so the
    # scratch tree needs them before the lock is regenerated.
    for rel in kept_paths(cfg):
        src = ROOT / rel
        if not src.exists():
            raise SyncError(f"[verify] keep lists {rel}, which does not exist")
        shutil.copytree(src, VERIFY / rel, ignore=shutil.ignore_patterns("target"))
    regenerate(cfg, info, commit, str(pin["crate_version"]), VERIFY, lock=not args.no_lock)
    problems = compare_trees(cfg, VERIFY, lock=not args.no_lock)
    if not args.keep_work:
        remove_worktree()
        shutil.rmtree(VERIFY, ignore_errors=True)

    if problems:
        log(f"repository deviates from upstream@{commit[:10]} plus patches:")
        for problem in problems:
            print(problem, file=sys.stderr)
        log("edit the patch series (see CONTRIBUTING.md), not the generated files")
        return EXIT_DEVIATION
    log(f"repository matches upstream@{commit[:10]} plus {len(patch_files())} patches")
    return EXIT_OK


def cmd_export_patches(args: argparse.Namespace) -> int:
    """Rewrite ``patches/`` from the commits on top of the scratch worktree.

    Used after a human has resolved conflicts and finished ``git am``; ``sync``
    does the same thing itself after every successful apply.
    """
    for path in export_patches():
        log(f"  wrote {path.relative_to(ROOT)}")
    log("now re-run `sync/sync.py sync <target>` to regenerate the tree from the new patches")
    return EXIT_OK


def cmd_drift(args: argparse.Namespace) -> int:
    """Print the upstream changes between two commits, as markdown or JSON."""
    cfg = load_config()
    if not args.no_fetch:
        ensure_clone(upstream_repo(args, cfg))
    d = drift(cfg, resolve_commit(args.old), resolve_commit(args.new))
    print(json.dumps(d, indent=2) if args.json else format_drift(d))
    return EXIT_OK


def cmd_branch_point(args: argparse.Namespace) -> int:
    """Print the commit whose patch series a new release branch should start from.

    The newest commit on ``--ref`` (``main`` by default) whose
    ``upstream.toml`` pins an ancestor of the upstream commit: the series
    there was rebased just before Cargo's branch point, so only the backports
    on the release branch remain to be absorbed. The branch itself is cut
    from the tip of ``--ref``, for the current tooling; only ``patches/`` is
    taken from here. When there is no such commit, because this repository
    started after the upstream branch was cut, the oldest pinned commit is
    printed with a warning.
    """
    cfg = load_config()
    if not args.no_fetch:
        ensure_clone(upstream_repo(args, cfg))
    upstream = resolve_commit(args.commit)
    pin_path = str(PIN.relative_to(ROOT))
    pinned: list[tuple[str, str]] = []
    for sha in git("log", "--format=%H", args.ref, "--", pin_path, cwd=ROOT).split():
        text = git("show", f"{sha}:{pin_path}", cwd=ROOT, check=False)
        if m := re.search(r'^commit = "([0-9a-f]+)"', text, re.MULTILINE):
            pinned.append((sha, m[1]))
    if not pinned:
        raise SyncError(f"no commit on {args.ref} has an {pin_path}")
    for sha, pin in pinned:
        is_ancestor = ["git", "merge-base", "--is-ancestor", pin, upstream]
        if run(is_ancestor, cwd=CLONE, check=False).returncode == 0:
            log(f"{sha[:10]} pins {pin[:10]}, an ancestor of {upstream[:10]}")
            print(sha)
            return EXIT_OK
    sha, pin = pinned[-1]
    log(
        f"warning: no commit on {args.ref} pins an ancestor of {upstream[:10]}; "
        f"falling back to the oldest, {sha[:10]} (pins {pin[:10]})"
    )
    print(sha)
    return EXIT_OK


def cmd_plan(args: argparse.Namespace) -> int:
    """Print the sync matrix for the scheduled workflow as JSON."""
    channels = [c.strip() for c in args.channels.split(",") if c.strip()]
    for c in channels:
        if c not in ("nightly", "beta", "stable"):
            raise SyncError(f"unknown channel {c!r}")
    print(json.dumps(plan(channels, args.lag), indent=2))
    return EXIT_OK


def cmd_markdown(args: argparse.Namespace) -> int:
    """Render a JSON sync report as markdown for a pull request body."""
    report = json.loads(Path(args.report).read_text())
    print(format_report(report))
    return EXIT_OK


def cmd_clean(args: argparse.Namespace) -> int:
    """Remove the scratch worktree."""
    remove_worktree()
    log("removed scratch worktree")
    return EXIT_OK


def build_parser() -> argparse.ArgumentParser:
    """Build the command line parser."""
    p = argparse.ArgumentParser(
        description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter
    )
    p.add_argument(
        "--repo",
        help="upstream repository URL or local path (default: sync/config.toml)",
    )
    sub = p.add_subparsers(dest="cmd", required=True)

    lag_help = "for nightly and beta, use the dated manifest this many days back"

    s = sub.add_parser("resolve", help="show which Cargo commit a target refers to")
    s.add_argument("target")
    s.add_argument("--lag", type=int, default=0, metavar="DAYS", help=lag_help)
    s.add_argument("--no-fetch", action="store_true", help="do not touch the upstream clone")
    s.set_defaults(fn=cmd_resolve)

    s = sub.add_parser("sync", help="sync this repository to a target")
    s.add_argument("target")
    s.add_argument("--lag", type=int, default=0, metavar="DAYS", help=lag_help)
    s.add_argument("--version", help="override the crate version (default: derived from upstream)")
    s.add_argument(
        "--force",
        action="store_true",
        help="regenerate even if already at the target commit",
    )
    s.add_argument("--no-lock", action="store_true", help="do not regenerate Cargo.lock")
    s.add_argument(
        "--keep-work",
        action="store_true",
        help="keep the scratch worktree after a successful sync",
    )
    s.add_argument("--report", metavar="FILE", help="write a JSON report")
    s.set_defaults(fn=cmd_sync)

    s = sub.add_parser("verify", help="check the generated tree equals upstream plus patches")
    s.add_argument("--no-lock", action="store_true", help="do not regenerate or compare Cargo.lock")
    s.add_argument("--keep-work", action="store_true", help="keep the scratch directories")
    s.set_defaults(fn=cmd_verify)

    s = sub.add_parser("export-patches", help="regenerate patches/ from the scratch worktree")
    s.set_defaults(fn=cmd_export_patches)

    s = sub.add_parser("drift", help="report upstream changes between two commits")
    s.add_argument("old")
    s.add_argument("new")
    s.add_argument("--json", action="store_true")
    s.add_argument("--no-fetch", action="store_true")
    s.set_defaults(fn=cmd_drift)

    s = sub.add_parser(
        "branch-point",
        help="print the commit whose patch series a new release branch starts from",
    )
    s.add_argument("commit", help="upstream Cargo commit the release branch will track")
    s.add_argument("--ref", default="main", help="where to look (default: main)")
    s.add_argument("--no-fetch", action="store_true")
    s.set_defaults(fn=cmd_branch_point)

    s = sub.add_parser("plan", help="print which branch to sync to what, per channel")
    s.add_argument(
        "--channels",
        default="nightly,stable",
        help="comma-separated subset of nightly, stable, beta (default: nightly,stable)",
    )
    s.add_argument("--lag", type=int, default=1, metavar="DAYS", help=lag_help)
    s.set_defaults(fn=cmd_plan)

    s = sub.add_parser(
        "markdown",
        help="render a JSON sync report as markdown (for pull request bodies)",
    )
    s.add_argument("report")
    s.set_defaults(fn=cmd_markdown)

    s = sub.add_parser("clean", help="remove the scratch worktree")
    s.set_defaults(fn=cmd_clean)
    return p


def main(argv: list[str]) -> int:
    """Run the command line tool and return its exit code."""
    args = build_parser().parse_args(argv)
    try:
        result: int = args.fn(args)
        return result
    except SyncError as e:
        log(f"error: {e}")
        return EXIT_ERROR
    except subprocess.CalledProcessError as e:
        log(f"error: command failed: {' '.join(e.cmd)}\n{e.stderr}")
        return EXIT_ERROR
