from __future__ import annotations

from zipfile import ZipFile

import pytest

from scripts.archive_desktop import archive_bundle


def test_archive_bundle_is_deterministic_and_preserves_executable_mode(tmp_path) -> None:
    bundle = tmp_path / "SeatTrellis"
    executable = bundle / "SeatTrellis"
    static = bundle / "_internal" / "seattrellis" / "web_static" / "index.html"
    executable.parent.mkdir(parents=True)
    static.parent.mkdir(parents=True)
    executable.write_bytes(b"binary")
    static.write_text("<html></html>", encoding="utf-8")
    executable.chmod(0o755)

    first = archive_bundle(
        bundle,
        tmp_path / "release",
        platform_name="macOS",
        version="v1.8.2",
    )
    first_bytes = first.read_bytes()
    first.unlink()
    second = archive_bundle(
        bundle,
        tmp_path / "release",
        platform_name="macOS",
        version="v1.8.2",
    )

    assert first_bytes == second.read_bytes()
    with ZipFile(second) as archive:
        assert archive.namelist() == [
            "SeatTrellis/SeatTrellis",
            "SeatTrellis/_internal/seattrellis/web_static/index.html",
        ]
        assert (archive.getinfo("SeatTrellis/SeatTrellis").external_attr >> 16) & 0o111


def test_archive_bundle_rejects_missing_or_unsafe_inputs(tmp_path) -> None:
    with pytest.raises(ValueError, match="not found"):
        archive_bundle(
            tmp_path / "missing",
            tmp_path / "release",
            platform_name="macOS",
            version="manual",
        )

    bundle = tmp_path / "SeatTrellis"
    bundle.mkdir()
    with pytest.raises(ValueError, match="platform"):
        archive_bundle(
            bundle,
            tmp_path / "release",
            platform_name="mac/os",
            version="manual",
        )

    target = bundle / "target.dylib"
    target.write_bytes(b"library")
    link = bundle / "link.dylib"
    try:
        link.symlink_to(target)
    except OSError:
        pytest.skip("Symlinks are not available on this filesystem")
    archive = archive_bundle(
        bundle,
        tmp_path / "release",
        platform_name="macOS",
        version="manual",
    )
    with ZipFile(archive) as contents:
        assert contents.read("SeatTrellis/link.dylib") == b"library"


def test_archive_bundle_expands_safe_directory_symlinks(tmp_path) -> None:
    bundle = tmp_path / "SeatTrellis"
    target = bundle / "Python.framework" / "Versions" / "Current" / "Resources"
    target.mkdir(parents=True)
    (target / "version.txt").write_text("3.14", encoding="utf-8")
    link = bundle / "Python.framework" / "Resources"
    try:
        link.symlink_to(target, target_is_directory=True)
    except OSError:
        pytest.skip("Symlinks are not available on this filesystem")

    archive = archive_bundle(
        bundle,
        tmp_path / "release",
        platform_name="macOS",
        version="manual",
    )

    with ZipFile(archive) as contents:
        assert contents.read(
            "SeatTrellis/Python.framework/Resources/version.txt"
        ) == b"3.14"
