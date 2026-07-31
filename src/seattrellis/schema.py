"""Stable schema-version constants, validation helpers, and JSON Schema export."""

from __future__ import annotations

import json
from dataclasses import dataclass
from pathlib import Path
from typing import TypeVar


SNAPSHOT_SCHEMA_VERSION = "1.0"
CANDIDATE_SCHEMA_VERSION = "0.2.2"
PROJECT_SCHEMA_VERSION = 1
RULESET_SCHEMA_VERSION = 1
EDITOR_PROTOCOL_VERSION = "1.0"
JSON_SCHEMA_DRAFT = "http://json-schema.org/draft-07/schema#"

SchemaVersion = TypeVar("SchemaVersion", str, int)


@dataclass(frozen=True)
class JsonSchemaArtifact:
    """A model that can be exported as a public JSON Schema document."""

    name: str
    file_name: str
    title: str
    model_path: str
    schema_version: str | int | None = None


JSON_SCHEMA_ARTIFACTS: tuple[JsonSchemaArtifact, ...] = (
    JsonSchemaArtifact(
        name="student",
        file_name="student.schema.json",
        title="SeatTrellis Student",
        model_path="seattrellis.models.student:Student",
    ),
    JsonSchemaArtifact(
        name="classroom-layout",
        file_name="classroom-layout.schema.json",
        title="SeatTrellis Classroom Layout",
        model_path="seattrellis.models.layout:ClassroomLayout",
    ),
    JsonSchemaArtifact(
        name="ruleset",
        file_name="ruleset.schema.json",
        title="SeatTrellis RuleSet",
        model_path="seattrellis.models.rules:RuleSet",
        schema_version=RULESET_SCHEMA_VERSION,
    ),
    JsonSchemaArtifact(
        name="seating-snapshot",
        file_name="seating-snapshot.schema.json",
        title="SeatTrellis Seating Snapshot",
        model_path="seattrellis.models.snapshot:SeatingSnapshot",
        schema_version=SNAPSHOT_SCHEMA_VERSION,
    ),
    JsonSchemaArtifact(
        name="candidate-set",
        file_name="candidate-set.schema.json",
        title="SeatTrellis Candidate Set",
        model_path="seattrellis.models.candidate:CandidateSet",
        schema_version=CANDIDATE_SCHEMA_VERSION,
    ),
    JsonSchemaArtifact(
        name="plan-comparison-report",
        file_name="plan-comparison-report.schema.json",
        title="SeatTrellis Plan Comparison Report",
        model_path="seattrellis.models.candidate:PlanComparisonReport",
        schema_version=CANDIDATE_SCHEMA_VERSION,
    ),
    JsonSchemaArtifact(
        name="project",
        file_name="project.schema.json",
        title="SeatTrellis Project",
        model_path="seattrellis.models.project:SeatTrellisProject",
        schema_version=PROJECT_SCHEMA_VERSION,
    ),
    JsonSchemaArtifact(
        name="editor-command",
        file_name="editor-command.schema.json",
        title="SeatTrellis Editor Command",
        model_path="seattrellis.editing_protocol:EditorCommandEnvelope",
        schema_version=EDITOR_PROTOCOL_VERSION,
    ),
    JsonSchemaArtifact(
        name="editor-state",
        file_name="editor-state.schema.json",
        title="SeatTrellis Editor State",
        model_path="seattrellis.editing_protocol:EditorStateEnvelope",
        schema_version=EDITOR_PROTOCOL_VERSION,
    ),
)


def require_schema_version(
    value: object,
    *,
    expected: SchemaVersion,
    artifact: str,
) -> SchemaVersion:
    """Return a supported schema version or reject it with a clear message."""
    if type(value) is not type(expected) or value != expected:
        raise ValueError(
            f"Unsupported {artifact} schema_version {value!r}; "
            f"expected {expected!r}. Use a compatible SeatTrellis release and run "
            "'seattrellis schema migrate --input <file> --dry-run' before "
            "replacing the original file."
        )
    return expected


def json_schema_artifact_names() -> list[str]:
    """Return public JSON Schema artifact names in export order."""

    return [artifact.name for artifact in JSON_SCHEMA_ARTIFACTS]


def json_schema_documents() -> dict[str, dict[str, object]]:
    """Return all public JSON Schema documents keyed by file name."""

    return {
        artifact.file_name: json_schema_document(artifact)
        for artifact in JSON_SCHEMA_ARTIFACTS
    }


def json_schema_document(artifact: str | JsonSchemaArtifact) -> dict[str, object]:
    """Build one public JSON Schema document."""

    definition = _artifact_by_name(artifact) if isinstance(artifact, str) else artifact
    model = _load_model(definition.model_path)
    schema = model.model_json_schema(ref_template="#/definitions/{model}")
    # Pydantic v2 emits the definition table under ``$defs`` while the ref
    # template above still points at ``#/definitions/...``.  Rename the table
    # so the public documents keep the v1 layout and all references resolve.
    if "$defs" in schema and "definitions" not in schema:
        schema["definitions"] = schema.pop("$defs")
    schema_body = dict(schema)
    schema_body.pop("title", None)
    document = {
        "$schema": JSON_SCHEMA_DRAFT,
        "$id": f"https://frankfu916.github.io/seattrellis/schemas/{definition.file_name}",
        "title": definition.title,
        **schema_body,
    }
    document["x-seattrellis-artifact"] = definition.name
    if definition.schema_version is not None:
        document["x-seattrellis-schema-version"] = definition.schema_version
    return document


def write_json_schema_files(output_dir: str | Path) -> list[Path]:
    """Write public JSON Schema files and return the created paths."""

    destination = Path(output_dir)
    destination.mkdir(parents=True, exist_ok=True)
    paths: list[Path] = []
    for file_name, document in json_schema_documents().items():
        path = destination / file_name
        path.write_text(
            json.dumps(document, ensure_ascii=False, indent=2) + "\n",
            encoding="utf-8",
        )
        paths.append(path)
    return paths


def format_json_schema_artifacts() -> str:
    """Return a compact human-readable list of exportable schema documents."""

    lines = ["Available JSON Schema artifacts:"]
    for artifact in JSON_SCHEMA_ARTIFACTS:
        version = (
            f" schema_version={artifact.schema_version!r}"
            if artifact.schema_version is not None
            else ""
        )
        lines.append(f"- {artifact.name} -> {artifact.file_name}{version}")
    return "\n".join(lines)


def _artifact_by_name(name: str) -> JsonSchemaArtifact:
    for artifact in JSON_SCHEMA_ARTIFACTS:
        if artifact.name == name or artifact.file_name == name:
            return artifact
    available = ", ".join(json_schema_artifact_names())
    raise ValueError(f"Unknown JSON Schema artifact {name!r}. Available: {available}.")


def _load_model(path: str):
    module_name, _, attribute = path.partition(":")
    if not module_name or not attribute:
        raise ValueError(f"Invalid model path: {path}")
    import importlib

    module = importlib.import_module(module_name)
    return getattr(module, attribute)
