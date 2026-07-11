#!/usr/bin/env python3
"""Ensure schema bumps freeze the previous version in migration coverage."""

from __future__ import annotations

import json
import re
from pathlib import Path


ROOT = Path(__file__).resolve().parent.parent
MANIFEST = ROOT / "scripts" / "migration-matrix.json"
SCHEMA = ROOT / "src" / "db" / "schema.rs"
TESTS = ROOT / "src" / "db" / "tests" / "filters_and_migrations.rs"


def extract(pattern: str, text: str, description: str) -> str:
    match = re.search(pattern, text, re.MULTILINE | re.DOTALL)
    if match is None:
        raise SystemExit(f"migration matrix check could not find {description}")
    return match.group(1)


def main() -> int:
    manifest = json.loads(MANIFEST.read_text(encoding="utf-8"))
    schema_text = SCHEMA.read_text(encoding="utf-8")
    test_text = TESTS.read_text(encoding="utf-8")
    current = int(
        extract(
            r"CURRENT_SCHEMA_VERSION:\s*i64\s*=\s*(\d+)",
            schema_text,
            "CURRENT_SCHEMA_VERSION",
        )
    )
    recorded = int(manifest["current_schema_version"])
    boundaries = [int(version) for version in manifest["supported_boundary_versions"]]
    covered_text = extract(
        r"migration_backfills_unified_search_documents_from_supported_versions\(\).*?"
        r"source_version in \[([^]]+)\]",
        test_text,
        "supported-version migration test",
    )
    covered = [
        int(value)
        for value in re.findall(r"(?:^|,)\s*(\d+)", covered_text)
    ]

    errors: list[str] = []
    if current != recorded:
        errors.append(
            f"schema is v{current}, but migration-matrix.json records v{recorded}; "
            "freeze the former current version before updating the manifest"
        )
    if current - 1 not in boundaries:
        errors.append(f"v{current - 1} (the predecessor of current v{current}) is not frozen")
    if covered != boundaries:
        errors.append(
            f"migration test covers {covered}, but manifest requires {boundaries}"
        )

    if errors:
        raise SystemExit("migration matrix check failed:\n  " + "\n  ".join(errors))
    print(f"migration matrix check passed (schema v{current}; boundaries {boundaries})")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
