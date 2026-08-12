#!/usr/bin/env python3
"""M6 §9.1 automatic check: the v2 production artifacts must not probe or
load a Python runtime.

Modes:
  --binary <path> [<path> ...]
                   scan one or more built binaries (CLI / app / desktop
                   shell) for Python runtime symbols (libpython, PyO3, the
                   native shim, embedded interpreters) and v1 stack references.
  --tree           scan the production source tree (crates/, app/,
                   clients/web/dist) for Python references in build
                   manifests and vendored runtime files.
  --archive <path> [<path> ...]
                   scan one or more distribution archives (zip/tar) for
                   Python payload files.

Exit 0 = clean, 1 = problems found. The alpha.2 tree legitimately keeps
src/seattrellis/ and the PyO3 shim as oracle/compat; --tree only flags the
production tree, and M6 deletes the oracle paths entirely (the release
gate then runs with a retired tree).
"""

from __future__ import annotations

import argparse
import re
import sys
import tarfile
import zipfile
from pathlib import Path

# Runtime-embedding symbols: the mere mention of these in a built binary
# means the process can load Python code. Case-sensitive (matches the
# actual symbols), so ordinary English strings never trip it.
BINARY_PYTHON_SYMBOLS = [
    b"libpython",
    b"Py_Initialize",
    b"PyImport_Import",
    b"PyEval_EvalCode",
    b"pyo3",
    b"seattrellis_native",
    b"site-packages",
    b"python3.dll",
]

# v1 stack references that must never appear in a v2 artifact (plan §发布
# 红线: no Python, Pydantic, FastAPI, Streamlit, OR-Tools, PyO3, pywebview).
V1_STACK_REFERENCES = [
    b"pydantic",
    b"fastapi",
    b"streamlit",
    b"ortools",
    b"pywebview",
]

# Manifest references inside the production tree (Cargo.toml / lockfile /
# build scripts) that would pull the Python stack into a v2 binary.
TREE_PYTHON_DEPENDENCIES = [
    "pyo3",
    "seattrellis_native",
    "pydantic",
    "fastapi",
    "streamlit",
    "ortools",
    "pywebview",
]


def scan_binary(path: Path) -> list[str]:
    data = path.read_bytes()
    problems: list[str] = []
    for token in BINARY_PYTHON_SYMBOLS + V1_STACK_REFERENCES:
        if token in data:
            problems.append(f"binary contains Python runtime symbol {token!r}")
    return problems


def scan_archive(path: Path) -> list[str]:
    try:
        if path.suffix == ".whl" or zipfile.is_zipfile(path):
            names = zipfile.ZipFile(path).namelist()
        elif tarfile.is_tarfile(path):
            names = tarfile.open(path).getnames()
        else:
            return [f"unsupported archive format: {path}"]
    except (OSError, zipfile.BadZipFile, tarfile.TarError) as error:
        return [f"could not read archive {path}: {error}"]
    problems: list[str] = []
    for name in names:
        lowered = name.lower()
        if lowered.endswith(".py") or "/site-packages/" in lowered:
            problems.append(f"archive carries Python payload: {name}")
    return problems


def scan_tree(root: Path) -> list[str]:
    problems: list[str] = []
    manifests = [
        root / "Cargo.toml",
        root / "Cargo.lock",
        *sorted((root / "crates").glob("*/Cargo.toml")),
        *sorted((root / "app").glob("*/Cargo.toml")),
    ]
    for manifest in manifests:
        if not manifest.is_file():
            continue
        text = manifest.read_text(encoding="utf-8", errors="replace")
        for dependency in TREE_PYTHON_DEPENDENCIES:
            if re.search(rf"(?m)^\s*{re.escape(dependency)}\s*=", text):
                problems.append(
                    f"{manifest.relative_to(root)} depends on {dependency}"
                )
    # The production build script must embed the React workbench, never
    # serve from a Python web root (M6 decoupling, build.rs).
    server_build = root / "crates" / "seattrellis-server" / "build.rs"
    if server_build.is_file():
        text = server_build.read_text(encoding="utf-8", errors="replace")
        if "clients/web/dist" not in text:
            problems.append(
                "seattrellis-server/build.rs no longer embeds clients/web/dist"
            )
    return problems


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--binary", action="extend", nargs="+", default=[], metavar="PATH"
    )
    parser.add_argument(
        "--archive", action="extend", nargs="+", default=[], metavar="PATH"
    )
    parser.add_argument("--tree", action="store_true")
    args = parser.parse_args()

    problems: list[str] = []
    for raw in args.binary:
        path = Path(raw)
        if not path.is_file():
            problems.append(f"binary not found: {path}")
            continue
        problems.extend(scan_binary(path))
    for raw in args.archive:
        problems.extend(scan_archive(Path(raw)))
    if args.tree:
        problems.extend(scan_tree(Path.cwd()))

    if problems:
        print("Python runtime references found:")
        for problem in sorted(set(problems)):
            print(f"  - {problem}")
        return 1
    print("clean: no Python runtime references")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
