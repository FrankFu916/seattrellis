from __future__ import annotations

import pytest

import seattrellis.service as service_module
from seattrellis.editing import (
    EditingError,
    EditingLockState,
    EditingOperation,
    EditingSession,
    snapshot_with_lock_state,
)
from seattrellis.exporters.html import export_html
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
from seattrellis.models.rules import FixedSeatRule, HardRules, PairRule, RuleSet
from seattrellis.models.snapshot import SeatAssignment, SeatingSnapshot
from seattrellis.models.project import SeatTrellisProject
from seattrellis.models.student import Student
from seattrellis.service import (
    compute_edit,
    compute_repair,
    edit_snapshot,
    project_edit,
    project_repair,
    repair_snapshot,
)
from seattrellis.service_types import EditInput, RepairInput
from seattrellis.solver import SeatTrellisSolveError
from seattrellis.solver.result import SeatingSolution


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
    assert result.lock_state.locked_seats == ("B2",)
    assert result.snapshot.metadata["lock_state"] == {
        "locked_students": [],
        "locked_seats": ["B2"],
    }
    assert [record.operation.kind for record in result.operation_log] == [
        "swap_students",
        "lock_seat",
    ]


@pytest.mark.parametrize("locked_students", [("s1",), "s1"])
def test_compute_edit_accepts_initial_locks(locked_students) -> None:
    with pytest.raises(EditingError, match="Student is locked"):
        compute_edit(
            EditInput(
                snapshot=_snapshot(),
                locked_students=locked_students,
                operations=[
                    EditingOperation(
                        kind="move_student",
                        payload={"student_key": "s1", "seat_id": "B2"},
                    )
                ],
            )
        )


def test_compute_edit_merges_saved_and_explicit_locks() -> None:
    snapshot = snapshot_with_lock_state(
        _snapshot(),
        EditingLockState.from_values(locked_students=("s1",)),
    )

    result = compute_edit(
        EditInput(
            snapshot=snapshot,
            locked_seats=("B2",),
            operations=[
                EditingOperation(
                    kind="unlock_student",
                    payload={"student_key": "s1"},
                )
            ],
        )
    )

    assert result.lock_state.locked_students == ()
    assert result.lock_state.locked_seats == ("B2",)


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
    assert result.snapshot.solver_status == "MANUAL_DRAFT"
    assert result.snapshot.objective_value is None
    assert result.snapshot.metrics["manual_edit"] == {
        "operation_count": 1,
        "hard_constraints_satisfied": False,
        "violation_count": 1,
    }


def test_batch_move_is_atomic_and_undoable() -> None:
    session = EditingSession.from_snapshot(_snapshot())

    summary = session.batch_move(
        {
            "s1": "A2",
            "s2": "B1",
            "s3": "A1",
        }
    )

    assert summary.satisfied
    assert _seat_for(session.current_snapshot(), "s1") == "A2"
    assert _seat_for(session.current_snapshot(), "s2") == "B1"
    assert _seat_for(session.current_snapshot(), "s3") == "A1"
    assert [record.operation.kind for record in session.operation_log] == [
        "batch_move"
    ]

    session.undo()
    assert _seat_for(session.current_snapshot(), "s1") == "A1"
    assert _seat_for(session.current_snapshot(), "s2") == "A2"
    assert _seat_for(session.current_snapshot(), "s3") == "B1"
    session.redo()
    assert _seat_for(session.current_snapshot(), "s1") == "A2"


@pytest.mark.parametrize(
    ("moves", "message"),
    [
        (
            [
                {"student_key": "s1", "seat_id": "B2"},
                {"student_key": "s2", "seat_id": "B2"},
            ],
            "duplicate target seats",
        ),
        (
            [{"student_key": "s1", "seat_id": "A2"}],
            "outside the batch",
        ),
    ],
)
def test_batch_move_rejects_invalid_batch_without_partial_changes(
    moves,
    message,
) -> None:
    session = EditingSession.from_snapshot(_snapshot())
    before = session.current_snapshot()

    with pytest.raises(EditingError, match=message):
        session.apply(
            EditingOperation(kind="batch_move", payload={"moves": moves})
        )

    assert session.current_snapshot().assignments == before.assignments
    assert session.operation_log == ()


def test_compute_edit_serializes_batch_move_metadata() -> None:
    result = compute_edit(
        EditInput(
            snapshot=_snapshot(),
            operations=[
                EditingOperation(
                    kind="batch_move",
                    payload={
                        "moves": [
                            {"student_key": "s1", "seat_id": "B2"},
                            {"student_key": "s2", "seat_id": "A1"},
                        ]
                    },
                )
            ],
        )
    )

    operation = result.operation_log[0].operation
    assert operation.kind == "batch_move"
    assert operation.payload["moves"] == [
        {"student_key": "s1", "seat_id": "B2"},
        {"student_key": "s2", "seat_id": "A1"},
    ]
    assert _seat_for(result.snapshot, "s1") == "B2"
    assert _seat_for(result.snapshot, "s2") == "A1"


@pytest.mark.parametrize("lock_kind", ["student", "seat"])
def test_batch_move_respects_locks_without_partial_changes(lock_kind) -> None:
    session = EditingSession.from_snapshot(_snapshot())
    if lock_kind == "student":
        session.lock_student("s1")
        message = "Student is locked"
    else:
        session.lock_seat("B2")
        message = "Seat is locked"
    before = session.current_snapshot()
    operation_count = len(session.operation_log)

    with pytest.raises(EditingError, match=message):
        session.batch_move({"s1": "B2", "s2": "A1"})

    assert session.current_snapshot().assignments == before.assignments
    assert len(session.operation_log) == operation_count


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


def test_compute_repair_preserves_locks_and_limits_changes_to_local_scope() -> None:
    snapshot = _snapshot(
        rules=RuleSet(
            seed=7,
            hard=HardRules(
                cannot_be_adjacent=[PairRule(students=("s1", "s2"))]
            ),
        )
    )

    result = compute_repair(
        RepairInput(
            snapshot=snapshot,
            affected_students=("s2", "s3"),
            locked_students=("s1",),
            backend="fallback",
            time_limit_seconds=1,
        )
    )

    assert result.hard_constraints.satisfied
    assert _seat_for(result.snapshot, "s1") == "A1"
    assert result.fixed_assignments == {"s1": "A1"}
    assert result.mutable_students == ["s2", "s3"]
    assert result.changed_students == ["s2", "s3"]
    assert result.snapshot.rules.hard.fixed_seats == []
    assert result.snapshot.metadata["repair"]["solver_backend"] == "fallback"


def test_compute_repair_reuses_saved_empty_seat_locks_without_mutating_layout() -> None:
    snapshot = _snapshot(
        assignments=[
            SeatAssignment(student_key="s1", student_name="林安", seat_id="A1"),
            SeatAssignment(student_key="s2", student_name="周雨", seat_id="A2"),
        ]
    )
    snapshot.metadata["manual_edit"] = {
        "locked_students": [],
        "locked_seats": ["B2"],
    }

    result = compute_repair(
        RepairInput(snapshot=snapshot, backend="fallback", time_limit_seconds=1)
    )

    assert result.hard_constraints.satisfied
    assert result.reserved_empty_seats == ["B2"]
    assert "B2" not in {_seat_for(result.snapshot, key) for key in ["s1", "s2", "s3"]}
    assert result.snapshot.layout.seat_by_id("B2").enabled is True
    assert result.snapshot.metadata["repair"]["reserved_empty_seats"] == ["B2"]
    assert "manual_edit" not in result.snapshot.metadata
    assert result.snapshot.metadata["source_manual_edit"]["locked_seats"] == ["B2"]


def test_compute_repair_rejects_a_student_that_is_both_affected_and_locked() -> None:
    with pytest.raises(ValueError, match="Affected students cannot also be locked"):
        compute_repair(
            RepairInput(
                snapshot=_snapshot(),
                affected_students=("s1",),
                locked_students=("s1",),
            )
        )


def test_compute_repair_reuses_pure_edit_lock_state_and_accepts_string_locks() -> None:
    edited = compute_edit(
        EditInput(
            snapshot=_snapshot(),
            operations=[
                EditingOperation(
                    kind="lock_student",
                    payload={"student_key": "s1"},
                )
            ],
        )
    )

    result = compute_repair(
        RepairInput(
            snapshot=edited.snapshot,
            affected_students=("s2", "s3"),
            backend="fallback",
            time_limit_seconds=1,
        )
    )
    explicit_string = compute_repair(
        RepairInput(
            snapshot=_snapshot(),
            affected_students=("s2", "s3"),
            locked_students="s1",
            backend="fallback",
            time_limit_seconds=1,
        )
    )

    assert result.lock_state.locked_students == ("s1",)
    assert result.fixed_assignments["s1"] == "A1"
    assert explicit_string.fixed_assignments["s1"] == "A1"


def test_compute_repair_counts_existing_fixed_rules_as_effective_locks() -> None:
    snapshot = _snapshot(
        rules=RuleSet(
            hard=HardRules(
                fixed_seats=[FixedSeatRule(student="s1", seat_id="A1")]
            )
        )
    )

    result = compute_repair(
        RepairInput(
            snapshot=snapshot,
            affected_students=("s1",),
            backend="fallback",
            time_limit_seconds=1,
        )
    )

    assert result.fixed_assignments["s1"] == "A1"
    assert "s1" not in result.mutable_students
    assert result.snapshot.metadata["repair"]["temporary_fixed_assignments"] == {
        "s2": "A2",
        "s3": "B1",
    }


def test_compute_repair_rejects_reserving_a_seat_required_by_hard_rules() -> None:
    snapshot = _snapshot(
        assignments=[
            SeatAssignment(student_key="s1", student_name="林安", seat_id="A1"),
            SeatAssignment(student_key="s2", student_name="周雨", seat_id="A2"),
        ],
        rules=RuleSet(
            hard=HardRules(
                fixed_seats=[FixedSeatRule(student="s3", seat_id="B2")]
            )
        ),
    )

    with pytest.raises(ValueError, match="Cannot reserve an empty locked seat"):
        compute_repair(
            RepairInput(snapshot=snapshot, locked_seats=("B2",))
        )


def test_compute_repair_rejects_a_solver_result_that_breaks_a_temporary_lock(
    monkeypatch,
) -> None:
    def invalid_solution(*_args, **_kwargs) -> SeatingSolution:
        return SeatingSolution(
            assignments=[
                SeatAssignment(student_key="s1", student_name="林安", seat_id="B2"),
                SeatAssignment(student_key="s2", student_name="周雨", seat_id="A2"),
                SeatAssignment(student_key="s3", student_name="许然", seat_id="B1"),
            ],
            solver_status="FEASIBLE",
            metrics={"solver_backend_effective": "fallback"},
        )

    monkeypatch.setattr(service_module, "solve_seating", invalid_solution)

    with pytest.raises(SeatTrellisSolveError, match="repair anchors"):
        compute_repair(
            RepairInput(
                snapshot=_snapshot(),
                locked_students=("s1",),
                backend="fallback",
            )
        )


def test_compute_repair_keeps_history_metrics() -> None:
    snapshot = _snapshot()
    history = _snapshot()

    result = compute_repair(
        RepairInput(
            snapshot=snapshot,
            history_snapshots=(history,),
            backend="fallback",
            time_limit_seconds=1,
        )
    )

    assert result.snapshot.metadata["repair"]["history_count"] == 1
    assert result.snapshot.metrics["fairness"]["history_count"] == 1


def test_edit_snapshot_writes_draft_and_strict_rejects_violations(tmp_path) -> None:
    source = _snapshot()
    source.solver_status = "FEASIBLE"
    source.objective_value = 1711
    source.metrics = {"solver_backend_effective": "fallback"}
    snapshot_path = write_json_model(source, tmp_path / "input.snapshot.json")
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
    assert draft.solver_status == "MANUAL_DRAFT"
    assert draft.objective_value is None
    assert draft.metadata["source_solution"] == {
        "created_at": source.created_at.isoformat(),
        "solver_status": "FEASIBLE",
        "objective_value": 1711.0,
        "metrics": {"solver_backend_effective": "fallback"},
    }

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


def test_edit_snapshot_respects_and_can_release_saved_student_locks(tmp_path) -> None:
    snapshot = snapshot_with_lock_state(
        _snapshot(),
        EditingLockState.from_values(locked_students=("s1",)),
    )
    snapshot_path = write_json_model(snapshot, tmp_path / "locked.snapshot.json")
    rejected_path = tmp_path / "rejected.snapshot.json"

    with pytest.raises(EditingError, match="Student is locked"):
        edit_snapshot(
            snapshot_path=snapshot_path,
            output_path=rejected_path,
            operations=[
                EditingOperation(
                    kind="move_student",
                    payload={"student_key": "s1", "seat_id": "B2"},
                )
            ],
        )
    assert not rejected_path.exists()

    unlocked_path, _summary = edit_snapshot(
        snapshot_path=snapshot_path,
        output_path=tmp_path / "unlocked.snapshot.json",
        operations=[
            EditingOperation(
                kind="unlock_student",
                payload={"student_key": "s1"},
            ),
            EditingOperation(
                kind="move_student",
                payload={"student_key": "s1", "seat_id": "B2"},
            ),
        ],
    )

    unlocked = load_snapshot(unlocked_path)
    assert _seat_for(unlocked, "s1") == "B2"
    assert unlocked.metadata["lock_state"]["locked_students"] == []


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
    assert edited.metadata["source_candidate"]["candidate_id"] == "candidate_02"
    assert "candidate" not in edited.metadata
    assert edited.metadata["manual_edit"]["operation_count"] == 1
    assert _seat_for(edited, "s1") == "A2"
    assert _seat_for(edited, "s2") == "B1"
    html = export_html(edited, tmp_path / "recommended-edited.html").read_text(
        encoding="utf-8"
    )
    assert "Score: 90" not in html


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
    assert selected.metadata["source_candidate"]["candidate_id"] == "candidate_01"
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


def test_repair_snapshot_selects_candidate_and_project_repair_uses_latest(tmp_path) -> None:
    candidate_path = write_json_model(_candidate_set(), tmp_path / "candidates.json")
    repaired_path = tmp_path / "candidate-repaired.snapshot.json"

    path, summary = repair_snapshot(
        snapshot_path=candidate_path,
        candidate_id="candidate_01",
        affected_students=("s1", "s2"),
        output_path=repaired_path,
        backend="fallback",
        time_limit_seconds=1,
    )

    repaired = load_snapshot(path)
    assert path == repaired_path
    assert "Repair summary:" in summary
    assert repaired.metadata["source_candidate"]["candidate_id"] == "candidate_01"
    assert "candidate" not in repaired.metadata
    assert repaired.metadata["repair"]["source_candidate_id"] == "candidate_01"
    assert repaired.metadata["repair"]["mutable_students"] == ["s1", "s2"]

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

    project_path_result, project_summary = project_repair(
        project_path=project_path,
        affected_students=("s1", "s2"),
        backend="fallback",
        time_limit_seconds=1,
    )

    assert project_path_result == outputs_dir / "latest.repaired.snapshot.json"
    assert "Repair summary:" in project_summary
    project_repaired = load_snapshot(project_path_result)
    assert project_repaired.metadata["source_candidate"]["candidate_id"] == "candidate_02"


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
    assert edited.metadata["source_candidate"]["candidate_id"] == "candidate_02"
    assert edited.metadata["manual_edit"]["operation_count"] == 1


def test_reediting_a_draft_retains_the_complete_operation_history(tmp_path) -> None:
    source_path = write_json_model(_snapshot(), tmp_path / "source.snapshot.json")
    first_path, _summary = edit_snapshot(
        snapshot_path=source_path,
        output_path=tmp_path / "first.snapshot.json",
        operations=[
            EditingOperation(
                kind="swap_students",
                payload={"first_student": "s1", "second_student": "s2"},
            )
        ],
    )

    second_path, _summary = edit_snapshot(
        snapshot_path=first_path,
        output_path=tmp_path / "second.snapshot.json",
        operations=[
            EditingOperation(
                kind="unseat_student",
                payload={"student_key": "s3"},
            )
        ],
    )

    second = load_snapshot(second_path)
    assert second.metadata["manual_edit"]["operation_count"] == 2
    assert [
        operation["kind"]
        for operation in second.metadata["manual_edit"]["operations"]
    ] == ["swap_students", "unseat_student"]
    assert second.metadata["source_solution"]["solver_status"] == (
        "manual-service-test"
    )


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
