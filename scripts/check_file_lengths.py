#!/usr/bin/env python3
"""Fail when tracked source files exceed configured line-count budgets."""

from __future__ import annotations

import argparse
import json
import subprocess
import sys
from pathlib import Path


def count_lines(path: Path) -> int:
    with path.open("rb") as handle:
        return sum(1 for _ in handle)


def load_config(path: Path) -> dict:
    with path.open("r", encoding="utf-8") as handle:
        return json.load(handle)


def iter_tracked_files(root: Path, patterns: list[str]) -> list[Path]:
    try:
        result = subprocess.run(
            ["git", "-C", str(root), "ls-files", "-z", "--", *patterns],
            check=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
        )
    except (FileNotFoundError, subprocess.CalledProcessError) as error:
        raise SystemExit(
            "file length lint requires a Git checkout because limits apply to "
            "tracked files; release archives should run this check before packaging"
        ) from error
    return sorted(
        root / item.decode("utf-8")
        for item in result.stdout.split(b"\0")
        if item
    )


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Check repository source files against line-count limits.",
    )
    parser.add_argument(
        "--config",
        default="file-length-limits.json",
        help="Path to the JSON config file (default: %(default)s)",
    )
    args = parser.parse_args()

    repo_root = Path(__file__).resolve().parent.parent
    config_path = (repo_root / args.config).resolve()
    config = load_config(config_path)

    defaults = config.get("defaults", {})
    default_max = int(defaults["max_lines"])
    patterns = list(defaults.get("include", []))
    overrides = {
        str(key): int(value)
        for key, value in config.get("overrides", {}).items()
    }

    files = iter_tracked_files(repo_root, patterns)
    file_lines = {
        path.relative_to(repo_root).as_posix(): count_lines(path)
        for path in files
    }
    failures: list[tuple[str, int, int]] = []
    config_errors: list[str] = []

    for relative, lines in file_lines.items():
        limit = overrides.get(relative, default_max)
        if lines > limit:
            failures.append((relative, lines, limit))

    for relative, limit in overrides.items():
        lines = file_lines.get(relative)
        if lines is None:
            config_errors.append(
                f"{relative}: override does not match a tracked included file",
            )
            continue
        if lines <= default_max:
            config_errors.append(
                f"{relative}: {lines} lines no longer needs an override "
                f"(default {default_max})",
            )
        elif limit > lines:
            config_errors.append(
                f"{relative}: override limit {limit} is above current line "
                f"count {lines}; lower it to {lines}",
            )

    if failures or config_errors:
        print("file length lint failed:", file=sys.stderr)
        for relative, lines, limit in failures:
            print(
                f"  {relative}: {lines} lines (limit {limit})",
                file=sys.stderr,
            )
        for error in config_errors:
            print(f"  {error}", file=sys.stderr)
        print(
            "\nRefactor the file, lower stale overrides, or update "
            "file-length-limits.json explicitly.",
            file=sys.stderr,
        )
        return 1

    print(f"file length lint passed ({len(files)} tracked files checked)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
