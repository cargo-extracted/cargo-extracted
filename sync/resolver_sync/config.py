"""Reading the tool configuration and pin file, and writing simple TOML."""

from __future__ import annotations

import json
import tomllib
from pathlib import Path

from sync.resolver_sync.common import CONFIG, PIN, SyncError, Toml


def read_toml(path: Path) -> Toml:
    """Parse a TOML file."""
    with path.open("rb") as f:
        return tomllib.load(f)


def load_config() -> Toml:
    """Load ``sync/config.toml``."""
    return read_toml(CONFIG)


def load_pin() -> Toml | None:
    """Load ``upstream.toml``, or return ``None`` if no sync has happened yet."""
    if not PIN.exists():
        return None
    return read_toml(PIN)


def upstream_url(cfg: Toml) -> str:
    """Return the canonical upstream URL.

    The configured repository may be a local checkout for development; files
    that end up in the repository must still point at the public URL.
    """
    repo = str(cfg["upstream"]["repo"])
    if Path(repo).expanduser().exists():
        return "https://github.com/rust-lang/cargo.git"
    return repo


def toml_value(v: object) -> str:
    """Serialize a scalar, list or dict as a TOML value.

    Dicts become inline tables, which is all the generated files need. Strings
    use JSON escaping, a subset of TOML basic strings.

    Raises:
        SyncError: If the value has a type TOML cannot represent here.
    """
    if isinstance(v, bool):
        return "true" if v else "false"
    if isinstance(v, int):
        return str(v)
    if isinstance(v, str):
        return json.dumps(v)
    if isinstance(v, list):
        return "[" + ", ".join(toml_value(x) for x in v) + "]"
    if isinstance(v, dict):
        return "{ " + ", ".join(f"{k} = {toml_value(x)}" for k, x in v.items()) + " }"
    raise SyncError(f"cannot serialize {v!r} as TOML")
