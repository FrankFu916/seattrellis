from __future__ import annotations

import json
from zipfile import ZIP_DEFLATED, ZipFile

import pytest

from seattrellis import cli
from seattrellis.io.json_files import InputFileError
from seattrellis.project_bundle import (
    list_recent_projects,
    pack_project,
    restore_project_bundle,
    scan_project_privacy,
)


def test_project_bundle_round_trip_and_privacy_report(tmp_path) -> None:
    paths = cli.init_demo(output_dir=tmp_path, overwrite=True)
    bundle = tmp_path / "class.seattrellis.zip"
    result = pack_project(paths["project"], bundle, include_outputs=False)

    assert result.file_count >= 4
    assert result.privacy.safe_for_public_sharing is False
    report = scan_project_privacy(paths["project"], include_outputs=False)
    assert any(finding.file.endswith("students.csv") for finding in report.findings)

    restored_project = restore_project_bundle(bundle, tmp_path / "restored")
    assert restored_project.exists()
    assert restored_project.parent.joinpath("students.csv").exists()
    assert list_recent_projects(tmp_path / "restored", limit=1)[0].path == restored_project


def test_project_bundle_rejects_path_traversal(tmp_path) -> None:
    bundle = tmp_path / "unsafe.zip"
    manifest = {
        "kind": "seattrellis_project_bundle",
        "format_version": 1,
        "project_file": "../project.seattrellis.json",
        "files": ["../project.seattrellis.json"],
    }
    with ZipFile(bundle, "w", compression=ZIP_DEFLATED) as archive:
        archive.writestr("manifest.json", json.dumps(manifest))
        archive.writestr("../project.seattrellis.json", "{}")

    with pytest.raises(InputFileError, match="Unsafe project bundle path"):
        restore_project_bundle(bundle, tmp_path / "restored")


def test_project_bundle_uses_a_clean_default_name_and_rejects_corrupt_zip(tmp_path) -> None:
    paths = cli.init_demo(output_dir=tmp_path, overwrite=True)
    result = pack_project(paths["project"], include_outputs=False)
    assert result.path.name == "project.seattrellis.zip"

    corrupt = tmp_path / "corrupt.seattrellis.zip"
    corrupt.write_bytes(b"not a zip")
    with pytest.raises(InputFileError, match="Could not restore project bundle"):
        restore_project_bundle(corrupt, tmp_path / "corrupt-restore")
