from __future__ import annotations

import io
import json
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
