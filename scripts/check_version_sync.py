#!/usr/bin/env python3
"""Fail when package and app bundle versions drift."""

from __future__ import annotations

import re
import sys
from pathlib import Path


MARKETING_VERSION_RE = re.compile(r"MARKETING_VERSION = ([^;]+);")
TOML_SECTION_RE = re.compile(r"^\s*\[([^\]]+)\]\s*$")
TOML_STRING_RE = re.compile(r'^\s*([A-Za-z0-9_-]+)\s*=\s*"([^"]+)"\s*$')


def cargo_version(repo_root: Path) -> str:
    cargo_toml = repo_root / "Cargo.toml"
    section: str | None = None

    for line in cargo_toml.read_text(encoding="utf-8").splitlines():
        section_match = TOML_SECTION_RE.match(line)
        if section_match:
            section = section_match.group(1)
            continue
        if section != "package":
            continue

        value_match = TOML_STRING_RE.match(line)
        if value_match and value_match.group(1) == "version":
            return value_match.group(2)

    raise RuntimeError(f"failed to read [package] version from {cargo_toml}")


def xcode_marketing_versions(project_file: Path) -> list[str]:
    text = project_file.read_text(encoding="utf-8")
    return MARKETING_VERSION_RE.findall(text)


def main() -> int:
    repo_root = Path(__file__).resolve().parent.parent
    project_file = (
        repo_root
        / "macos"
        / "ClipmemMenuBar"
        / "ClipmemMenuBar.xcodeproj"
        / "project.pbxproj"
    )
    expected = cargo_version(repo_root)
    versions = xcode_marketing_versions(project_file)

    if not versions:
        print(
            f"version sync failed: no MARKETING_VERSION entries found in {project_file}",
            file=sys.stderr,
        )
        return 1

    mismatches = sorted({version for version in versions if version != expected})
    if mismatches:
        print("version sync failed:", file=sys.stderr)
        print(f"  Cargo.toml package.version: {expected}", file=sys.stderr)
        print(
            "  Xcode MARKETING_VERSION values: "
            f"{', '.join(sorted(set(versions)))}",
            file=sys.stderr,
        )
        print(
            "\nUpdate every MARKETING_VERSION in "
            "macos/ClipmemMenuBar/ClipmemMenuBar.xcodeproj/project.pbxproj "
            "to match Cargo.toml.",
            file=sys.stderr,
        )
        return 1

    print(f"version sync passed (Cargo.toml and menu bar app are {expected})")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
