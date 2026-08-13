from __future__ import annotations

import base64
import sys
from types import SimpleNamespace
from urllib.error import HTTPError
from urllib.request import Request, urlopen

from seattrellis.desktop_app import build_parser
import pytest

from seattrellis.desktop import DesktopBridge, DesktopOptions, DesktopSession


class _FakeWindow:
    def __init__(self, *responses: object) -> None:
        self.responses = list(responses)
        self.calls: list[tuple[object, dict[str, object]]] = []

    def create_file_dialog(self, dialog_type: object, **kwargs: object) -> object:
        self.calls.append((dialog_type, kwargs))
        return self.responses.pop(0)


def test_desktop_options_allow_ephemeral_loopback_port() -> None:
    assert DesktopOptions(port=0).width == 1280
    assert DesktopOptions(host="localhost", port=8766).height == 900

    with pytest.raises(ValueError, match="loopback"):
        DesktopOptions(host="0.0.0.0")
    with pytest.raises(ValueError, match="dimensions"):
        DesktopOptions(width=100)


def test_desktop_session_url_contains_unpredictable_session_token() -> None:
    session = DesktopSession(DesktopOptions(port=8766))
    with pytest.raises(RuntimeError, match="has not started"):
        _ = session.url
    session.port = 8766
    assert session.url.startswith("http://127.0.0.1:8766/?session=")
    assert len(session.session_token) >= 32


def test_standalone_desktop_parser_has_stable_defaults() -> None:
    args = build_parser().parse_args([])
    assert args.width == 1280
    assert args.height == 900
    assert args.title == "SeatTrellis"

    custom = build_parser().parse_args(["--width", "1440", "--height", "960", "--title", "Classroom"])
    assert (custom.width, custom.height, custom.title) == (1440, 960, "Classroom")


def test_standalone_desktop_parser_exposes_version(capsys) -> None:
    with pytest.raises(SystemExit) as exit_info:
        build_parser().parse_args(["--version"])

    assert exit_info.value.code == 0
    assert capsys.readouterr().out.strip() == "seattrellis-desktop 1.9.0"


def test_desktop_session_serves_bootstrap_before_api_authentication(tmp_path) -> None:
    """The embedded window must receive HTML before React attaches its token."""

    (tmp_path / "index.html").write_text(
        "<!doctype html><title>SeatTrellis</title>", encoding="utf-8"
    )
    session = DesktopSession(
        DesktopOptions(port=0, startup_timeout_seconds=5),
        static_dir=str(tmp_path),
    )
    try:
        try:
            page_url = session.start()
        except PermissionError as exc:
            pytest.skip(f"This test environment does not allow loopback sockets: {exc}")
        with urlopen(page_url, timeout=5) as response:
            assert response.status == 200
            assert "SeatTrellis" in response.read().decode("utf-8")

        api_url = page_url.split("?", 1)[0] + "api/v1/health"
        with pytest.raises(HTTPError) as missing_session:
            urlopen(api_url, timeout=5)
        assert missing_session.value.code == 401

        request = Request(
            api_url,
            headers={"Authorization": f"Bearer {session.session_token}"},
        )
        with urlopen(request, timeout=5) as response:
            assert response.status == 200
            assert '"status":"ok"' in response.read().decode("utf-8")
    finally:
        session.stop()


def test_desktop_bridge_opens_roster_and_keeps_recent_metadata(tmp_path, monkeypatch) -> None:
    roster = tmp_path / "班级名单.CSV"
    roster.write_text("student_id,name\nS01,小林\n", encoding="utf-8")
    fake_webview = SimpleNamespace(
        FileDialog=SimpleNamespace(OPEN="open", SAVE="save", FOLDER="folder")
    )
    monkeypatch.setitem(sys.modules, "webview", fake_webview)
    window = _FakeWindow([str(roster)])
    bridge = DesktopBridge(recent_file_path=tmp_path / "recent.json")
    bridge.attach_window(window)

    payload = bridge.open_roster_file()

    assert payload is not None
    assert payload["name"] == roster.name
    assert base64.b64decode(payload["content_base64"]).startswith(b"student_id")
    assert bridge.list_recent_files() == [{"name": roster.name, "path": str(roster.resolve())}]
    assert window.calls[0][0] == "open"

    reopened = bridge.open_recent_file(str(roster))
    assert reopened is not None
    assert bridge.open_recent_file(str(tmp_path / "other.csv")) is None


def test_desktop_bridge_saves_export_atomically_and_sanitizes_filename(
    tmp_path, monkeypatch
) -> None:
    fake_webview = SimpleNamespace(
        FileDialog=SimpleNamespace(OPEN="open", SAVE="save", FOLDER="folder")
    )
    monkeypatch.setitem(sys.modules, "webview", fake_webview)
    destination = tmp_path / "saved.html"
    window = _FakeWindow([str(destination)])
    bridge = DesktopBridge(recent_file_path=tmp_path / "recent.json")
    bridge.attach_window(window)

    result = bridge.save_export_file(
        "../../teacher-plan.html",
        base64.b64encode(b"<html>ok</html>").decode("ascii"),
    )

    assert result == {"saved": True, "name": destination.name}
    assert destination.read_bytes() == b"<html>ok</html>"
    assert window.calls[0][0] == "save"
    assert window.calls[0][1]["save_filename"] == "teacher-plan.html"


def test_desktop_bridge_rejects_unsupported_roster_and_invalid_export(
    tmp_path, monkeypatch
) -> None:
    fake_webview = SimpleNamespace(
        FileDialog=SimpleNamespace(OPEN="open", SAVE="save", FOLDER="folder")
    )
    monkeypatch.setitem(sys.modules, "webview", fake_webview)
    window = _FakeWindow([str(tmp_path / "students.txt")])
    bridge = DesktopBridge(recent_file_path=tmp_path / "recent.json")
    bridge.attach_window(window)

    with pytest.raises(ValueError, match="CSV or Excel"):
        bridge.open_roster_file()
    with pytest.raises(ValueError, match="base64"):
        bridge.save_export_file("seating.html", "not-base64")
