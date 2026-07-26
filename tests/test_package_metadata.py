from __future__ import annotations

import tomllib
from pathlib import Path


def test_native_build_tool_is_not_advertised_as_a_runtime_extra() -> None:
    root = Path(__file__).resolve().parents[1]
    with (root / "pyproject.toml").open("rb") as project_file:
        project = tomllib.load(project_file)

    extras = project["project"]["optional-dependencies"]
    assert "native" not in extras
    assert all(
        not dependency.lower().startswith("maturin")
        for dependencies in extras.values()
        for dependency in dependencies
    )
