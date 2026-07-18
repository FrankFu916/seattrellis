from __future__ import annotations

import json
import subprocess
import sys
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
    EDITOR_PROTOCOL_VERSION,
    JSON_SCHEMA_DRAFT,
    PROJECT_SCHEMA_VERSION,
    SNAPSHOT_SCHEMA_VERSION,
    json_schema_artifact_names,
    json_schema_documents,
    write_json_schema_files,
)
from seattrellis.schema_migration import migrate_json_file
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


def test_json_schema_files_match_registry() -> None:
    documents = json_schema_documents()

    assert json_schema_artifact_names() == [
        "student",
        "classroom-layout",
        "ruleset",
        "seating-snapshot",
        "candidate-set",
        "plan-comparison-report",
        "project",
        "editor-command",
        "editor-state",
    ]
    assert set(documents) == {
        "student.schema.json",
        "classroom-layout.schema.json",
        "ruleset.schema.json",
        "seating-snapshot.schema.json",
        "candidate-set.schema.json",
        "plan-comparison-report.schema.json",
        "project.schema.json",
        "editor-command.schema.json",
        "editor-state.schema.json",
    }
    assert documents["seating-snapshot.schema.json"]["$schema"]
    assert documents["seating-snapshot.schema.json"]["x-seattrellis-schema-version"] == (
        SNAPSHOT_SCHEMA_VERSION
    )
    assert documents["candidate-set.schema.json"]["x-seattrellis-schema-version"] == (
        CANDIDATE_SCHEMA_VERSION
    )
    assert documents["project.schema.json"]["x-seattrellis-schema-version"] == (
        PROJECT_SCHEMA_VERSION
    )
    for artifact in ("editor-command", "editor-state"):
        document = documents[f"{artifact}.schema.json"]
        assert document["$schema"] == JSON_SCHEMA_DRAFT
        assert document["$id"].endswith(f"/{artifact}.schema.json")
        assert document["x-seattrellis-artifact"] == artifact
        assert document["x-seattrellis-schema-version"] == EDITOR_PROTOCOL_VERSION
    command_schema = documents["editor-command.schema.json"]
    assert {
        "kind",
        "protocol_version",
        "command_id",
        "draft_id",
        "base_revision",
        "action",
    } <= set(command_schema["required"])
    assert command_schema["properties"]["operations"]["maxItems"] == 100
    assert command_schema["properties"]["command_id"]["minLength"] == 1
    assert command_schema["properties"]["command_id"]["maxLength"] == 128
    assert command_schema["allOf"][0]["then"]["properties"]["operations"][
        "minItems"
    ] == 1
    assert command_schema["allOf"][1]["then"]["properties"]["operations"][
        "maxItems"
    ] == 0
    assert all(
        "kind" in definition["required"]
        for name, definition in command_schema["definitions"].items()
        if name.endswith("Operation")
    )
    for file_name, document in documents.items():
        committed = json.loads(Path("schemas", file_name).read_text(encoding="utf-8"))
        assert committed == document


def test_json_schema_export_command(tmp_path) -> None:
    output_dir = tmp_path / "schemas"

    result = subprocess.run(
        [
            sys.executable,
            "-m",
            "seattrellis.cli",
            "schema",
            "export",
            "--output-dir",
            str(output_dir),
        ],
        check=False,
        capture_output=True,
        text=True,
    )

    assert result.returncode == 0, result.stderr
    assert sorted(path.name for path in output_dir.iterdir()) == sorted(
        json_schema_documents()
    )


def test_write_json_schema_files_returns_created_paths(tmp_path) -> None:
    paths = write_json_schema_files(tmp_path)

    assert sorted(path.name for path in paths) == sorted(json_schema_documents())
    assert all(path.exists() for path in paths)


def test_schema_migrate_current_snapshot_and_project(tmp_path) -> None:
    snapshot_output = tmp_path / "week1.migrated.json"
    project_output = tmp_path / "project.migrated.json"

    snapshot_result = migrate_json_file(
        "examples/history/week1.snapshot.json",
        output=snapshot_output,
    )
    project_result = migrate_json_file(
        "examples/project.seattrellis.json",
        output=project_output,
    )

    assert snapshot_result.artifact == "snapshot"
    assert snapshot_result.schema_version == SNAPSHOT_SCHEMA_VERSION
    assert load_snapshot(snapshot_output).schema_version == SNAPSHOT_SCHEMA_VERSION
    assert project_result.artifact == "project"
    assert project_result.schema_version == PROJECT_SCHEMA_VERSION
    assert load_project(project_output).schema_version == PROJECT_SCHEMA_VERSION


def test_schema_migrate_command_requires_output_or_in_place(tmp_path) -> None:
    result = subprocess.run(
        [
            sys.executable,
            "-m",
            "seattrellis.cli",
            "schema",
            "migrate",
            "--input",
            "examples/history/week1.snapshot.json",
        ],
        check=False,
        capture_output=True,
        text=True,
    )

    assert result.returncode != 0
    assert "requires --output unless --in-place is set" in result.stderr
