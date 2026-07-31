from __future__ import annotations

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
