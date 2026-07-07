from pathlib import PurePosixPath

from scripts.check_repository_hygiene import (
    _is_allowed_example_snapshot,
    _path_problem,
    workspace_metadata,
)


def test_rejects_private_and_generated_paths() -> None:
    assert _path_problem("private/class.csv")
    assert _path_problem("outputs/seating.pdf")
    assert _path_problem("class.snapshot.json")
    assert _path_problem(".github/.DS_Store")
    assert _path_problem(".env")


def test_allows_fictional_history_fixtures() -> None:
    path = "examples/history/week1.snapshot.json"
    assert _is_allowed_example_snapshot(PurePosixPath(path))
    assert _path_problem(path) is None


def test_allows_normal_repository_files() -> None:
    assert _path_problem("src/seattrellis/service.py") is None
    assert _path_problem("docs/privacy.md") is None


def test_finds_workspace_metadata(tmp_path) -> None:
    metadata = tmp_path / "nested" / ".DS_Store"
    metadata.parent.mkdir()
    metadata.touch()
    ignored_git_metadata = tmp_path / ".git" / ".DS_Store"
    ignored_git_metadata.parent.mkdir()
    ignored_git_metadata.touch()

    assert workspace_metadata(tmp_path) == ["nested/.DS_Store"]
