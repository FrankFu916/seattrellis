"""Create a deterministic ZIP from a PyInstaller onedir bundle."""

from __future__ import annotations

import argparse
import hashlib
import platform as platform_module
import re
from collections.abc import Sequence
from pathlib import Path
from zipfile import ZIP_DEFLATED, ZipFile, ZipInfo


_SAFE_PART = re.compile(r"^[A-Za-z0-9][A-Za-z0-9._-]*$")


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description="Archive a SeatTrellis desktop bundle.")
    parser.add_argument("--bundle-dir", type=Path, required=True)
    parser.add_argument(
        "--output-dir",
        type=Path,
        default=Path("dist") / "release",
    )
    parser.add_argument("--platform", default=platform_module.system())
    parser.add_argument("--version", default="manual")
    return parser


def archive_bundle(
    bundle_dir: Path,
    output_dir: Path,
    *,
    platform_name: str,
    version: str,
) -> Path:
    """Archive ``bundle_dir`` and return the resulting ZIP path."""

    bundle = bundle_dir.expanduser().resolve()
    if not bundle.is_dir():
        raise ValueError(f"Desktop bundle directory not found: {bundle}")
    safe_platform = _safe_part(platform_name, "platform")
    safe_version = _safe_part(version, "version")
    destination = output_dir.expanduser().resolve()
    destination.mkdir(parents=True, exist_ok=True)
    archive_path = destination / f"SeatTrellis-{safe_platform}-{safe_version}.zip"

    # Sort by the POSIX archive path rather than ``Path`` ordering.  ``Path``
    # comparisons use platform-specific rules, so the same bundle could have
    # a different member order on Windows and Unix hosts.  PyInstaller's
    # macOS framework sometimes contains directory symlinks (for example
    # ``Python.framework/Resources``); ``_collect_files`` expands those links
    # into regular archive entries while still rejecting links that leave the
    # bundle.
    files = sorted(
        _collect_files(bundle),
        key=lambda entry: entry[0].as_posix(),
    )
    with ZipFile(archive_path, "w", compression=ZIP_DEFLATED, compresslevel=9) as archive:
        for relative_path, source in files:
            if not source.is_file():
                raise ValueError(
                    f"Desktop bundle contains an unsafe entry: {bundle / relative_path}"
                )
            relative = Path("SeatTrellis") / relative_path
            info = ZipInfo(relative.as_posix())
            info.date_time = (1980, 1, 1, 0, 0, 0)
            info.compress_type = ZIP_DEFLATED
            info.create_system = 3
            mode = source.stat().st_mode & 0o777
            # Windows does not expose POSIX execute bits through ``stat``.
            # PyInstaller's launcher is the bundle root executable, so retain
            # the executable bit for that well-known entry when the host API
            # reports no execute permissions.
            if not mode & 0o111 and _is_bundle_launcher(bundle / relative_path, bundle):
                mode = 0o755
            info.external_attr = mode << 16
            archive.writestr(info, source.read_bytes())
    return archive_path


def _collect_files(bundle: Path) -> list[tuple[Path, Path]]:
    """Return logical archive paths and their safe source files.

    Directory symlinks are expanded instead of being written as symlink
    metadata.  This keeps the extracted desktop package usable on systems
    where ZIP extraction does not recreate symlinks, while the containment
    check prevents a bundle from smuggling files from outside its root.
    """

    entries: list[tuple[Path, Path]] = []

    def visit(path: Path, relative: Path, active_directories: set[Path]) -> None:
        if path.is_symlink():
            resolved = path.resolve()
            _ensure_inside_bundle(resolved, bundle, path)
            if resolved.is_dir():
                if resolved in active_directories:
                    raise ValueError(f"Desktop bundle contains a symlink loop: {path}")
                visit_directory(resolved, relative, active_directories)
                return
            if resolved.is_file():
                entries.append((relative, resolved))
                return
            raise ValueError(f"Desktop bundle contains an unsafe entry: {path}")

        if path.is_dir():
            visit_directory(path, relative, active_directories)
            return
        if path.is_file():
            entries.append((relative, path))
            return
        raise ValueError(f"Desktop bundle contains an unsafe entry: {path}")

    def visit_directory(path: Path, relative: Path, active_directories: set[Path]) -> None:
        resolved_directory = path.resolve()
        _ensure_inside_bundle(resolved_directory, bundle, path)
        next_active = {*active_directories, resolved_directory}
        for child in path.iterdir():
            visit(child, relative / child.name, next_active)

    visit_directory(bundle, Path(), set())
    return entries


def _ensure_inside_bundle(path: Path, bundle: Path, original: Path) -> None:
    try:
        path.relative_to(bundle)
    except ValueError as exc:
        raise ValueError(
            f"Desktop bundle symlink points outside the bundle: {original}"
        ) from exc


def sha256(path: Path) -> str:
    """Return the lowercase SHA-256 digest of a file."""

    digest = hashlib.sha256()
    with path.open("rb") as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def _safe_part(value: str, label: str) -> str:
    if not _SAFE_PART.fullmatch(value):
        raise ValueError(f"{label} must contain only letters, numbers, '.', '_' or '-'.")
    return value


def _is_bundle_launcher(path: Path, bundle: Path) -> bool:
    """Return whether ``path`` is the top-level PyInstaller launcher."""

    return path.parent == bundle and (
        path.name == bundle.name or path.suffix.lower() == ".exe"
    )


def main(argv: Sequence[str] | None = None) -> int:
    args = build_parser().parse_args(argv)
    archive = archive_bundle(
        args.bundle_dir,
        args.output_dir,
        platform_name=args.platform,
        version=args.version,
    )
    print(f"{archive} {sha256(archive)}")
    return 0


if __name__ == "__main__":  # pragma: no cover - exercised by release workflow.
    raise SystemExit(main())
