#!/usr/bin/env python3
"""Verify or regenerate the bundled plans SHA-256 manifest."""

from __future__ import annotations

import argparse
import hashlib
import subprocess
import tempfile
from pathlib import Path


ROOT = Path(__file__).resolve().parent.parent
PLANS = ROOT / "plans"
MANIFEST = PLANS / "MANIFEST.sha256"


class ManifestError(Exception):
    """The tracked bundle cannot be represented by the manifest."""


def tracked_bundle_files() -> list[Path]:
    try:
        result = subprocess.run(
            ["git", "-C", str(ROOT), "ls-files", "-z", "--", "plans"],
            check=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
        )
    except (FileNotFoundError, subprocess.CalledProcessError) as error:
        raise ManifestError(
            "plans manifest check requires a Git checkout to identify bundled files"
        ) from error

    files: list[Path] = []
    missing: list[str] = []
    unsupported: list[str] = []
    for encoded in result.stdout.split(b"\0"):
        if not encoded:
            continue
        relative = Path(encoded.decode("utf-8"))
        path = ROOT / relative
        if path == MANIFEST:
            continue
        if not path.exists():
            missing.append(relative.as_posix())
        elif path.is_symlink() or not path.is_file():
            unsupported.append(relative.as_posix())
        else:
            files.append(path)

    if missing:
        raise ManifestError("tracked plans files are missing: " + ", ".join(missing))
    if unsupported:
        raise ManifestError(
            "tracked plans entries are not regular files: " + ", ".join(unsupported)
        )
    return sorted(files, key=lambda path: path.relative_to(PLANS).as_posix())


def manifest_text(files: list[Path], plans_dir: Path = PLANS) -> str:
    lines: list[str] = []
    for path in files:
        if not path.exists():
            raise ManifestError(f"tracked plans file is missing: {path}")
        relative = path.relative_to(plans_dir).as_posix()
        digest = hashlib.sha256(path.read_bytes()).hexdigest()
        lines.append(f"{digest}  ./{relative}")
    return "\n".join(lines) + "\n"


def verify_manifest(actual: str, expected: str) -> None:
    if actual != expected:
        raise ManifestError(
            "manifest content is stale; run "
            "`python3 scripts/check_plans_manifest.py --write`"
        )


def self_test() -> None:
    with tempfile.TemporaryDirectory(prefix="clipmem-plans-manifest-") as directory:
        plans = Path(directory)
        tracked = plans / "plan.md"
        tracked.write_text("first\n", encoding="utf-8")
        (plans / ".DS_Store").write_bytes(b"ignored Finder metadata")
        baseline = manifest_text([tracked], plans)
        assert ".DS_Store" not in baseline
        tracked.write_text("changed\n", encoding="utf-8")
        try:
            verify_manifest(baseline, manifest_text([tracked], plans))
        except ManifestError as error:
            assert "stale" in str(error)
        else:
            raise AssertionError("tracked content mutation should fail verification")
        tracked.write_text("first\n", encoding="utf-8")
        added = plans / "added.md"
        added.write_text("new tracked plan\n", encoding="utf-8")
        try:
            verify_manifest(baseline, manifest_text([added, tracked], plans))
        except ManifestError as error:
            assert "stale" in str(error)
        else:
            raise AssertionError("tracked file addition should fail verification")
        tracked.unlink()
        try:
            manifest_text([tracked], plans)
        except ManifestError as error:
            assert "missing" in str(error)
        else:
            raise AssertionError("missing tracked file should fail manifest generation")


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Check plans/MANIFEST.sha256 against every bundled plans file.",
    )
    mode = parser.add_mutually_exclusive_group()
    mode.add_argument(
        "--write",
        action="store_true",
        help="Regenerate the manifest in canonical path order.",
    )
    mode.add_argument(
        "--self-test",
        action="store_true",
        help="Test ignored-noise, mutation, and missing-file behavior.",
    )
    args = parser.parse_args()
    if args.self_test:
        self_test()
        print("plans manifest self-test passed")
        return 0
    try:
        expected = manifest_text(tracked_bundle_files())
    except ManifestError as error:
        raise SystemExit(f"plans manifest check failed: {error}") from error

    if args.write:
        MANIFEST.write_text(expected, encoding="utf-8")
        print(f"wrote {MANIFEST.relative_to(ROOT)}")
        return 0

    actual = MANIFEST.read_text(encoding="utf-8")
    try:
        verify_manifest(actual, expected)
    except ManifestError as error:
        raise SystemExit(f"plans manifest check failed: {error}") from error
    print(f"plans manifest check passed ({len(expected.splitlines())} files)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
