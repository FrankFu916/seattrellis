from __future__ import annotations

from urllib.error import HTTPError
from urllib.request import Request, urlopen

from seattrellis.desktop_app import build_parser
import pytest

from seattrellis.desktop import DesktopOptions, DesktopSession


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
    assert capsys.readouterr().out.strip() == "seattrellis-desktop 1.8.3"


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
