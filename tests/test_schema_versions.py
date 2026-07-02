from __future__ import annotations

import json
from pathlib import Path

import pytest

from seattrellis.io.json_files import (
    InputFileError,
    load_candidate_set,
    load_plan_comparison_report,
    load_snapshot,
)
from seattrellis.io.project import load_project, write_project
from seattrellis.models.project import SeatTrellisProject
from seattrellis.schema import (
    CANDIDATE_SCHEMA_VERSION,
    PROJECT_SCHEMA_VERSION,
    SNAPSHOT_SCHEMA_VERSION,
)
from seattrellis.web.workflow import solve_for_web


def test_existing_snapshot_examples_remain_readable() -> None:
    snapshot = load_snapshot("examples/history/week1.snapshot.json")

    assert snapshot.schema_version == SNAPSHOT_SCHEMA_VERSION


def test_current_artifact_versions_round_trip(tmp_path) -> None:
    result = solve_for_web(
        students_path="examples/students.csv",
        layout_path="examples/classroom.json",
        preset_name="random",
        output_dir=tmp_path,
        candidate_count=2,
    )

    candidate_set = load_candidate_set(result.artifact_path)
    report = load_plan_comparison_report(result.report_path)
    project = SeatTrellisProject(
        students="students.csv",
        layout="classroom.json",
        rules="rules.json",
    )
    project_path = write_project(project, tmp_path / "project.json")

    assert candidate_set.schema_version == CANDIDATE_SCHEMA_VERSION
    assert report.schema_version == CANDIDATE_SCHEMA_VERSION
    assert load_project(project_path).schema_version == PROJECT_SCHEMA_VERSION


def test_unknown_snapshot_schema_is_rejected(tmp_path) -> None:
    data = json.loads(
        Path("examples/history/week1.snapshot.json").read_text(encoding="utf-8")
    )
    data["schema_version"] = "2.0"
    path = tmp_path / "unsupported.json"
    path.write_text(json.dumps(data), encoding="utf-8")

    with pytest.raises(InputFileError, match="Unsupported snapshot schema_version"):
        load_snapshot(path)


def test_unknown_candidate_and_report_schemas_are_rejected(tmp_path) -> None:
    result = solve_for_web(
        students_path="examples/students.csv",
        layout_path="examples/classroom.json",
        preset_name="random",
        output_dir=tmp_path / "solve",
        candidate_count=2,
    )

    for source, loader, artifact in [
        (result.artifact_path, load_candidate_set, "candidate set"),
        (result.report_path, load_plan_comparison_report, "plan comparison report"),
    ]:
        data = json.loads(source.read_text(encoding="utf-8"))
        data["schema_version"] = "9.9"
        path = tmp_path / f"{source.stem}-unsupported.json"
        path.write_text(json.dumps(data), encoding="utf-8")
        with pytest.raises(
            InputFileError,
            match=rf"Unsupported {artifact} schema_version",
        ):
            loader(path)


@pytest.mark.parametrize("version", [2, "1", True])
def test_unknown_or_wrongly_typed_project_schema_is_rejected(
    tmp_path, version
) -> None:
    path = tmp_path / "project.json"
    path.write_text(
        json.dumps(
            {
                "kind": "seattrellis_project",
                "schema_version": version,
                "students": "students.csv",
                "layout": "classroom.json",
                "rules": "rules.json",
            }
        ),
        encoding="utf-8",
    )

    with pytest.raises(InputFileError, match="Unsupported project schema_version"):
        load_project(path)
