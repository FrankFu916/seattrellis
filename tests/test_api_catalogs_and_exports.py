from __future__ import annotations

import io
import zipfile

import pytest

from seattrellis.api.handlers import catalogs
from seattrellis.api.models import ExportDraftRequest


def test_catalogs_contract_is_bilingual_and_bounded() -> None:
    data = catalogs()

    assert list(data.keys()) == ["roomTemplates", "teacherGoals", "exportFormats"]

    room_ids = [room["id"] for room in data["roomTemplates"]]
    assert room_ids == ["standard-30", "standard-48", "standard-60"]
    first = data["roomTemplates"][0]
    assert first["name"].keys() == {"zh-CN", "en"}
    assert first["description"].keys() == {"zh-CN", "en"}
    assert first["rows"] > 0
    assert first["columns"] >= first["rows"]

    goal_ids = [goal["id"] for goal in data["teacherGoals"]]
    assert goal_ids == ["daily-rotation", "quick-shuffle", "fair-shuffle", "peer-support"]
    assert "custom" not in goal_ids
    assert all(goal["name"].keys() == {"zh-CN", "en"} for goal in data["teacherGoals"])

    format_ids = [item["id"] for item in data["exportFormats"]]
    assert format_ids
    assert all(
        item["name"].keys() == {"zh-CN", "en"} for item in data["exportFormats"]
    )


def test_export_draft_request_validates_identifier() -> None:
    with pytest.raises(ValueError):
        ExportDraftRequest(draft_id="  ", format="svg")


def test_export_draft_request_accepts_template_privacy_and_scale() -> None:
    request = ExportDraftRequest(
        draft_id="draft-1",
        format="print-html",
        template="teacher",
        privacy={"hide_scores": True, "anonymize": True},
        page_scale=1.25,
    )

    assert request.template == "teacher"
    assert request.privacy is not None
    assert request.privacy.hide_scores is True
    assert request.privacy.anonymize is True
    assert request.page_scale == 1.25


def _generate_draft(client: object, students: list[dict[str, object]] | None = None) -> str:
    response = client.post(
        "/api/v1/classes/generate",
        json={
            "draft": {
                "name": "Test Class",
                "students": students
                or [
                    {"student_id": "S1", "name": "Alice"},
                    {"student_id": "S2", "name": "Bob"},
                    {"student_id": "S3", "name": "Cara"},
                ],
                "room": {"template_id": "standard-30"},
                "goal": {"goal_id": "quick-shuffle"},
            },
            "options": {"candidate_count": 1, "time_limit_seconds": 0.5},
        },
        headers={"Host": "127.0.0.1"},
    )
    assert response.status_code == 200
    return response.json()["editor"]["draft_id"]


def test_catalogs_http_route_serves_the_workbench_contract() -> None:
    pytest.importorskip("fastapi")
    pytest.importorskip("httpx")
    from fastapi.testclient import TestClient

    from seattrellis.api.http import create_app

    with TestClient(create_app()) as client:
        response = client.get("/api/v1/catalogs", headers={"Host": "127.0.0.1"})

    assert response.status_code == 200
    body = response.json()
    assert body["roomTemplates"][0]["id"] == "standard-30"
    assert body["teacherGoals"][0]["id"] == "daily-rotation"
    assert body["exportFormats"][0]["id"] == "print-html"


def test_export_svg_from_a_generated_draft() -> None:
    pytest.importorskip("fastapi")
    pytest.importorskip("httpx")
    from fastapi.testclient import TestClient

    from seattrellis.api.http import create_app

    with TestClient(create_app()) as client:
        draft_id = _generate_draft(client)
        export = client.post(
            "/api/v1/exports",
            json={"draft_id": draft_id, "format": "svg", "orientation": "landscape"},
            headers={"Host": "127.0.0.1"},
        )

    assert export.status_code == 200
    assert export.headers["content-type"].startswith("image/svg+xml")
    assert export.headers["content-disposition"] == 'attachment; filename="seating.svg"'
    assert export.content.startswith(b"<?xml")
    assert b"<svg" in export.content
    assert b"<script" not in export.content
    assert b"PRIVATE-SECRET" not in export.content


def test_export_print_html_applies_template_privacy_and_scale() -> None:
    pytest.importorskip("fastapi")
    pytest.importorskip("httpx")
    from fastapi.testclient import TestClient

    from seattrellis.api.http import create_app

    with TestClient(create_app()) as client:
        draft_id = _generate_draft(client)
        export = client.post(
            "/api/v1/exports",
            json={
                "draft_id": draft_id,
                "format": "print-html",
                "template": "teacher",
                "privacy": {
                    "hide_scores": True,
                    "hide_notes": True,
                    "hide_special_needs": True,
                    "anonymize": True,
                    "show_height": False,
                    "show_vision": False,
                },
                "orientation": "portrait",
                "page_scale": 1.2,
            },
            headers={"Host": "127.0.0.1"},
        )

    assert export.status_code == 200
    assert export.headers["content-type"].startswith("text/html")
    assert "学生 01" in export.text
    assert "Alice" not in export.text


def test_public_export_keeps_sensitive_fields_hidden() -> None:
    pytest.importorskip("fastapi")
    pytest.importorskip("httpx")
    from fastapi.testclient import TestClient

    from seattrellis.api.http import create_app

    with TestClient(create_app()) as client:
        draft_id = _generate_draft(
            client,
            [
                {
                    "student_id": "S1",
                    "name": "Alice",
                    "score": 98,
                    "height_cm": 178,
                    "vision": "front",
                    "needs": ["quiet"],
                    "notes": "PRIVATE-SECRET",
                },
                {"student_id": "S2", "name": "Bob"},
            ],
        )
        export = client.post(
            "/api/v1/exports",
            json={
                "draft_id": draft_id,
                "format": "print-html",
                "template": "public",
                "privacy": {
                    "hide_scores": False,
                    "hide_notes": False,
                    "hide_special_needs": False,
                    "anonymize": False,
                    "show_height": True,
                    "show_vision": True,
                },
            },
            headers={"Host": "127.0.0.1"},
        )

    assert export.status_code == 200
    assert "PRIVATE-SECRET" not in export.text
    # The height field must not render at all under the public template.
    # Assert on the field label instead of the value "178": the export embeds
    # a generation timestamp whose random microseconds can contain "178",
    # which made the value assertion flaky (CI hit 469178+00:00 once).
    assert "身高" not in export.text
    assert "quiet" not in export.text


def test_export_report_template_uses_the_draft_candidate() -> None:
    pytest.importorskip("fastapi")
    pytest.importorskip("httpx")
    from fastapi.testclient import TestClient

    from seattrellis.api.http import create_app

    with TestClient(create_app()) as client:
        draft_id = _generate_draft(client)
        export = client.post(
            "/api/v1/exports",
            json={
                "draft_id": draft_id,
                "format": "print-html",
                "template": "report",
            },
            headers={"Host": "127.0.0.1"},
        )

    assert export.status_code == 200
    assert "推荐理由" in export.text


def test_export_missing_draft_returns_404() -> None:
    pytest.importorskip("fastapi")
    pytest.importorskip("httpx")
    from fastapi.testclient import TestClient

    from seattrellis.api.http import create_app

    with TestClient(create_app()) as client:
        export = client.post(
            "/api/v1/exports",
            json={"draft_id": "no-such-draft", "format": "svg"},
            headers={"Host": "127.0.0.1"},
        )

    assert export.status_code == 404
    assert export.json()["error"]["code"] == "editor_draft_not_found"


def test_export_rejects_unknown_format() -> None:
    pytest.importorskip("fastapi")
    pytest.importorskip("httpx")
    from fastapi.testclient import TestClient

    from seattrellis.api.http import create_app

    with TestClient(create_app()) as client:
        draft_id = _generate_draft(client)
        export = client.post(
            "/api/v1/exports",
            json={"draft_id": draft_id, "format": "nonsense"},
            headers={"Host": "127.0.0.1"},
        )

    assert export.status_code == 422


def test_export_pptx_uses_no_external_media_when_available() -> None:
    pytest.importorskip("fastapi")
    pytest.importorskip("httpx")
    pytest.importorskip("pptx")
    from fastapi.testclient import TestClient

    from seattrellis.api.http import create_app

    with TestClient(create_app()) as client:
        draft_id = _generate_draft(client)
        export = client.post(
            "/api/v1/exports",
            json={"draft_id": draft_id, "format": "pptx"},
            headers={"Host": "127.0.0.1"},
        )

    assert export.status_code == 200
    assert export.content[:2] == b"PK"
    with zipfile.ZipFile(io.BytesIO(export.content)) as archive:
        names = archive.namelist()
        assert "ppt/presentation.xml" in names
        assert not [n for n in names if n.startswith("ppt/media/")]
        assert not [n for n in names if n.endswith("vbaProject.bin")]
