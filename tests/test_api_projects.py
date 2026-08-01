from __future__ import annotations

import io
import json
from pathlib import Path
from zipfile import ZipFile

import pytest

from seattrellis import cli
from seattrellis.api import create_app


def _client():
    pytest.importorskip("fastapi")
    httpx = pytest.importorskip("httpx")
    from fastapi.testclient import TestClient

    # Keep this import local so the dependency-free API modules remain usable
    # in installations without FastAPI or httpx.
    assert httpx is not None
    return TestClient(create_app(), base_url="http://127.0.0.1")


def test_project_workspace_lists_history_and_scans_privacy(tmp_path) -> None:
    paths = cli.init_demo(output_dir=tmp_path / "class", overwrite=True)
    client = _client()

    recent = client.get(
        "/api/v1/projects/recent",
        params={"root": str(tmp_path), "limit": 10},
    )
    assert recent.status_code == 200
    recent_payload = recent.json()
    assert recent_payload["api_version"] == "1"
    assert recent_payload["projects"][0]["name"] == "Demo Class"

    history = client.post(
        "/api/v1/projects/history",
        json={"project_path": str(paths["project"])},
    )
    assert history.status_code == 200
    history_payload = history.json()
    assert history_payload["project_name"] == "Demo Class"
    assert len(history_payload["history"]) == 3
    assert all("students" not in json.dumps(item) for item in history_payload["history"])

    privacy = client.post(
        "/api/v1/projects/privacy",
        json={"project_path": str(paths["project"]), "include_outputs": False},
    )
    assert privacy.status_code == 200
    privacy_payload = privacy.json()
    assert not privacy_payload["safe_for_public_sharing"]
    assert any(item["file"] == "students.csv" for item in privacy_payload["findings"])


def test_project_history_exposes_safe_artifact_provenance(tmp_path) -> None:
    paths = cli.init_demo(output_dir=tmp_path / "class", overwrite=True)
    source = paths["history"] / "week3.snapshot.json"
    payload = json.loads(source.read_text(encoding="utf-8"))
    payload["metadata"] = {
        "restored_from": "../../private/previous.snapshot.json",
        "manual_edit": {
            "operation_count": 2,
            "commands": [
                {
                    "action": "apply",
                    "operations": [
                        {
                            "kind": "swap_students",
                            "payload": {
                                "first_student": "SENSITIVE",
                                "second_student": "SENSITIVE-2",
                            },
                        }
                    ],
                }
            ],
        },
    }
    source.write_text(json.dumps(payload, ensure_ascii=False), encoding="utf-8")

    history = _client().post(
        "/api/v1/projects/history",
        json={"project_path": str(paths["project"])},
    )
    assert history.status_code == 200, history.text
    artifact = next(
        item for item in history.json()["history"] if item["name"] == source.name
    )
    assert artifact["provenance"] == {
        "source": "restored",
        "parent_name": "previous.snapshot.json",
        "operation_count": 2,
    }
    assert artifact["operation_history"] == [
        {
            "sequence": 1,
            "action": "apply",
            "operation_count": 1,
            "operation_kinds": ["swap_students"],
        }
    ]
    assert "SENSITIVE" not in history.text
    assert "../" not in history.text


def test_project_workspace_packs_and_restores_uploaded_bundle(tmp_path) -> None:
    paths = cli.init_demo(output_dir=tmp_path / "source", overwrite=True)
    client = _client()

    response = client.post(
        "/api/v1/projects/bundle",
        json={
            "project_path": str(paths["project"]),
            "include_outputs": False,
        },
    )
    assert response.status_code == 200
    assert response.headers["content-type"].startswith("application/zip")
    assert "project.seattrellis.zip" in response.headers["content-disposition"]
    with ZipFile(io.BytesIO(response.content)) as archive:
        manifest = json.loads(archive.read("manifest.json"))
    assert manifest["kind"] == "seattrellis_project_bundle"

    destination = tmp_path / "restored"
    restored = client.post(
        "/api/v1/projects/restore",
        data={"output_dir": str(destination), "overwrite": "false"},
        files={
            "bundle": (
                "project.seattrellis.zip",
                response.content,
                "application/zip",
            )
        },
    )
    assert restored.status_code == 200
    restored_payload = restored.json()
    assert restored_payload["project_path"].endswith("project.seattrellis.json")
    assert destination.joinpath("students.csv").exists()


def test_project_artifacts_can_be_compared_and_restored_safely(tmp_path) -> None:
    paths = cli.init_demo(output_dir=tmp_path / "class", overwrite=True)
    client = _client()

    history = client.post(
        "/api/v1/projects/history",
        json={"project_path": str(paths["project"])},
    ).json()["history"]
    newest, previous = history[:2]

    compared = client.post(
        "/api/v1/projects/artifacts/compare",
        json={
            "project_path": str(paths["project"]),
            "artifact_path": newest["path"],
            "compare_to_path": previous["path"],
        },
    )
    assert compared.status_code == 200
    compared_payload = compared.json()
    assert compared_payload["left"]["kind"] == "snapshot"
    assert compared_payload["diff"]["assignment_changes"] > 0
    assert compared_payload["diff"]["assignment_details"]
    assert compared_payload["diff"]["assignment_details"][0]["student_ref"].startswith(
        "student-"
    )
    assert "Alice" not in compared.text

    restored = client.post(
        "/api/v1/projects/artifacts/restore",
        json={
            "project_path": str(paths["project"]),
            "artifact_path": newest["path"],
        },
    )
    assert restored.status_code == 200
    restored_payload = restored.json()
    restored_path = Path(restored_payload["restored_artifact"])
    assert restored_path.exists()
    assert restored_path.parent == paths["project"].parent / "outputs"
    assert restored_path.name == "restored-week3.snapshot.json"


def test_project_artifact_endpoints_reject_paths_outside_the_project(tmp_path) -> None:
    paths = cli.init_demo(output_dir=tmp_path / "class", overwrite=True)
    outside = tmp_path / "outside.snapshot.json"
    outside.write_text("{}", encoding="utf-8")
    client = _client()

    response = client.post(
        "/api/v1/projects/artifacts/restore",
        json={
            "project_path": str(paths["project"]),
            "artifact_path": str(outside),
        },
    )
    assert response.status_code == 422


def test_project_schema_migration_can_preview_and_write_a_new_file(tmp_path) -> None:
    paths = cli.init_demo(output_dir=tmp_path / "class", overwrite=True)
    client = _client()
    source = paths["project"]
    original = source.read_bytes()

    preview = client.post(
        "/api/v1/projects/migration/preview",
        json={"project_path": str(source)},
    )
    assert preview.status_code == 200
    preview_payload = preview.json()
    assert preview_payload["dry_run"] is True
    assert preview_payload["artifact"] == "project"
    assert preview_payload["output_path"].endswith(".migrated.json")
    assert preview_payload["before_valid"] is True
    assert preview_payload["after_valid"] is None
    assert preview_payload["rollback_available"] is True
    assert isinstance(preview_payload["changes"], list)
    assert "Alice" not in preview.text
    assert "Bob" not in preview.text
    assert not Path(preview_payload["output_path"]).exists()
    assert source.read_bytes() == original

    applied = client.post(
        "/api/v1/projects/migration/apply",
        json={"project_path": str(source)},
    )
    assert applied.status_code == 200
    applied_payload = applied.json()
    migrated = Path(applied_payload["output_path"])
    assert applied_payload["dry_run"] is False
    assert applied_payload["before_valid"] is True
    assert applied_payload["after_valid"] is True
    assert applied_payload["rollback_available"] is True
    assert migrated.exists()
    assert migrated.parent == source.parent
    assert source.read_bytes() == original


def test_project_schema_migration_in_place_keeps_a_backup(tmp_path) -> None:
    paths = cli.init_demo(output_dir=tmp_path / "class", overwrite=True)
    client = _client()
    source = paths["project"]

    response = client.post(
        "/api/v1/projects/migration/apply",
        json={"project_path": str(source), "in_place": True},
    )
    assert response.status_code == 200
    payload = response.json()
    assert payload["output_path"] == str(source)
    assert payload["backup_path"]
    assert payload["before_valid"] is True
    assert payload["after_valid"] is True
    assert payload["rollback_available"] is True
    assert Path(payload["backup_path"]).exists()
    assert source.exists()


def test_project_schema_migration_checks_referenced_files(tmp_path) -> None:
    paths = cli.init_demo(output_dir=tmp_path / "class", overwrite=True)
    project_path = paths["project"]
    (project_path.parent / "outputs").mkdir(parents=True, exist_ok=True)
    project_data = json.loads(project_path.read_text(encoding="utf-8"))
    project_data["students"] = "missing-roster.csv"
    project_path.write_text(
        json.dumps(project_data, ensure_ascii=False, indent=2) + "\n",
        encoding="utf-8",
    )

    response = _client().post(
        "/api/v1/projects/migration/preview",
        json={"project_path": str(project_path)},
    )
    assert response.status_code == 200, response.text
    checks = {item["field"]: item for item in response.json()["reference_checks"]}
    assert checks["students"] == {
        "field": "students",
        "path": "missing-roster.csv",
        "expected": "file",
        "status": "missing",
    }
    assert checks["layout"]["status"] == "ok"
    assert checks["rules"]["status"] == "ok"
    assert checks["outputs_dir"]["status"] == "ok"


def test_project_schema_migration_backup_can_be_restored_safely(tmp_path) -> None:
    paths = cli.init_demo(output_dir=tmp_path / "class", overwrite=True)
    client = _client()
    source = paths["project"]
    original = source.read_bytes()

    applied = client.post(
        "/api/v1/projects/migration/apply",
        json={"project_path": str(source), "in_place": True},
    )
    assert applied.status_code == 200, applied.text
    applied_payload = applied.json()
    backup = Path(applied_payload["backup_path"])
    changed = json.loads(source.read_text(encoding="utf-8"))
    changed["name"] = "Changed locally"
    source.write_text(json.dumps(changed, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")

    restored = client.post(
        "/api/v1/projects/migration/restore",
        json={
            "project_path": str(source),
            "source_path": str(source),
            "backup_path": str(backup),
        },
    )
    assert restored.status_code == 200, restored.text
    restored_payload = restored.json()
    assert restored_payload["restored_valid"] is True
    assert restored_payload["source_path"] == str(source)
    assert Path(restored_payload["safety_backup_path"]).exists()
    assert source.read_bytes() == original


def test_project_rotation_save_persists_current_period_drafts(tmp_path) -> None:
    paths = cli.init_demo(output_dir=tmp_path / "class", overwrite=True)
    client = _client()
    generated = client.post(
        "/api/v1/classes/rotation",
        json={
            "draft": {
                "name": "Rotation class",
                "students": [
                    {"student_id": "S1", "name": "Alice"},
                    {"student_id": "S2", "name": "Bob"},
                ],
                "room": {
                    "layout": {
                        "layout_id": "rotation-room",
                        "name": "Rotation room",
                        "seats": [
                            {"seat_id": "A1", "row": 1, "col": 1},
                            {"seat_id": "A2", "row": 1, "col": 2},
                        ],
                    }
                },
                "goal": {"goal_id": "quick-shuffle"},
            },
            "period_count": 2,
            "period_labels": ["Monday", "Friday"],
            "options": {
                "backend": "fallback",
                "time_limit_seconds": 0.2,
                "seed": 7,
            },
        },
    )
    assert generated.status_code == 200, generated.text
    generated_payload = generated.json()
    period_editors = generated_payload["period_editors"]

    first_editor = period_editors[0]
    first_students = first_editor["students"]
    swapped = client.post(
        f"/api/v1/editing/drafts/{first_editor['draft_id']}/commands",
        json={
            "kind": "seattrellis_editor_command",
            "protocol_version": "1.0",
            "command_id": "save-rotation-swap",
            "draft_id": first_editor["draft_id"],
            "base_revision": first_editor["revision"],
            "action": "apply",
            "operations": [
                {
                    "kind": "swap_students",
                    "payload": {
                        "first_student": first_students[0]["student_key"],
                        "second_student": first_students[1]["student_key"],
                    },
                }
            ],
        },
    )
    assert swapped.status_code == 200, swapped.text

    saved = client.post(
        "/api/v1/projects/rotation/save",
        json={
            "project_path": str(paths["project"]),
            "rotation_plan": generated_payload["rotation_plan"],
            "draft_ids": [editor["draft_id"] for editor in period_editors],
            "output_name": "edited-rotation",
        },
    )
    assert saved.status_code == 200, saved.text
    saved_payload = saved.json()
    output = Path(saved_payload["output_path"])
    assert output == paths["project"].parent / "outputs" / "edited-rotation.json"
    assert output.exists()

    from seattrellis.io.json_files import load_rotation_plan

    plan = load_rotation_plan(output)
    assert [period.label for period in plan.periods] == ["Monday", "Friday"]
    assert plan.periods[0].snapshot.metadata["project_persistence"]["period"] == 1
    assert plan.periods[0].snapshot.metadata["manual_edit"]["operation_count"] == 1
    assert plan.periods[0].snapshot.metadata["manual_edit"]["commands"][0]["command_id"] == "save-rotation-swap"

    listed = client.post(
        "/api/v1/projects/history",
        json={"project_path": str(paths["project"])},
    ).json()
    rotation_outputs = [item for item in listed["outputs"] if item["kind"] == "rotation_plan"]
    saved_rotation = next(
        item
        for item in rotation_outputs
        if item["path"] == str(output) and item["period_count"] == 2
    )
    assert saved_rotation["operation_history"][0] == {
        "sequence": 1,
        "action": "apply",
        "operation_count": 1,
        "operation_kinds": ["swap_students"],
    }

    loaded = client.post(
        "/api/v1/projects/rotation/load",
        json={
            "project_path": str(paths["project"]),
            "artifact_path": str(output),
        },
    )
    assert loaded.status_code == 200, loaded.text
    loaded_payload = loaded.json()
    assert loaded_payload["artifact_path"] == str(output)
    assert loaded_payload["rotation_plan"]["name"] == generated_payload["rotation_plan"]["name"]
    assert len(loaded_payload["period_editors"]) == 2
    assert loaded_payload["period_editors"][0]["students"][0]["display_name"] in {
        "Alice",
        "Bob",
    }

    grouped_output = output.parent / "grouped-rotation.json"
    grouped_data = json.loads(output.read_text(encoding="utf-8"))
    for period in grouped_data["periods"]:
        period["snapshot"]["rules"]["groups"] = [
            {
                "name": "Pair A",
                "students": ["S1", "S2"],
                "together": True,
                "separate": False,
            },
            {
                "name": "Empty B",
                "students": [],
                "together": False,
                "separate": False,
            },
            {
                "name": "Missing C",
                "students": ["S999"],
                "together": False,
                "separate": False,
            },
        ]
        if period["period"] == 1:
            period["snapshot"]["assignments"] = [
                assignment
                for assignment in period["snapshot"]["assignments"]
                if assignment["student_key"] != "S2"
            ]
    grouped_output.write_text(json.dumps(grouped_data), encoding="utf-8")
    register = client.post(
        "/api/v1/projects/rotation/group-register",
        json={
            "project_path": str(paths["project"]),
            "artifact_path": str(grouped_output),
            "format": "html",
            "locale": "en",
        },
    )
    assert register.status_code == 200, register.text
    assert register.headers["content-type"].startswith("text/html")
    assert "Pair A" in register.text
    assert "Alice" in register.text
    assert "Bob" in register.text
    assert "Empty group" in register.text
    assert "Missing from roster" in register.text
    assert "Unseated" in register.text
    assert "group-register.html" in register.headers["content-disposition"]

    csv_register = client.post(
        "/api/v1/projects/rotation/group-register",
        json={
            "project_path": str(paths["project"]),
            "artifact_path": str(grouped_output),
            "format": "csv",
            "locale": "en",
        },
    )
    assert csv_register.status_code == 200
    assert csv_register.headers["content-type"].startswith("text/csv")
    assert "Student ID" in csv_register.content.decode("utf-8-sig")
