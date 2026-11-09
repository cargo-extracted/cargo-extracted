"""Producing the generated tree from the patched worktree, and comparing it."""

from __future__ import annotations

import datetime as dt
import difflib
import filecmp
import re
import shutil
from pathlib import Path
from typing import Any

from sync.resolver_sync.common import (
    GENERATED_FILES,
    PIN,
    ROOT,
    UPSTREAM_TOOLING_FILES,
    WORK,
    SyncError,
    Toml,
)
from sync.resolver_sync.config import read_toml, toml_value, upstream_url
from sync.resolver_sync.shell import git, log, run


def export(cfg: Toml, out: Path) -> None:
    """Copy each ``[export]`` directory and Cargo's tooling files to ``out``.

    Destinations are replaced wholesale so files removed upstream disappear.

    Raises:
        SyncError: If a source directory does not exist upstream.
    """
    for dest, src in cfg["export"].items():
        src_path, dest_path = WORK / src, out / dest
        if not src_path.is_dir():
            raise SyncError(f"{src} does not exist in upstream checkout")
        log(f"exporting {src} -> {dest}")
        shutil.rmtree(dest_path, ignore_errors=True)
        shutil.copytree(src_path, dest_path, ignore=shutil.ignore_patterns("target", ".git"))
    for name in UPSTREAM_TOOLING_FILES:
        if (WORK / name).is_file():
            shutil.copyfile(WORK / name, out / name)
        else:
            (out / name).unlink(missing_ok=True)


def add_build_aliases(cfg: Toml) -> None:
    """Make the patched worktree buildable on its own.

    The crates name one another by Cargo's crate names, which the generated
    workspace manifest defines and Cargo's root manifest does not. This adds
    the entries to the worktree's root manifest, uncommitted, so they never
    reach the patches.
    """
    manifest = WORK / "Cargo.toml"
    text = manifest.read_text()
    marker = "[workspace.dependencies]\n"
    if marker not in text or "cargo-extracted:" in text:
        return
    lines = [
        marker,
        "# cargo-extracted: the names the extracted crates use for one another.",
        "# Added by sync/sync.py and not committed; the generated workspace has them.",
    ]
    for entry in published_crates(cfg):
        alias = entry.get("alias", entry["crate"])
        directory = entry["manifest"].rsplit("/", 1)[0]
        spec = {"package": entry["crate"], "path": directory}
        lines.append(f"{alias} = {toml_value(spec)}")
    manifest.write_text(text.replace(marker, "\n".join(lines) + "\n", 1))


def crate_version_for(channel: str, upstream_version: str, override: str | None) -> str:
    """Derive the published crate's version.

    The crate follows the upstream ``cargo`` crate's version. Stable syncs use
    it verbatim; other channels get a pre-release suffix so they can never be
    published by accident.

    Args:
        channel: ``stable``, ``nightly``, ``beta`` or ``commit``.
        upstream_version: The ``cargo`` package version at the synced commit.
        override: An explicit version that wins over the derivation.
    """
    if override:
        return override
    base = upstream_version.split("-", 1)[0]
    if channel == "stable":
        return base
    suffix = {"nightly": "nightly", "beta": "beta"}.get(channel, "dev")
    return f"{base}-{suffix}"


def published_crates(cfg: Toml) -> list[Toml]:
    """Return the ``[[publish]]`` entries, each with ``crate`` and ``manifest``."""
    publish = cfg["publish"]
    return [publish] if isinstance(publish, dict) else list(publish)


def rewrite_crate_manifests(cfg: Toml, version: str, msrv: str | None, out: Path) -> None:
    """Set ``version`` and ``rust-version`` in every exported crate manifest.

    The patch series carries placeholder values; the real ones depend on the
    synced commit. All published crates share one version.

    Raises:
        SyncError: If a manifest has no ``version`` line to rewrite.
    """
    for entry in published_crates(cfg):
        path = out / entry["manifest"]
        text = path.read_text()
        text, n = re.subn(
            r'^version = "[^"]*"',
            f'version = "{version}"',
            text,
            count=1,
            flags=re.MULTILINE,
        )
        if n != 1:
            raise SyncError(f"could not find version line in {path}")
        if msrv:
            text, _ = re.subn(
                r'^rust-version = "[^"]*".*$',
                f'rust-version = "{msrv}"',
                text,
                count=1,
                flags=re.MULTILINE,
            )
        path.write_text(text)


def workspace_dep_names(manifest: Toml) -> set[str]:
    """Return the names a crate manifest pulls in with ``workspace = true``.

    Covers normal, dev and build dependencies, including target-specific ones.
    """
    names: set[str] = set()

    def visit(table: Toml | None) -> None:
        for name, spec in (table or {}).items():
            if isinstance(spec, dict) and spec.get("workspace"):
                names.add(name)

    for key in ("dependencies", "dev-dependencies", "build-dependencies"):
        visit(manifest.get(key))
    for tgt in manifest.get("target", {}).values():
        for key in ("dependencies", "dev-dependencies", "build-dependencies"):
            visit(tgt.get(key))
    return names


def generate_workspace(
    cfg: Toml, commit: str, crate_version: str, channel: str, out: Path
) -> dict[str, str]:
    """Write the root ``Cargo.toml`` for the generated workspace.

    The crates reference workspace dependencies, lints and package metadata
    by ``workspace = true``, so the root manifest is rebuilt from upstream's:
    only the dependency entries the members actually use are copied, sibling
    crates lose their ``path`` and come from crates.io, and off the stable
    channel a ``[patch.crates-io]`` section builds the siblings from the same
    upstream commit.

    Returns:
        The sibling crates and the versions required of them.

    Raises:
        SyncError: If a crate uses a workspace dependency upstream lacks.
    """
    upstream = read_toml(WORK / "Cargo.toml")
    ws = upstream["workspace"]
    # Keyed by the dependency key other crates use (the `alias`, which is
    # Cargo's own crate name), mapping to the published package name and
    # its directory.
    published = {
        entry.get("alias", entry["crate"]): (entry["crate"], entry["manifest"].rsplit("/", 1)[0])
        for entry in published_crates(cfg)
    }
    siblings: list[str] = cfg["siblings"]["crates"]
    sibling_versions: dict[str, str] = {}

    exported: dict[str, Toml] = {}
    for dest in cfg["export"]:
        exported[dest] = read_toml(out / dest / "Cargo.toml")
    local_crates = {m["package"]["name"]: d for d, m in exported.items()}

    # The hand-maintained members use workspace dependencies too.
    members = dict(exported)
    for rel in kept_paths(cfg):
        if (out / rel / "Cargo.toml").exists():
            members[rel] = read_toml(out / rel / "Cargo.toml")

    needed: set[str] = set()
    for manifest in members.values():
        needed |= workspace_dep_names(manifest)

    deps: dict[str, Any] = {}
    for name in sorted(needed):
        if name in published:
            # Built from one commit, so the crates require one another exactly.
            package, path = published[name]
            spec = {"version": f"={crate_version}", "path": path}
            if package != name:
                spec = {"package": package, **spec}
            deps[name] = spec
            continue
        spec = ws["dependencies"].get(name)
        if spec is None:
            raise SyncError(
                f"{name} is used as a workspace dependency but upstream does not define it"
            )
        spec = dict(spec) if isinstance(spec, dict) else {"version": spec}
        if name in siblings:
            spec.pop("path", None)
            sibling_versions[name] = str(spec["version"])
        elif "path" in spec:
            log(
                f"warning: {name} is a path dependency upstream and not listed in "
                f"[siblings]; using crates.io version {spec.get('version')}"
            )
            spec.pop("path")
        elif name in local_crates:
            spec["path"] = local_crates[name]
        deps[name] = spec

    pkg = ws.get("package", {})
    lines = [
        "# This file is generated by `sync/sync.py`. Do not edit it by hand.",
        f"# Generated from rust-lang/cargo @ {commit}",
        "",
        "[workspace]",
        f"resolver = {toml_value(ws.get('resolver', '2'))}",
        'members = ["crates/*"]',
        "",
        "[workspace.package]",
        f"edition = {toml_value(pkg.get('edition', '2021'))}",
        f"license = {toml_value(pkg.get('license', 'MIT OR Apache-2.0'))}",
        f"repository = {toml_value(cfg['repository']['url'])}",
    ]
    if "rust-version" in pkg:
        lines.append(f"rust-version = {toml_value(pkg['rust-version'])}")
    lines += ["", "[workspace.dependencies]"]
    for name, spec in deps.items():
        lines.append(f"{name} = {toml_value(spec)}")
    for table, values in ws.get("lints", {}).items():
        lines += ["", f"[workspace.lints.{table}]"]
        for k, v in values.items():
            lines.append(f"{k} = {toml_value(v)}")
    if channel != "stable" and sibling_versions:
        lines += [
            "",
            "# Upstream bumps sibling crate versions weeks before the Cargo team publishes",
            "# them. Off the stable channel, build them from the same upstream commit.",
            "# This only affects builds of this workspace, not the published crate.",
            "[patch.crates-io]",
        ]
        for name in sorted(sibling_versions):
            lines.append(f"{name} = {toml_value({'git': upstream_url(cfg), 'rev': commit})}")
    (out / "Cargo.toml").write_text("\n".join(lines) + "\n")
    return sibling_versions


def sync_lockfile(out: Path) -> None:
    """Derive ``Cargo.lock`` from upstream's.

    Seeding from upstream's lock keeps the dependency versions Cargo itself was
    tested with; ``cargo update --workspace`` then drops the entries this
    smaller workspace does not use.

    Raises:
        SyncError: If ``cargo update`` fails.
    """
    shutil.copyfile(WORK / "Cargo.lock", out / "Cargo.lock")
    log("trimming Cargo.lock")
    proc = run(["cargo", "update", "--workspace"], cwd=out, check=False)
    if proc.returncode != 0:
        raise SyncError(f"cargo update --workspace failed:\n{proc.stderr}")


def write_pin(
    cfg: Toml,
    info: Toml,
    commit: str,
    crate_version: str,
    upstream_version: str,
    siblings: dict[str, str],
) -> Toml:
    """Write ``upstream.toml`` describing what the tree was generated from.

    Args:
        cfg: Tool configuration.
        info: Channel metadata from :func:`resolver_sync.channels.resolve_target`.
        commit: The full upstream commit hash.
        crate_version: The version written into the crate manifest.
        upstream_version: The ``cargo`` package version at the commit.
        siblings: Sibling crate versions the published crate depends on; they
            must be on crates.io before a release can be published.

    Returns:
        The pin contents.
    """
    pin: Toml = {
        "repo": upstream_url(cfg),
        "commit": commit,
        "commit_date": git("log", "-1", "--format=%cs", commit),
        "channel": info["channel"],
        "channel_date": info.get("channel_date") or "",
        "rust_version": info.get("rust_version") or "",
        "cargo_version": upstream_version,
        "crate_version": crate_version,
        "synced_at": dt.datetime.now(dt.UTC).strftime("%Y-%m-%dT%H:%M:%SZ"),
        "siblings": dict(sorted(siblings.items())),
    }
    lines = [
        "# Generated by `sync/sync.py`. Records the upstream Cargo commit this tree",
        "# was generated from. Do not edit by hand.",
    ] + [f"{k} = {toml_value(v)}" for k, v in pin.items()]
    PIN.write_text("\n".join(lines) + "\n")
    return pin


def regenerate(
    cfg: Toml, info: Toml, commit: str, version_override: str | None, out: Path, lock: bool
) -> tuple[str, str, dict[str, str]]:
    """Produce the complete generated tree under ``out`` from the patched worktree.

    Args:
        cfg: Tool configuration.
        info: Channel metadata; only ``channel`` is required.
        commit: The full upstream commit hash.
        version_override: Explicit crate version, or ``None`` to derive one.
        out: Directory to generate into, the repository root or a scratch dir.
        lock: Whether to regenerate ``Cargo.lock`` (needs ``cargo`` and network).

    Returns:
        The crate version, the upstream ``cargo`` version, and the sibling
        versions required.
    """
    upstream_pkg = read_toml(WORK / "Cargo.toml")["package"]
    upstream_version = str(upstream_pkg["version"])
    crate_version = crate_version_for(info["channel"], upstream_version, version_override)
    export(cfg, out)
    rewrite_crate_manifests(cfg, crate_version, upstream_pkg.get("rust-version"), out)
    siblings = generate_workspace(cfg, commit, crate_version, info["channel"], out)
    if lock:
        sync_lockfile(out)
    return crate_version, upstream_version, siblings


def generated_paths(cfg: Toml, lock: bool = True) -> list[str]:
    """Return every repository path that is generated rather than maintained.

    Args:
        cfg: Tool configuration.
        lock: Include ``Cargo.lock``; ``verify --no-lock`` leaves it out.
    """
    files = GENERATED_FILES if lock else [f for f in GENERATED_FILES if f != "Cargo.lock"]
    return [*cfg["export"], *files]


def kept_paths(cfg: Toml) -> list[str]:
    """Return the hand-maintained workspace members listed under ``[verify] keep``.

    They are not generated, but they are members of the generated workspace
    and therefore part of ``Cargo.lock``.
    """
    keep: list[str] = cfg.get("verify", {}).get("keep", [])
    return keep


def compare_trees(cfg: Toml, expected: Path, lock: bool = True) -> list[str]:
    """Describe every way the repository's generated paths differ from ``expected``.

    Args:
        cfg: Tool configuration.
        expected: The freshly generated tree.
        lock: Whether ``Cargo.lock`` was regenerated and should be compared.

    Returns:
        Human-readable problems: files present only on one side, and for files
        that differ, a unified diff truncated to forty lines. Empty when the
        trees are identical.
    """
    problems: list[str] = []

    def rel_files(base: Path, rel: str) -> set[str]:
        root = base / rel
        if root.is_file():
            return {rel}
        if not root.exists():
            return set()
        return {str(p.relative_to(base)) for p in root.rglob("*") if p.is_file()}

    for rel in generated_paths(cfg, lock):
        have, want = rel_files(ROOT, rel), rel_files(expected, rel)
        problems.extend(
            f"{f}: present in repository but not generated" for f in sorted(have - want)
        )
        problems.extend(f"{f}: generated but missing from repository" for f in sorted(want - have))
        for f in sorted(have & want):
            if filecmp.cmp(ROOT / f, expected / f, shallow=False):
                continue
            try:
                a = (expected / f).read_text().splitlines(keepends=True)
                b = (ROOT / f).read_text().splitlines(keepends=True)
                diff = list(difflib.unified_diff(a, b, f"generated/{f}", f"repository/{f}"))
            except UnicodeDecodeError:
                diff = []
            shown = "".join(diff[:40]) + ("...\n" if len(diff) > 40 else "")
            problems.append(f"{f}: differs\n{shown}")
    return problems
