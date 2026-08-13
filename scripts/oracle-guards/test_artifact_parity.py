from __future__ import annotations

from datetime import datetime
import json
from pathlib import Path
import shutil

from seattrellis.api.handlers import (
    project_artifact_compare,
    project_artifact_restore,
)
from seattrellis.api.models import ProjectArtifactRequest


FIXTURE_ROOT = Path(__file__).parents[2] / "fixtures" / "artifact-parity"


def _is_rfc3339(value: object) -> bool:
    if not isinstance(value, str) or "T" not in value:
        return False
    try:
        datetime.fromisoformat(value.replace("Z", "+00:00"))
    except ValueError:
        return False
    return True


def _restore_summary(path: Path) -> dict[str, object]:
    document = json.loads(path.read_text(encoding="utf-8"))
    metadata = document.get("metadata", {})
    restored_at = metadata.get("restored_at")
    assert _is_rfc3339(restored_at), restored_at
    return {
        "schema_version": document.get("schema_version"),
        "kind": document.get("kind"),
        "restored_from": metadata.get("restored_from"),
        "restored_at": "<RFC3339>",
        "student_keys": [
            student.get("student_id") or student.get("name")
            for student in document.get("students", [])
        ],
        "assignments": [
            {
                "student_key": assignment.get("student_key"),
                "seat_id": assignment.get("seat_id"),
            }
            for assignment in document.get("assignments", [])
        ],
        "has_candidate_envelope": "candidates" in document,
    }


def test_artifact_compare_and_restore_match_shared_oracle_golden(tmp_path: Path) -> None:
    workspace = tmp_path / "artifact-parity"
    shutil.copytree(FIXTURE_ROOT, workspace)
    expected = json.loads((workspace / "expected.json").read_text(encoding="utf-8"))
    project = str(workspace / "project.json")
    left = str(workspace / "history" / "snapshot-left.json")
    right = str(workspace / "outputs" / "snapshot-right.json")
    candidates = str(workspace / "outputs" / "candidates.json")

    compare = project_artifact_compare(
        ProjectArtifactRequest(
            project_path=project,
            artifact_path=left,
            compare_to_path=right,
        )
    ).model_dump(mode="json")
    compare["left"]["path"] = Path(compare["left"]["path"]).name
    compare["right"]["path"] = Path(compare["right"]["path"]).name
    assert compare == expected["compare"]

    restored_snapshot = project_artifact_restore(
        ProjectArtifactRequest(project_path=project, artifact_path=left)
    )
    assert _restore_summary(Path(restored_snapshot.restored_artifact)) == expected[
        "restore_snapshot"
    ]

    restored_candidate = project_artifact_restore(
        ProjectArtifactRequest(project_path=project, artifact_path=candidates)
    )
    assert _restore_summary(Path(restored_candidate.restored_artifact)) == expected[
        "restore_candidate_set"
    ]
