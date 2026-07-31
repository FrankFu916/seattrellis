"""Build the optional desktop workbench with PyInstaller.

The script deliberately invokes PyInstaller as a module so the builder used by
CI is the one installed in the active Python environment. It creates an
onedir bundle; platform-specific installers, signing, and notarization remain
separate release steps.
"""

from __future__ import annotations

import argparse
import os
import subprocess
import sys
from collections.abc import Sequence
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
SPEC = ROOT / "packaging" / "desktop" / "SeatTrellis.spec"


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description="Build the SeatTrellis desktop bundle.")
    parser.add_argument(
        "--output-dir",
        type=Path,
        default=ROOT / "dist" / "desktop",
        help="Directory receiving the onedir bundle.",
    )
    parser.add_argument(
        "--work-dir",
        type=Path,
        default=ROOT / "build" / "desktop",
        help="Temporary PyInstaller work directory.",
    )
    return parser


def main(argv: Sequence[str] | None = None) -> int:
    args = build_parser().parse_args(argv)
    args.output_dir.parent.mkdir(parents=True, exist_ok=True)
    args.work_dir.parent.mkdir(parents=True, exist_ok=True)
    command = [
        sys.executable,
        "-m",
        "PyInstaller",
        "--noconfirm",
        "--clean",
        "--distpath",
        str(args.output_dir),
        "--workpath",
        str(args.work_dir),
        str(SPEC),
    ]
    environment = os.environ.copy()
    # Keep PyInstaller's cache inside the declared work directory. This makes
    # CI and local builds reproducible and avoids mutating a user's global
    # application-support directory.
    environment.setdefault(
        "PYINSTALLER_CONFIG_DIR",
        str(args.work_dir / "pyinstaller-cache"),
    )
    completed = subprocess.run(command, cwd=ROOT, env=environment, check=False)
    return completed.returncode


if __name__ == "__main__":  # pragma: no cover - exercised by CI builders.
    raise SystemExit(main())
