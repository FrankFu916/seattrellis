from __future__ import annotations

from pathlib import Path

import pytest

from seattrellis.optional import MissingOptionalDependencyError
from seattrellis.workspace_server import (
    WorkspaceServerOptions,
    create_workspace_app,
    resolve_workspace_assets,
)


def test_workspace_server_options_accept_loopback_only() -> None:
    assert WorkspaceServerOptions().browser_url == "http://127.0.0.1:8765/"
    assert (
        WorkspaceServerOptions(host="::1", port=9000).browser_url
        == "http://[::1]:9000/"
    )

    with pytest.raises(ValueError, match="loopback"):
        WorkspaceServerOptions(host="0.0.0.0")
    with pytest.raises(ValueError, match="between 1 and 65535"):
        WorkspaceServerOptions(port=0)


def test_workspace_assets_require_a_complete_build(tmp_path: Path) -> None:
    incomplete = tmp_path / "incomplete"
    incomplete.mkdir()
    with pytest.raises(ValueError, match="index.html"):
        resolve_workspace_assets(incomplete)

    complete = tmp_path / "complete"
    complete.mkdir()
    (complete / "index.html").write_text("<main>ready</main>", encoding="utf-8")
    assert resolve_workspace_assets(complete) == complete.resolve()


def test_workspace_app_serves_static_client_and_api(tmp_path: Path) -> None:
    pytest.importorskip("fastapi")
    pytest.importorskip("httpx")
    from fastapi.testclient import TestClient

    assets = tmp_path / "client"
    assets.mkdir()
    (assets / "index.html").write_text("<main>SeatTrellis</main>", encoding="utf-8")

    app = create_workspace_app(static_dir=assets)
    with TestClient(app) as client:
        page = client.get("/", headers={"Host": "127.0.0.1"})
        health = client.get("/api/v1/health", headers={"Host": "127.0.0.1"})

    assert page.status_code == 200
    assert "SeatTrellis" in page.text
    assert health.status_code == 200
    assert health.json()["status"] == "ok"


def test_missing_default_assets_have_install_guidance(monkeypatch, tmp_path: Path) -> None:
    import seattrellis.workspace_server as workspace_server

    fake_module = tmp_path / "package" / "workspace_server.py"
    monkeypatch.setattr(workspace_server, "__file__", str(fake_module))

    with pytest.raises(MissingOptionalDependencyError, match="Browser workbench"):
        resolve_workspace_assets()
