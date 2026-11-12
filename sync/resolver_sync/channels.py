"""Mapping sync targets to Cargo commits via the Rust release manifests."""

from __future__ import annotations

import datetime as dt
import re
import tomllib
import urllib.error
import urllib.request

from sync.resolver_sync.common import DIST, SyncError, Toml
from sync.resolver_sync.shell import log

VERSION_RE = re.compile(
    r"^(?P<version>\S+) \((?P<sha>[0-9a-f]{7,40}) (?P<date>\d{4}-\d{2}-\d{2})\)"
)
"""Cargo's version string in a release manifest.

For example ``0.102.0-nightly (29c5daa1a 2026-10-07)``. The hash is Cargo's own
commit, unlike the manifest's ``git_commit_hash`` field, which is rust-lang/rust's.
"""


def fetch_manifest(url: str, missing_ok: bool = False) -> Toml | None:
    """Download and parse a Rust channel manifest.

    Args:
        url: The manifest URL.
        missing_ok: Return ``None`` on a 404 instead of raising. Dated
            manifests are missing for days without a release.

    Raises:
        SyncError: If the download or parse fails.
    """
    log(f"fetching {url}")
    try:
        with urllib.request.urlopen(url, timeout=120) as resp:
            return tomllib.loads(resp.read().decode())
    except urllib.error.HTTPError as e:
        if missing_ok and e.code == 404:
            return None
        raise SyncError(f"failed to fetch {url}: {e}") from e
    except Exception as e:
        raise SyncError(f"failed to fetch {url}: {e}") from e


MAX_LAG_SEARCH = 14
"""How many days back a lagged channel target looks for a published manifest."""


def lagged_target(channel: str, lag: int, today: dt.date | None = None) -> tuple[str, Toml | None]:
    """Turn a rolling channel into the dated target ``lag`` days back.

    The dated manifests under ``dist/YYYY-MM-DD/`` never change, so a sync
    from one can be replayed. Days without a release have no manifest, so
    the search walks back until it finds one.

    Args:
        channel: ``nightly`` or ``beta``.
        lag: Days behind today to start from; ``0`` returns ``channel`` as is.
        today: Override for tests.

    Returns:
        The dated target and its manifest, or the input and ``None`` when no
        lag applies.

    Raises:
        SyncError: If no manifest is found within :data:`MAX_LAG_SEARCH` days.
    """
    if lag <= 0 or channel not in ("nightly", "beta"):
        return channel, None
    today = today or dt.datetime.now(dt.UTC).date()
    for back in range(lag, lag + MAX_LAG_SEARCH):
        date = (today - dt.timedelta(days=back)).isoformat()
        manifest = fetch_manifest(f"{DIST}/{date}/channel-rust-{channel}.toml", missing_ok=True)
        if manifest:
            return f"{channel}-{date}", manifest
    raise SyncError(f"no {channel} manifest published in the {MAX_LAG_SEARCH} days before the lag")


def resolve_target(target: str, lag: int = 0) -> Toml:
    """Map a target to a Cargo commit plus channel metadata.

    Args:
        target: ``nightly``, ``beta`` or ``stable``; a dated channel such as
            ``nightly-2026-10-01``; a Rust version such as ``1.99.0``; or a
            Cargo commit hash of at least seven hex digits.
        lag: For ``nightly`` and ``beta``, use the dated manifest this many
            days back instead of the moving pointer; see :func:`lagged_target`.

    Returns:
        A dict with ``target``, ``channel`` and ``commit``. For channels the
        commit is the short hash from the manifest, to be expanded against the
        clone, and ``channel_date``, ``commit_date``, ``cargo_version`` and
        ``rust_version`` are present too.

    Raises:
        SyncError: If the target is not recognised or the manifest is unusable.
    """
    if re.fullmatch(r"[0-9a-f]{7,40}", target):
        return {"target": target, "channel": "commit", "commit": target}
    target, manifest = lagged_target(target, lag)

    if target in ("nightly", "beta", "stable"):
        channel, url = target, f"{DIST}/channel-rust-{target}.toml"
    elif m := re.fullmatch(r"(nightly|beta|stable)-(\d{4}-\d{2}-\d{2})", target):
        channel, url = m[1], f"{DIST}/{m[2]}/channel-rust-{m[1]}.toml"
    elif re.fullmatch(r"\d+\.\d+\.\d+", target):
        channel, url = "stable", f"{DIST}/channel-rust-{target}.toml"
    else:
        raise SyncError(f"unrecognised target {target!r}")

    manifest = manifest or fetch_manifest(url)
    assert manifest is not None
    cargo = manifest["pkg"]["cargo"]["version"]
    m = VERSION_RE.match(cargo)
    if not m:
        raise SyncError(f"cannot parse cargo version {cargo!r} from {url}")
    return {
        "target": target,
        "channel": channel,
        "channel_date": manifest.get("date"),
        "commit": m["sha"],
        "commit_date": m["date"],
        "cargo_version": m["version"],
        "rust_version": manifest["pkg"]["rust"]["version"],
    }


def release_branch(rust_version: str) -> str:
    """Name the release branch for a Rust version string, as Cargo names its own.

    ``1.100.0-beta.13 (a2eb6097d 2026-10-04)`` and ``1.100.1 (...)`` both map
    to ``rust-1.100.0``, the branch Cargo cuts when the beta is branched and
    then ships every point release from.
    """
    m = re.match(r"(\d+)\.(\d+)\.", rust_version)
    if not m:
        raise SyncError(f"cannot parse a Rust version from {rust_version!r}")
    return f"rust-{m[1]}.{m[2]}.0"


def plan(channels: list[str], lag: int) -> list[Toml]:
    """Decide which branch each channel is synced on, and to what.

    ``nightly`` tracks ``main`` through the dated manifest ``lag`` days back.
    ``stable`` tracks the release branch named after its Rust version, as
    Cargo names its own, through the manifest of its exact version; the
    branch is cut when the release first appears. An explicit release such
    as ``1.99.1`` does the same for a release that is no longer the current
    stable. ``beta`` is accepted and maps to the same branch name.

    Returns:
        One dict per sync to run, with ``channel``, ``base``, ``target`` and the
        resolved channel metadata.
    """
    entries: dict[str, Toml] = {}
    for channel in channels:
        explicit = re.fullmatch(r"\d+\.\d+\.\d+", channel) is not None
        info = resolve_target(channel, 0 if channel == "stable" or explicit else lag)
        if channel == "nightly":
            base, target = "main", str(info["target"])
        elif channel == "beta":
            base, target = release_branch(info["rust_version"]), str(info["target"])
        else:
            version = str(info["rust_version"]).split()[0]
            base, target = release_branch(version), version
        if base in entries and info["channel"] != "stable":
            continue
        entries[base] = {**info, "channel": str(info["channel"]), "base": base, "target": target}
    return list(entries.values())
