"""Standalone entry point used by desktop launchers and package builders."""

from __future__ import annotations

import argparse
from collections.abc import Sequence

from seattrellis import __version__
from seattrellis.desktop import DesktopOptions, run_desktop_app


def build_parser() -> argparse.ArgumentParser:
    """Build the small, stable command line accepted by desktop packages."""

    parser = argparse.ArgumentParser(
        prog="seattrellis-desktop",
        description="Open the SeatTrellis desktop workbench.",
    )
    parser.add_argument(
        "--width",
        type=int,
        default=1280,
        help="Initial window width in pixels (default: 1280).",
    )
    parser.add_argument(
        "--height",
        type=int,
        default=900,
        help="Initial window height in pixels (default: 900).",
    )
    parser.add_argument(
        "--title",
        default="SeatTrellis",
        help="Window title (default: SeatTrellis).",
    )
    parser.add_argument(
        "--version",
        "-V",
        action="version",
        version=f"seattrellis-desktop {__version__}",
    )
    return parser


def main(argv: Sequence[str] | None = None) -> int:
    """Parse desktop options, launch the window, and return a process status."""

    args = build_parser().parse_args(argv)
    run_desktop_app(
        options=DesktopOptions(
            width=args.width,
            height=args.height,
            title=args.title,
        )
    )
    return 0


if __name__ == "__main__":  # pragma: no cover - exercised by package launchers.
    raise SystemExit(main())
