"""Drift reports and the markdown used for pull request bodies."""

from __future__ import annotations

import json
from pathlib import Path

from sync.resolver_sync.common import Toml
from sync.resolver_sync.shell import git
from sync.resolver_sync.upstream import moved_origins, upstream_log

UPSTREAM_WEB = "https://github.com/rust-lang/cargo"
"""Link target for upstream commits in rendered reports."""


def write_report(path: str | None, report: Toml) -> None:
    """Write a sync report as JSON, if a path was requested."""
    if path:
        Path(path).write_text(json.dumps(report, indent=2) + "\n")


def drift(cfg: Toml, old: str, new: str) -> Toml:
    """Summarise what changed upstream between two commits.

    Returns:
        A dict with the commit range, ``total_commits`` in it, the commits
        that touched files the move patch relocates (``merged``, carried
        forward by git automatically) and the commits that touched the
        hand-maintained files under ``[drift]`` (``manual``, for review).
    """
    return {
        "old": old,
        "new": new,
        "total_commits": int(git("rev-list", "--count", f"{old}..{new}") or 0),
        "merged": upstream_log(old, new, moved_origins()),
        "manual": upstream_log(old, new, list(cfg["drift"].keys())),
    }


def format_drift(d: Toml, repo_url: str = UPSTREAM_WEB) -> str:
    """Render a :func:`drift` result as markdown."""

    def fmt(commits: list[Toml]) -> list[str]:
        out = []
        for c in commits:
            files = ", ".join(f"`{f}`" for f in c["files"])
            out.append(
                f"- [`{c['sha']}`]({repo_url}/commit/{c['sha']}) {c['subject']}  \n  {files}"
            )
        return out or ["- none"]

    lines = [
        (
            f"Upstream moved from [`{d['old'][:10]}`]({repo_url}/commit/{d['old']}) to "
            f"[`{d['new'][:10]}`]({repo_url}/commit/{d['new']}) ({d['total_commits']} commits)."
        ),
        "",
        "### Upstream changes to hand-maintained files (review these)",
        "",
        *fmt(d["manual"]),
        "",
        "### Upstream changes merged into moved files automatically",
        "",
        *fmt(d["merged"]),
    ]
    return "\n".join(lines)


def format_report(report: Toml) -> str:
    """Render a sync report as the body of a pull request.

    Stable syncs get a release checklist, since merging them publishes.
    """
    commit = report["commit"]
    channel = report["channel"]
    lines = [
        f"Automated sync of the extracted crates to the `{channel}` channel.",
        "",
        "| | |",
        "|---|---|",
        (
            f"| Cargo commit | [`{commit[:10]}`]({UPSTREAM_WEB}/commit/{commit}) "
            f"({report.get('commit_date', '')}) |"
        ),
        f"| Channel date | {report.get('channel_date') or 'n/a'} |",
        f"| Rust | {report.get('rust_version') or 'n/a'} |",
        f"| Upstream `cargo` version | {report.get('cargo_version', '')} |",
        f"| crate version | {report.get('crate_version', '')} |",
        "",
    ]
    if channel == "stable":
        lines += [
            "### Release checklist",
            "",
            (
                "Merging this pull request triggers the publish workflow, which waits for "
                "approval in the `crates-io` environment before uploading."
            ),
            "",
            "- [ ] CI is green on all platforms",
            (
                "- [ ] The sibling crates this version depends on are published on crates.io "
                "(the publish workflow checks this too)"
            ),
            "- [ ] Upstream changes to hand-maintained files below have been reviewed",
            "",
        ]
        from_git = (report.get("pin") or {}).get("siblings_from_git") or []
        if from_git:
            names = ", ".join(f"`{n}`" for n in from_git)
            rust = str(report.get("rust_version") or "<version>").split()[0]
            lines += [
                (
                    f"crates.io did not have {names} at the versions this Cargo requires when "
                    "this was synced, so the tree builds them from upstream's git "
                    "(`siblings_from_git` in `upstream.toml`). The publish workflow refuses "
                    f"that; once the Cargo team has published them, run `sync/sync.py sync "
                    f"{rust} --force` on this branch and merge the result before publishing."
                ),
                "",
            ]
    changed = report.get("patches_changed") or []
    if changed:
        lines += [
            "### Patch series",
            "",
            (
                f"The series was rebased onto the new commit; {len(changed)} patch "
                "file(s) changed. The diff under `patches/` shows where upstream moved "
                "under the extraction."
            ),
            "",
            *(f"- `{name}`" for name in changed),
            "",
        ]
    if report.get("drift"):
        lines += [
            "### Upstream changes since the last sync",
            "",
            format_drift(report["drift"]),
            "",
        ]
    else:
        lines += [
            "No previous sync recorded, so no upstream change report is available.",
            "",
        ]
    lines += [
        "---",
        (
            "Generated by `sync/sync.py`. Everything under `crates/`, the root "
            "`Cargo.toml`, `Cargo.lock` and `upstream.toml` is generated; to change the "
            "crate, edit the patch series in `patches/` instead."
        ),
    ]
    return "\n".join(lines)
