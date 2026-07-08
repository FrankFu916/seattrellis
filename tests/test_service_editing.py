from __future__ import annotations

import pytest

from seattrellis.editing import EditingError, EditingOperation
from seattrellis.io.json_files import load_snapshot, write_json_model
from seattrellis.io.project import write_project
from seattrellis.models.candidate import (
    CandidatePlan,
    CandidateSet,
    HardConstraintSummary,
    PlanScore,
    ScoreBreakdown,
    ScoreDimension,
)
from seattrellis.models.layout import ClassroomLayout, SeatNode
from seattrellis.models.rules import HardRules, PairRule, RuleSet
from seattrellis.models.snapshot import SeatAssignment, SeatingSnapshot
from seattrellis.models.project import SeatTrellisProject
from seattrellis.models.student import Student
from seattrellis.service import compute_edit, edit_snapshot, project_edit
from seattrellis.service_types import EditInput


def test_compute_edit_applies_operations_and_returns_draft_state() -> None:
    result = compute_edit(
        EditInput(
            snapshot=_snapshot(),
            operations=[
                EditingOperation(
                    kind="swap_students",
                    payload={"first_student": "s1", "second_student": "s2"},
                ),
                EditingOperation(kind="lock_seat", payload={"seat_id": "B2"}),
            ],
        )
    )

    assert result.hard_constraints.satisfied
    assert _seat_for(result.snapshot, "s1") == "A2"
    assert _seat_for(result.snapshot, "s2") == "A1"
    assert result.locked_seats == ["B2"]
    assert result.locked_students == []
    assert result.unseated_students == []
    assert [record.operation.kind for record in result.operation_log] == [
        "swap_students",
        "lock_seat",
    ]


def test_compute_edit_accepts_initial_locks() -> None:
    with pytest.raises(EditingError, match="Student is locked"):
        compute_edit(
            EditInput(
                snapshot=_snapshot(),
                locked_students=("s1",),
                operations=[
                    EditingOperation(
                        kind="move_student",
                        payload={"student_key": "s1", "seat_id": "B2"},
                    )
                ],
            )
        )


def test_compute_edit_reports_unseated_draft_and_hard_constraint_summary() -> None:
    result = compute_edit(
        EditInput(
            snapshot=_snapshot(),
            operations=[
                EditingOperation(
                    kind="move_student",
                    payload={"student_key": "s1", "seat_id": "A2"},
                )
            ],
        )
    )

    assert not result.hard_constraints.satisfied
    assert result.unseated_students == ["s2"]
    assert "Assignments do not contain every current student exactly once." in (
        result.hard_constraints.violations
    )


def test_compute_edit_reuses_hard_constraint_diagnostics() -> None:
    result = compute_edit(
        EditInput(
            snapshot=_snapshot(
                rules=RuleSet(
                    hard=HardRules(cannot_be_adjacent=[PairRule(students=("s1", "s2"))])
                ),
                assignments=[
                    SeatAssignment(student_key="s1", student_name="林安", seat_id="A1"),
                    SeatAssignment(student_key="s2", student_name="周雨", seat_id="B1"),
                    SeatAssignment(student_key="s3", student_name="许然", seat_id="A2"),
                ],
            ),
            operations=[
                EditingOperation(
                    kind="swap_students",
                    payload={"first_student": "s2", "second_student": "s3"},
                )
            ],
        )
    )

    assert not result.hard_constraints.satisfied
    assert "cannot_be_adjacent is not satisfied for ('s1', 's2')." in (
        result.hard_constraints.violations
    )


def test_edit_snapshot_writes_draft_and_strict_rejects_violations(tmp_path) -> None:
    snapshot_path = write_json_model(_snapshot(), tmp_path / "input.snapshot.json")
    draft_path = tmp_path / "draft.snapshot.json"
    strict_path = tmp_path / "strict.snapshot.json"

    path, summary = edit_snapshot(
        snapshot_path=snapshot_path,
        output_path=draft_path,
        operations=[
            EditingOperation(
                kind="move_student",
                payload={"student_key": "s1", "seat_id": "A2"},
            )
        ],
    )

    assert path == draft_path
    assert "hard constraints: not satisfied" in summary
    draft = load_snapshot(draft_path)
    assert draft.assignments[0].seat_id == "A2"
    assert draft.metadata["manual_edit"]["operation_count"] == 1
    assert draft.metadata["manual_edit"]["operations"][0]["kind"] == "move_student"
    assert draft.metadata["manual_edit"]["hard_constraints_satisfied"] is False

    with pytest.raises(EditingError):
        edit_snapshot(
            snapshot_path=snapshot_path,
            output_path=strict_path,
            operations=[
                EditingOperation(kind="lock_seat", payload={"seat_id": "A2"}),
                EditingOperation(
                    kind="move_student",
                    payload={"student_key": "s1", "seat_id": "A2"},
                ),
            ],
            strict=True,
        )
    assert not strict_path.exists()

    with pytest.raises(ValueError, match="hard constraints"):
        edit_snapshot(
            snapshot_path=snapshot_path,
            output_path=strict_path,
            operations=[
                EditingOperation(
                    kind="move_student",
                    payload={"student_key": "s1", "seat_id": "A2"},
                )
            ],
            strict=True,
        )
    assert not strict_path.exists()


def test_edit_snapshot_selects_recommended_candidate_by_default(tmp_path) -> None:
    candidate_path = write_json_model(_candidate_set(), tmp_path / "candidates.json")
    output_path = tmp_path / "recommended-edited.snapshot.json"

    path, summary = edit_snapshot(
        snapshot_path=candidate_path,
        output_path=output_path,
        operations=[
            EditingOperation(
                kind="swap_students",
                payload={"first_student": "s1", "second_student": "s2"},
            )
        ],
    )

    edited = load_snapshot(path)
    assert path == output_path
    assert "hard constraints: satisfied" in summary
    assert edited.metadata["candidate"]["candidate_id"] == "candidate_02"
    assert edited.metadata["manual_edit"]["operation_count"] == 1
    assert _seat_for(edited, "s1") == "A2"
    assert _seat_for(edited, "s2") == "B1"


def test_edit_snapshot_can_select_candidate_and_rejects_candidate_for_snapshot(tmp_path) -> None:
    candidate_path = write_json_model(_candidate_set(), tmp_path / "candidates.json")
    snapshot_path = write_json_model(_snapshot(), tmp_path / "input.snapshot.json")

    selected_path, _summary = edit_snapshot(
        snapshot_path=candidate_path,
        output_path=tmp_path / "candidate-01-edited.snapshot.json",
        candidate_id="candidate_01",
        operations=[
            EditingOperation(
                kind="swap_students",
                payload={"first_student": "s1", "second_student": "s2"},
            )
        ],
    )

    selected = load_snapshot(selected_path)
    assert selected.metadata["candidate"]["candidate_id"] == "candidate_01"
    assert _seat_for(selected, "s1") == "A2"
    assert _seat_for(selected, "s2") == "A1"

    with pytest.raises(ValueError, match="candidate set"):
        edit_snapshot(
            snapshot_path=snapshot_path,
            output_path=tmp_path / "invalid.snapshot.json",
            candidate_id="recommended",
            operations=[
                EditingOperation(kind="unseat_student", payload={"student_key": "s1"})
            ],
        )


def test_project_edit_uses_latest_project_artifact_by_default(tmp_path) -> None:
    project_dir = tmp_path / "class-a"
    outputs_dir = project_dir / "outputs"
    outputs_dir.mkdir(parents=True)
    project_path = write_project(
        SeatTrellisProject(
            name="Class A",
            students="students.csv",
            layout="classroom.json",
            rules="rules.json",
            outputs_dir="outputs",
        ),
        project_dir / "project.seattrellis.json",
    )
    write_json_model(_candidate_set(), outputs_dir / "latest.candidates.json")

    path, summary = project_edit(
        project_path=project_path,
        operations=[
            EditingOperation(
                kind="swap_students",
                payload={"first_student": "s1", "second_student": "s2"},
            )
        ],
    )

    edited = load_snapshot(path)
    assert path == outputs_dir / "latest.edited.snapshot.json"
    assert "hard constraints: satisfied" in summary
    assert edited.metadata["candidate"]["candidate_id"] == "candidate_02"
    assert edited.metadata["manual_edit"]["operation_count"] == 1


def _snapshot(
    *,
    assignments: list[SeatAssignment] | None = None,
    rules: RuleSet | None = None,
) -> SeatingSnapshot:
    students = [
        Student(student_id="s1", name="林安"),
        Student(student_id="s2", name="周雨"),
        Student(student_id="s3", name="许然"),
    ]
    layout = ClassroomLayout(
        seats=[
            SeatNode(seat_id="A1", row=1, col=1),
            SeatNode(seat_id="A2", row=1, col=2),
            SeatNode(seat_id="B1", row=2, col=1),
            SeatNode(seat_id="B2", row=2, col=2),
        ]
    )
    return SeatingSnapshot(
        seed=7,
        students=students,
        layout=layout,
        rules=rules or RuleSet(seed=7),
        assignments=assignments
        or [
            SeatAssignment(student_key="s1", student_name="林安", seat_id="A1"),
            SeatAssignment(student_key="s2", student_name="周雨", seat_id="A2"),
            SeatAssignment(student_key="s3", student_name="许然", seat_id="B1"),
        ],
        solver_status="manual-service-test",
    )


def _seat_for(snapshot: SeatingSnapshot, student_key: str) -> str:
    return {
        assignment.student_key: assignment.seat_id
        for assignment in snapshot.assignments
    }[student_key]


def _candidate_set() -> CandidateSet:
    first = _snapshot()
    second = _snapshot(
        assignments=[
            SeatAssignment(student_key="s1", student_name="林安", seat_id="B1"),
            SeatAssignment(student_key="s2", student_name="周雨", seat_id="A2"),
            SeatAssignment(student_key="s3", student_name="许然", seat_id="A1"),
        ]
    )
    return CandidateSet(
        candidates=[
            CandidatePlan(
                candidate_id="candidate_01",
                snapshot=first,
                score=_plan_score(80),
                hard_constraints_satisfied=True,
            ),
            CandidatePlan(
                candidate_id="candidate_02",
                snapshot=second,
                score=_plan_score(90),
                hard_constraints_satisfied=True,
            ),
        ],
        recommended_candidate_id="candidate_02",
    )


def _plan_score(total: float) -> PlanScore:
    dimension = ScoreDimension(status="not_available")
    return PlanScore(
        total=total,
        breakdown=ScoreBreakdown(
            fair_rotation_score=dimension,
            avoid_recent_neighbors_score=dimension,
            score_balance_score=dimension,
            height_preference_score=dimension,
            vision_preference_score=dimension,
            diversity_score=dimension,
            stability_score=dimension,
            hard_constraint_summary=HardConstraintSummary(
                satisfied=True,
                checked_rule_count=0,
                violation_count=0,
            ),
        ),
    )
