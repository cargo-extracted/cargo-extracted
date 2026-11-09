#!/usr/bin/env python3
"""Entry point for the sync tooling; see ``sync.resolver_sync.cli`` for the commands.

Kept as a script so ``python3 sync/sync.py <command>`` works from any
directory without installing anything. Imports are rooted at the repository,
so the repository root goes first on the import path; it must come before the
script's own directory, where this file would otherwise shadow the ``sync``
package.
"""

from __future__ import annotations

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))

from sync.resolver_sync.cli import main

if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
