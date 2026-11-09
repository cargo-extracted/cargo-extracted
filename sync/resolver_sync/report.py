"""Drift reports and the markdown used for pull request bodies."""

from __future__ import annotations

import json
from pathlib import Path

from sync.resolver_sync.common import Toml
from sync.resolver_sync.shell import git
from sync.resolver_sync.upstream import moved_origins, upstream_log

UPSTREAM_WEB = "https://github.com/rust-lang/cargo"
"""Link target for upstream commits in rendered reports."""
MAX_DRIFT_COMMITS = 40
"""How many hand-copied-file commits a pull request body lists before summarising."""


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

    The table says what was synced. The only part that asks for a reader is
    the list of upstream commits that touched the hand-copied files, which git
    cannot merge, so it is kept, collapsed; the commits git merged into moved
    files show in the diff already.
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
    from_git = (report.get("pin") or {}).get("siblings_from_git") or []
    if from_git:
        names = ", ".join(f"`{n}`" for n in from_git)
        rust = str(report.get("rust_version") or "<version>").split()[0]
        lines += [
            (
                f"crates.io does not have {names} at the versions this Cargo requires yet, so "
                "the tree builds them from upstream's git and cannot be published. Once the "
                f"Cargo team has published them, run `sync/sync.py sync {rust} --force` on this "
                "branch and merge the result."
            ),
            "",
        ]
    d = report.get("drift")
    if d:
        manual: list[Toml] = d["manual"]
        shown = manual[:MAX_DRIFT_COMMITS]
        summary = (
            f"{len(manual)} upstream commit(s) touched hand-copied files, of "
            f"{d['total_commits']} since the last sync"
        )
        lines += [f"<details><summary>{summary}</summary>", ""]
        for c in shown:
            files = ", ".join(f"`{f}`" for f in c["files"])
            lines.append(
                f"- [`{c['sha']}`]({UPSTREAM_WEB}/commit/{c['sha']}) {c['subject']}: {files}"
            )
        if len(manual) > len(shown):
            lines.append(f"- and {len(manual) - len(shown)} more")
        if not manual:
            lines.append("- none")
        lines += ["", "</details>"]
    return "\n".join(lines)
