#!/usr/bin/env python3
"""Reject release assets built from sources with a different product version."""
from __future__ import annotations

import argparse
import json
import re
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def release_version(tag: str) -> str:
    tag = tag.removeprefix("refs/tags/")
    match = re.fullmatch(r"(?:v|desktop-v)(2\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?)", tag)
    if not match:
        raise ValueError("expected v2.x.y or desktop-v2.x.y release tag")
    return match.group(1)


def problems(root: Path, tag: str) -> list[str]:
    expected = release_version(tag)
    manifests = sorted((root / "crates").glob("*/Cargo.toml")) + [
        root / "xtask/Cargo.toml", root / "app/Cargo.toml", root / "app/src-tauri/Cargo.toml",
    ]
    mismatches = []
    for path in manifests:
        version = tomllib.loads(path.read_text(encoding="utf-8"))["package"]["version"]
        if version != expected:
            mismatches.append(f"{path.relative_to(root)}: {version} != {expected}")
    for relative in ["clients/web/package.json", "website/package.json", "app/src-tauri/tauri.conf.json"]:
        document = json.loads((root / relative).read_text(encoding="utf-8"))
        if document["version"] != expected:
            mismatches.append(f"{relative}: {document['version']} != {expected}")
        if relative.endswith("package.json"):
            lock_path = root / relative.replace("package.json", "package-lock.json")
            lock = json.loads(lock_path.read_text(encoding="utf-8"))
            for field, version in [("version", lock["version"]), ("packages[''].version", lock["packages"][""]["version"])]:
                if version != expected:
                    mismatches.append(f"{lock_path.relative_to(root)}: {field} {version} != {expected}")
        if relative.endswith("tauri.conf.json"):
            wix = document.get("bundle", {}).get("windows", {}).get("wix", {}).get("version")
            numeric = expected.split("-", 1)[0] + ".0"
            if wix != numeric:
                mismatches.append(f"{relative}: WiX {wix} != {numeric}")
    return mismatches


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("tag")
    args = parser.parse_args()
    try:
        errors = problems(ROOT, args.tag)
    except (ValueError, KeyError, OSError) as error:
        parser.error(str(error))
    if errors:
        print("Release version mismatch:\n" + "\n".join(errors))
        return 1
    print(f"Release versions match {args.tag}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
