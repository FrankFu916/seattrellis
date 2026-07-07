#!/usr/bin/env python3
"""Validate that package versions and an optional release tag agree."""

from __future__ import annotations

import argparse
import ast
import sys
import tomllib
from pathlib import Path


def pyproject_version(root: Path) -> str:
    with (root / "pyproject.toml").open("rb") as file:
        document = tomllib.load(file)
    version = document.get("project", {}).get("version")
    if not isinstance(version, str) or not version.strip():
        raise ValueError("pyproject.toml must define a non-empty project.version")
    return version.strip()


def package_version(root: Path) -> str:
    init_path = root / "src" / "seattrellis" / "__init__.py"
    module = ast.parse(init_path.read_text(encoding="utf-8"), filename=str(init_path))
    for node in module.body:
        if not isinstance(node, (ast.Assign, ast.AnnAssign)):
            continue
        targets = node.targets if isinstance(node, ast.Assign) else [node.target]
        if not any(
            isinstance(target, ast.Name) and target.id == "__version__"
            for target in targets
        ):
            continue
        value = node.value
        if isinstance(value, ast.Constant) and isinstance(value.value, str):
            return value.value.strip()
        raise ValueError("__version__ must be assigned a string literal")
    raise ValueError(f"{init_path} does not define __version__")


def validate_release(root: Path, tag: str | None = None) -> str:
    metadata_version = pyproject_version(root)
    runtime_version = package_version(root)
    if runtime_version != metadata_version:
        raise ValueError(
            "Package version mismatch: "
            f"pyproject.toml={metadata_version!r}, "
            f"seattrellis.__version__={runtime_version!r}"
        )

    if tag is not None:
        normalized_tag = tag.strip()
        expected_tag = f"v{metadata_version}"
        if normalized_tag != expected_tag:
            raise ValueError(
                f"Release tag {normalized_tag!r} does not match {expected_tag!r}"
            )
    return metadata_version


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--tag",
        help="Optional Git release tag, which must equal v<project.version>.",
    )
    parser.add_argument(
        "--print-version",
        action="store_true",
        help="Print only the validated project version.",
    )
    args = parser.parse_args()
    root = Path(__file__).resolve().parents[1]

    try:
        version = validate_release(root, args.tag)
    except (OSError, SyntaxError, KeyError, TypeError, ValueError) as exc:
        print(f"Release version check failed: {exc}", file=sys.stderr)
        return 1

    if args.print_version:
        print(version)
    else:
        suffix = f" and release tag {args.tag}" if args.tag else ""
        print(f"Release version check passed for {version}{suffix}.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
