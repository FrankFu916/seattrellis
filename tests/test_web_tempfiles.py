from pathlib import Path

from seattrellis.web.tempfiles import (
    discard_persistent_tempdir,
    make_persistent_tempdir,
)


def test_registered_web_directory_can_be_discarded_early() -> None:
    directory = Path(make_persistent_tempdir())
    marker = directory / "student-result.json"
    marker.write_text("sensitive", encoding="utf-8")

    assert discard_persistent_tempdir(directory) is True
    assert not directory.exists()
    assert discard_persistent_tempdir(directory) is False


def test_unknown_directory_is_never_removed(tmp_path: Path) -> None:
    directory = tmp_path / "not-owned-by-web-runtime"
    directory.mkdir()

    assert discard_persistent_tempdir(directory) is False
    assert directory.exists()
