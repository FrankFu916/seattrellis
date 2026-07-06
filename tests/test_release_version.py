from pathlib import Path

import pytest

from scripts.check_release_version import (
    package_version,
    pyproject_version,
    validate_release,
)


def _write_project(root: Path, metadata_version: str, runtime_version: str) -> None:
    (root / "src" / "seattrellis").mkdir(parents=True)
    (root / "pyproject.toml").write_text(
        f'[project]\nname = "seattrellis"\nversion = "{metadata_version}"\n',
        encoding="utf-8",
    )
    (root / "src" / "seattrellis" / "__init__.py").write_text(
        f'__version__: str = "{runtime_version}"\n',
        encoding="utf-8",
    )


def test_reads_versions_and_accepts_matching_release_tag(tmp_path) -> None:
    _write_project(tmp_path, "1.3.0", "1.3.0")

    assert pyproject_version(tmp_path) == "1.3.0"
    assert package_version(tmp_path) == "1.3.0"
    assert validate_release(tmp_path) == "1.3.0"
    assert validate_release(tmp_path, "v1.3.0") == "1.3.0"


def test_rejects_mismatched_runtime_version(tmp_path) -> None:
    _write_project(tmp_path, "1.3.0", "1.2.3")

    with pytest.raises(ValueError, match="Package version mismatch"):
        validate_release(tmp_path)


def test_rejects_mismatched_release_tag(tmp_path) -> None:
    _write_project(tmp_path, "1.3.0", "1.3.0")

    with pytest.raises(ValueError, match="does not match"):
        validate_release(tmp_path, "v1.2.3")
