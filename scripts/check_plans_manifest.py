#!/usr/bin/env python3
"""Verify or regenerate the bundled plans SHA-256 manifest."""

from __future__ import annotations

import argparse
import hashlib
from pathlib import Path


ROOT = Path(__file__).resolve().parent.parent
PLANS = ROOT / "plans"
MANIFEST = PLANS / "MANIFEST.sha256"


def manifest_text() -> str:
    lines: list[str] = []
    for path in sorted(PLANS.rglob("*")):
        if not path.is_file() or path == MANIFEST:
            continue
        relative = path.relative_to(PLANS).as_posix()
        digest = hashlib.sha256(path.read_bytes()).hexdigest()
        lines.append(f"{digest}  ./{relative}")
    return "\n".join(lines) + "\n"


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Check plans/MANIFEST.sha256 against every bundled plans file.",
    )
    parser.add_argument(
        "--write",
        action="store_true",
        help="Regenerate the manifest in canonical path order.",
    )
    args = parser.parse_args()
    expected = manifest_text()

    if args.write:
        MANIFEST.write_text(expected, encoding="utf-8")
        print(f"wrote {MANIFEST.relative_to(ROOT)}")
        return 0

    actual = MANIFEST.read_text(encoding="utf-8")
    if actual != expected:
        raise SystemExit(
            "plans manifest check failed; run "
            "`python3 scripts/check_plans_manifest.py --write`"
        )
    print(f"plans manifest check passed ({len(expected.splitlines())} files)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
