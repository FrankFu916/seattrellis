from __future__ import annotations

from dataclasses import dataclass
from pathlib import Path
from typing import Any

try:
    from pydantic.v1 import ValidationError
except ImportError:  # pragma: no cover - pydantic v1.
    from pydantic import ValidationError

from seattrellis.io.json_files import InputFileError, read_json, write_json_model
from seattrellis.models.project import SeatTrellisProject


@dataclass(frozen=True)
class ProjectPaths:
    project_file: Path
    root: Path
    students: Path
    layout: Path
    rules: Path
    history_dir: Path | None
    outputs_dir: Path


def load_project(path: str | Path) -> SeatTrellisProject:
    source = Path(path)
    if not source.exists():
        raise InputFileError(f"Project file not found: {source}")
    try:
        data = read_json(source)
    except InputFileError as exc:
        message = str(exc).replace("Input file", "Project file", 1)
        raise InputFileError(message) from exc
    try:
        if hasattr(SeatTrellisProject, "model_validate"):
            return SeatTrellisProject.model_validate(data)  # type: ignore[attr-defined,no-any-return]
        return SeatTrellisProject.parse_obj(data)
    except ValidationError as exc:
        errors = "; ".join(_format_validation_error(error) for error in exc.errors())
        raise InputFileError(f"Invalid project file: {source}\n{errors}") from exc


def write_project(
    project: SeatTrellisProject,
    path: str | Path,
    *,
    overwrite: bool = False,
) -> Path:
    output = Path(path)
    if output.exists() and not overwrite:
        raise InputFileError(f"Project file already exists: {output}. Use --force to overwrite it.")
    return write_json_model(project, output)


def resolve_project_paths(
    project: SeatTrellisProject,
    project_path: str | Path,
    *,
    require_inputs: bool = False,
    require_history: bool = False,
    create_outputs: bool = False,
) -> ProjectPaths:
    project_file = Path(project_path).resolve()
    root = project_file.parent
    paths = ProjectPaths(
        project_file=project_file,
        root=root,
        students=(root / project.students).resolve(),
        layout=(root / project.layout).resolve(),
        rules=(root / project.rules).resolve(),
        history_dir=(root / project.history_dir).resolve() if project.history_dir is not None else None,
        outputs_dir=(root / project.outputs_dir).resolve(),
    )
    if require_inputs:
        _require_file(paths.students, "students")
        _require_file(paths.layout, "layout")
        _require_file(paths.rules, "rules")
    if require_history and paths.history_dir is not None:
        _require_directory(paths.history_dir, "history_dir")
    if create_outputs:
        if paths.outputs_dir.exists() and not paths.outputs_dir.is_dir():
            raise InputFileError(
                f'Project reference "outputs_dir" is not a directory: {paths.outputs_dir}'
            )
        paths.outputs_dir.mkdir(parents=True, exist_ok=True)
    return paths


def load_project_paths(
    project_path: str | Path,
    *,
    require_inputs: bool = False,
    require_history: bool = False,
    create_outputs: bool = False,
) -> tuple[SeatTrellisProject, ProjectPaths]:
    project = load_project(project_path)
    paths = resolve_project_paths(
        project,
        project_path,
        require_inputs=require_inputs,
        require_history=require_history,
        create_outputs=create_outputs,
    )
    return project, paths


def find_latest_project_artifact(outputs_dir: str | Path) -> Path:
    directory = Path(outputs_dir)
    if not directory.exists():
        raise InputFileError(f"Project outputs directory not found: {directory}")
    if not directory.is_dir():
        raise InputFileError(f"Project outputs path is not a directory: {directory}")
    candidates = {
        *directory.glob("*.snapshot.json"),
        *directory.glob("*.candidates.json"),
    }
    if not candidates:
        raise InputFileError(
            f"No project snapshot or candidate set found in outputs directory: {directory}"
        )
    return max(candidates, key=lambda path: (path.stat().st_mtime_ns, path.name))


def _require_file(path: Path, field_name: str) -> None:
    if not path.exists():
        raise InputFileError(f'Project reference "{field_name}" not found: {path}')
    if not path.is_file():
        raise InputFileError(f'Project reference "{field_name}" is not a file: {path}')


def _require_directory(path: Path, field_name: str) -> None:
    if not path.exists():
        raise InputFileError(f'Project reference "{field_name}" directory not found: {path}')
    if not path.is_dir():
        raise InputFileError(f'Project reference "{field_name}" is not a directory: {path}')


def _format_validation_error(error: dict[str, Any]) -> str:
    location_items = [item for item in error.get("loc", ()) if item != "__root__"]
    location = ".".join(str(item) for item in location_items)
    message = error.get("msg", "invalid value")
    return f"{location}: {message}" if location else str(message)
