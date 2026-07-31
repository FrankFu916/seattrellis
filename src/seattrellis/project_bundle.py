"""Safe local project backup, restore, and privacy inspection helpers."""

from __future__ import annotations

import csv
import json
import os
import shutil
import tempfile
from dataclasses import dataclass
from datetime import datetime, timezone
from pathlib import Path, PurePosixPath
from typing import Any, Iterable
from zipfile import ZIP_DEFLATED, ZipFile, ZipInfo

from seattrellis.io.json_files import InputFileError, read_json
from seattrellis.io.project import load_project, resolve_project_paths


BUNDLE_FORMAT_VERSION = 1
MAX_BUNDLE_FILE_BYTES = 100 * 1024 * 1024
MAX_BUNDLE_TOTAL_BYTES = 500 * 1024 * 1024
_SENSITIVE_KEYS = {
    "student_id",
    "student_key",
    "student_name",
    "score",
    "grade",
    "notes",
    "note",
    "special_needs",
    "special_need",
    "height",
    "vision",
    "email",
    "phone",
    "name",
}


@dataclass(frozen=True)
class PrivacyFinding:
    """One potentially identifying or educationally sensitive field."""

    file: str
    fields: tuple[str, ...]


@dataclass(frozen=True)
class ProjectPrivacyReport:
    """Results of scanning the files that would be included in a bundle."""

    files_scanned: int
    findings: tuple[PrivacyFinding, ...]

    @property
    def safe_for_public_sharing(self) -> bool:
        return not self.findings

    def as_dict(self) -> dict[str, Any]:
        return {
            "files_scanned": self.files_scanned,
            "safe_for_public_sharing": self.safe_for_public_sharing,
            "findings": [
                {"file": finding.file, "fields": list(finding.fields)}
                for finding in self.findings
            ],
        }

    def format(self) -> str:
        lines = [
            f"Scanned {self.files_scanned} project file(s).",
            (
                "No sensitive fields detected. The selected files are suitable "
                "for public sharing."
                if self.safe_for_public_sharing
                else "Sensitive fields detected; review before sharing publicly."
            ),
        ]
        for finding in self.findings:
            lines.append(f"- {finding.file}: {', '.join(finding.fields)}")
        return "\n".join(lines)


@dataclass(frozen=True)
class ProjectBundleResult:
    """Created bundle and its privacy inspection."""

    path: Path
    file_count: int
    privacy: ProjectPrivacyReport


@dataclass(frozen=True)
class RecentProject:
    """A project file found while browsing a local projects directory."""

    path: Path
    name: str
    modified_at: datetime


def project_files(
    project_path: str | Path,
    *,
    include_outputs: bool = True,
) -> tuple[Path, list[Path]]:
    """Resolve and validate all files that belong to a project bundle.

    A project may use ``..`` references for command-line workflows, but a
    portable bundle must never follow a reference outside the project root.
    """

    project_file = Path(project_path).expanduser().resolve()
    project = load_project(project_file)
    paths = resolve_project_paths(project, project_file)
    root = project_file.parent.resolve()
    files: set[Path] = {project_file}

    for path, label in (
        (paths.students, "students"),
        (paths.layout, "layout"),
        (paths.rules, "rules"),
    ):
        _add_file_reference(files, path, root, label)
    if paths.history_dir is not None and paths.history_dir.exists():
        _add_directory_files(files, paths.history_dir, root, "history_dir")
    if include_outputs and paths.outputs_dir.exists():
        _add_directory_files(files, paths.outputs_dir, root, "outputs_dir")
    return root, sorted(files, key=lambda path: path.relative_to(root).as_posix())


def scan_project_privacy(
    project_path: str | Path,
    *,
    include_outputs: bool = True,
) -> ProjectPrivacyReport:
    """Inspect project text files without returning their contents."""

    root, files = project_files(project_path, include_outputs=include_outputs)
    findings: list[PrivacyFinding] = []
    for path in files:
        fields = _scan_file(path)
        if fields:
            findings.append(
                PrivacyFinding(
                    file=path.relative_to(root).as_posix(),
                    fields=tuple(sorted(fields)),
                )
            )
    return ProjectPrivacyReport(files_scanned=len(files), findings=tuple(findings))


def pack_project(
    project_path: str | Path,
    output_path: str | Path | None = None,
    *,
    include_outputs: bool = True,
    overwrite: bool = False,
) -> ProjectBundleResult:
    """Create a self-contained ``.seattrellis.zip`` backup."""

    project_file = Path(project_path).expanduser().resolve()
    root, files = project_files(project_file, include_outputs=include_outputs)
    output = (
        Path(output_path)
        if output_path is not None
        else _default_bundle_path(project_file)
    )
    if output.exists() and not overwrite:
        raise InputFileError(f"Project bundle already exists: {output}. Use --force to overwrite it.")
    privacy = scan_project_privacy(project_file, include_outputs=include_outputs)
    relative_project = project_file.relative_to(root).as_posix()
    manifest = {
        "kind": "seattrellis_project_bundle",
        "format_version": BUNDLE_FORMAT_VERSION,
        "created_at": datetime.now(timezone.utc).isoformat(),
        "project_file": relative_project,
        "include_outputs": include_outputs,
        "files": [path.relative_to(root).as_posix() for path in files],
        "privacy": privacy.as_dict(),
    }
    try:
        output.parent.mkdir(parents=True, exist_ok=True)
        with tempfile.NamedTemporaryFile(
            mode="wb",
            dir=output.parent,
            prefix=f".{output.name}.",
            suffix=".tmp",
            delete=False,
        ) as temporary:
            temporary_path = Path(temporary.name)
        try:
            with ZipFile(temporary_path, "w", compression=ZIP_DEFLATED) as archive:
                archive.writestr("manifest.json", json.dumps(manifest, ensure_ascii=False, indent=2) + "\n")
                for path in files:
                    archive.write(path, path.relative_to(root).as_posix())
            os.replace(temporary_path, output)
        finally:
            temporary_path.unlink(missing_ok=True)
    except OSError as exc:
        raise InputFileError(f"Could not create project bundle {output}: {exc}") from exc
    return ProjectBundleResult(path=output, file_count=len(files), privacy=privacy)


def restore_project_bundle(
    bundle_path: str | Path,
    output_dir: str | Path,
    *,
    overwrite: bool = False,
) -> Path:
    """Validate and restore a project bundle without allowing path traversal."""

    bundle = Path(bundle_path).expanduser().resolve()
    destination = Path(output_dir).expanduser().resolve()
    if not bundle.is_file():
        raise InputFileError(f"Project bundle not found: {bundle}")
    if destination.exists() and any(destination.iterdir()) and not overwrite:
        raise InputFileError(
            f"Restore destination is not empty: {destination}. Use --force to merge files."
        )

    try:
        destination.parent.mkdir(parents=True, exist_ok=True)
        with ZipFile(bundle) as archive:
            manifest = _read_manifest(archive)
            entries = _validated_entries(archive)
            listed_files = set(manifest["files"])
            entry_files = {entry.filename for entry in entries if not entry.is_dir()}
            if listed_files != entry_files:
                raise InputFileError("Project bundle manifest does not match its file entries.")
            project_name = _safe_archive_name(manifest["project_file"])
            if project_name not in listed_files:
                raise InputFileError("Project bundle manifest does not include its project file.")
            total_size = sum(entry.file_size for entry in entries)
            if total_size > MAX_BUNDLE_TOTAL_BYTES:
                raise InputFileError("Project bundle is too large to restore safely.")

            with tempfile.TemporaryDirectory(
                dir=destination.parent,
                prefix=".seattrellis-restore-",
            ) as temporary_dir:
                staging = Path(temporary_dir)
                for entry in entries:
                    if entry.is_dir():
                        continue
                    target = staging / _safe_archive_name(entry.filename)
                    target.parent.mkdir(parents=True, exist_ok=True)
                    with archive.open(entry) as source, target.open("wb") as output:
                        shutil.copyfileobj(source, output)
                restored_project = staging / project_name
                load_project(restored_project)
                destination.mkdir(parents=True, exist_ok=True)
                shutil.copytree(staging, destination, dirs_exist_ok=True)
    except (OSError, ValueError) as exc:
        if isinstance(exc, InputFileError):
            raise
        raise InputFileError(f"Could not restore project bundle {bundle}: {exc}") from exc
    return destination / project_name


def list_recent_projects(root: str | Path = ".", *, limit: int = 20) -> list[RecentProject]:
    """Find local project files for a simple recent-projects view."""

    if limit <= 0:
        raise ValueError("limit must be positive")
    directory = Path(root).expanduser().resolve()
    if not directory.is_dir():
        raise InputFileError(f"Projects directory not found: {directory}")
    results: list[RecentProject] = []
    candidates = {
        *directory.rglob("*.project.json"),
        *directory.rglob("*.seattrellis.json"),
    }
    for path in candidates:
        if any(part.startswith(".") for part in path.relative_to(directory).parts):
            continue
        try:
            project = load_project(path)
            modified_at = datetime.fromtimestamp(path.stat().st_mtime, tz=timezone.utc)
        except (InputFileError, OSError):
            continue
        results.append(RecentProject(path=path, name=project.name, modified_at=modified_at))
    results.sort(key=lambda item: (item.modified_at, item.path.name), reverse=True)
    return results[:limit]


def _add_file_reference(files: set[Path], path: Path, root: Path, label: str) -> None:
    resolved = path.resolve()
    _ensure_inside(resolved, root, label)
    if not resolved.is_file():
        raise InputFileError(f'Project reference "{label}" not found: {resolved}')
    files.add(resolved)


def _add_directory_files(files: set[Path], directory: Path, root: Path, label: str) -> None:
    resolved_directory = directory.resolve()
    _ensure_inside(resolved_directory, root, label)
    if not resolved_directory.is_dir():
        raise InputFileError(f'Project reference "{label}" is not a directory: {resolved_directory}')
    for path in resolved_directory.rglob("*"):
        if path.name == ".DS_Store" or path.is_dir():
            continue
        resolved = path.resolve()
        _ensure_inside(resolved, root, label)
        if not resolved.is_file() or path.is_symlink():
            continue
        files.add(resolved)


def _ensure_inside(path: Path, root: Path, label: str) -> None:
    try:
        path.relative_to(root)
    except ValueError as exc:
        raise InputFileError(
            f'Project reference "{label}" points outside the project root: {path}'
        ) from exc


def _scan_file(path: Path) -> set[str]:
    if path.stat().st_size > MAX_BUNDLE_FILE_BYTES:
        return set()
    try:
        text = path.read_text(encoding="utf-8")
    except (OSError, UnicodeDecodeError):
        return set()
    if path.suffix.lower() == ".csv":
        try:
            headers = next(csv.reader(text.splitlines()), [])
        except csv.Error:
            return set()
        return {header.strip() for header in headers if _is_sensitive_key(header)}
    if path.suffix.lower() != ".json":
        return set()
    try:
        data = json.loads(text)
    except json.JSONDecodeError:
        return set()
    return _sensitive_keys_in_json(data)


def _sensitive_keys_in_json(value: Any) -> set[str]:
    found: set[str] = set()
    if isinstance(value, dict):
        for key, child in value.items():
            if _is_sensitive_key(key):
                found.add(str(key))
            found.update(_sensitive_keys_in_json(child))
    elif isinstance(value, list):
        for child in value:
            found.update(_sensitive_keys_in_json(child))
    return found


def _is_sensitive_key(value: object) -> bool:
    normalized = str(value).strip().lower().replace("-", "_").replace(" ", "_")
    return normalized in _SENSITIVE_KEYS or normalized.endswith("_name")


def _read_manifest(archive: ZipFile) -> dict[str, Any]:
    try:
        data = json.loads(archive.read("manifest.json"))
    except KeyError as exc:
        raise InputFileError("Project bundle is missing manifest.json.") from exc
    except (UnicodeDecodeError, json.JSONDecodeError) as exc:
        raise InputFileError("Project bundle manifest is not valid UTF-8 JSON.") from exc
    if not isinstance(data, dict) or data.get("kind") != "seattrellis_project_bundle":
        raise InputFileError("Project bundle has an unknown manifest kind.")
    if data.get("format_version") != BUNDLE_FORMAT_VERSION:
        raise InputFileError(
            f"Unsupported project bundle format_version {data.get('format_version')!r}."
        )
    if not isinstance(data.get("project_file"), str) or not isinstance(data.get("files"), list):
        raise InputFileError("Project bundle manifest is incomplete.")
    if not all(isinstance(item, str) for item in data["files"]):
        raise InputFileError("Project bundle manifest files must be strings.")
    if len(data["files"]) != len(set(data["files"])):
        raise InputFileError("Project bundle manifest contains duplicate file entries.")
    return data


def _validated_entries(archive: ZipFile) -> list[ZipInfo]:
    entries: list[ZipInfo] = []
    seen: set[str] = set()
    for entry in archive.infolist():
        if entry.filename == "manifest.json" or entry.is_dir():
            continue
        name = _safe_archive_name(entry.filename)
        if name in seen:
            raise InputFileError(f"Project bundle contains duplicate file: {name}")
        seen.add(name)
        if entry.file_size > MAX_BUNDLE_FILE_BYTES:
            raise InputFileError(f"Project bundle file is too large: {name}")
        mode = (entry.external_attr >> 16) & 0o170000
        if mode == 0o120000:
            raise InputFileError(f"Project bundle cannot restore symlinks: {name}")
        entries.append(entry)
    return entries


def _safe_archive_name(value: str) -> str:
    path = PurePosixPath(value)
    if not value or path.is_absolute() or ".." in path.parts or "\\" in value:
        raise InputFileError(f"Unsafe project bundle path: {value!r}")
    return path.as_posix()


def _default_bundle_path(project_file: Path) -> Path:
    name = project_file.name
    for suffix in (".seattrellis.json", ".project.json", ".json"):
        if name.endswith(suffix):
            return project_file.with_name(f"{name[:-len(suffix)]}.seattrellis.zip")
    return project_file.with_name(f"{name}.seattrellis.zip")
