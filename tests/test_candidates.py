from __future__ import annotations

import json

import pytest

import seattrellis.solver.cp_sat as cp_sat
from seattrellis import cli
from seattrellis.candidates import generate_candidate_set
from seattrellis.history import build_pair_history, build_seat_history, load_history_snapshots
from seattrellis.io.json_files import (
    load_candidate_set,
    load_layout,
    load_rules,
    load_snapshot,
    write_json_model,
)
from seattrellis.io.students import read_students
from seattrellis.models.candidate import MultiSolveOptions
from seattrellis.models.layout import ClassroomLayout, SeatNode
from seattrellis.models.rules import RuleSet
from seattrellis.models.student import Student
from seattrellis.solver.adjacency import build_adjacency_edges, normalize_edge


def _example_inputs():
    students = read_students("examples/students.csv")
    layout = load_layout("examples/classroom.json")
    rules = load_rules("examples/rules_multi_candidate.json")
    snapshots = load_history_snapshots(history_dir="examples/history")
    history = build_seat_history(students, layout, snapshots)
    pair_rule = rules.soft.avoid_recent_neighbors
    pair_history = build_pair_history(
        students,
        layout,
        snapshots,
        lookback=pair_rule.lookback,
        within_distance=pair_rule.within_distance,
    )
    return students, layout, rules, snapshots, history, pair_history


def _generate(count: int = 3):
    students, layout, rules, snapshots, history, pair_history = _example_inputs()
    candidate_set = generate_candidate_set(
        students,
        layout,
        rules,
        history=history,
        pair_history=pair_history,
        history_snapshots=snapshots,
        options=MultiSolveOptions(candidate_count=count, seed=42),
    )
    return candidate_set, layout


def _assignment_tuple(candidate) -> tuple[tuple[str, str], ...]:
    return tuple(
        sorted(
            (assignment.student_key, assignment.seat_id)
            for assignment in candidate.snapshot.assignments
        )
    )


def test_candidates_one_keeps_single_snapshot_output(tmp_path) -> None:
    output = cli.solve(
        students_path="examples/students.csv",
        layout_path="examples/classroom.json",
        rules_path="examples/rules_multi_candidate.json",
        history_dir="examples/history",
        candidate_count=1,
        output_path=tmp_path / "latest.snapshot.json",
    )

    snapshot = load_snapshot(output)

    assert snapshot.schema_version == "1.0"
    assert len(snapshot.assignments) == 8


def test_generate_three_distinct_scored_candidates() -> None:
    candidate_set, _layout = _generate(3)

    assert len(candidate_set.candidates) == 3
    assert len({_assignment_tuple(candidate) for candidate in candidate_set.candidates}) == 3
    assert candidate_set.recommended_candidate_id in {
        candidate.candidate_id for candidate in candidate_set.candidates
    }
    assert all(candidate.hard_constraints_satisfied for candidate in candidate_set.candidates)
    assert all(
        candidate.score.breakdown.fair_rotation_score.status == "available"
        for candidate in candidate_set.candidates
    )
    assert all(
        candidate.score.breakdown.avoid_recent_neighbors_score.status == "available"
        for candidate in candidate_set.candidates
    )


def test_candidate_generation_is_reproducible_for_seed() -> None:
    first, _layout = _generate(3)
    second, _layout = _generate(3)

    assert [_assignment_tuple(candidate) for candidate in first.candidates] == [
        _assignment_tuple(candidate) for candidate in second.candidates
    ]
    assert [candidate.total_score for candidate in first.candidates] == [
        candidate.total_score for candidate in second.candidates
    ]


def test_candidate_set_can_be_saved_and_loaded(tmp_path) -> None:
    candidate_set, _layout = _generate(3)
    path = write_json_model(candidate_set, tmp_path / "candidates.json")

    loaded = load_candidate_set(path)

    assert loaded.kind == "candidate_set"
    assert loaded.recommended_candidate_id == candidate_set.recommended_candidate_id
    assert len(loaded.candidates) == 3


def test_no_history_still_generates_candidates_with_not_available_scores() -> None:
    students = [Student(student_id=f"S{index}", name=f"Student{index}") for index in range(1, 4)]
    layout = ClassroomLayout(
        seats=[SeatNode(seat_id=f"A{index}", row=1, col=index) for index in range(1, 4)]
    )
    rules = RuleSet(seed=7)

    candidate_set = generate_candidate_set(
        students,
        layout,
        rules,
        options=MultiSolveOptions(candidate_count=3, seed=7),
    )

    assert len(candidate_set.candidates) == 3
    for candidate in candidate_set.candidates:
        assert candidate.score.breakdown.fair_rotation_score.status == "not_available"
        assert candidate.score.breakdown.avoid_recent_neighbors_score.status == "not_available"


def test_insufficient_distinct_candidates_produces_warning() -> None:
    students = [Student(student_id="S1", name="Student1")]
    layout = ClassroomLayout(seats=[SeatNode(seat_id="A1", row=1, col=1)])

    candidate_set = generate_candidate_set(
        students,
        layout,
        RuleSet(seed=3),
        options=MultiSolveOptions(candidate_count=3, seed=3),
    )

    assert len(candidate_set.candidates) == 1
    assert any("Requested 3 candidates" in warning for warning in candidate_set.warnings)


def test_hard_fixed_and_cannot_be_adjacent_rules_hold_for_all_candidates() -> None:
    candidate_set, layout = _generate(3)
    edges = build_adjacency_edges(layout)

    for candidate in candidate_set.candidates:
        assignments = {
            assignment.student_key: assignment.seat_id
            for assignment in candidate.snapshot.assignments
        }
        assert assignments["STU001"] == "R1C1"
        assert normalize_edge(assignments["STU004"], assignments["STU005"]) not in edges
        assert candidate.score.breakdown.hard_constraint_summary.satisfied is True


def test_cli_writes_candidate_set_and_plan_report(tmp_path) -> None:
    output, summary = cli.solve_with_report(
        students_path="examples/students.csv",
        layout_path="examples/classroom.json",
        rules_path="examples/rules_multi_candidate.json",
        history_dir="examples/history",
        candidate_count=3,
        output_path=tmp_path / "candidates.json",
        report_path=tmp_path / "plan-report.json",
    )

    candidate_set = load_candidate_set(output)
    report = json.loads((tmp_path / "plan-report.json").read_text(encoding="utf-8"))

    assert len(candidate_set.candidates) == 3
    assert report["candidate_count"] == 3
    assert report["recommended_candidate_id"] == candidate_set.recommended_candidate_id
    assert report["candidates"][0]["history_comparison"]["fair_rotation"] in {
        "improved",
        "similar",
        "worse",
    }
    assert "Generated 3 candidate seating plans." in (summary or "")


def test_export_candidate_set_defaults_to_recommended_and_accepts_id(tmp_path) -> None:
    candidate_set, _layout = _generate(3)
    bundle_path = write_json_model(candidate_set, tmp_path / "candidates.json")

    recommended_path = cli.export(
        snapshot_path=bundle_path,
        output_format="html",
        output_path=tmp_path / "recommended.html",
    )
    selected_path = cli.export(
        snapshot_path=bundle_path,
        candidate_id="candidate_02",
        output_format="html",
        output_path=tmp_path / "candidate_02.html",
    )

    assert candidate_set.recommended_candidate_id in recommended_path.read_text(encoding="utf-8")
    assert "candidate_02" in selected_path.read_text(encoding="utf-8")


def test_export_candidate_set_rejects_unknown_candidate(tmp_path) -> None:
    candidate_set, _layout = _generate(2)
    bundle_path = write_json_model(candidate_set, tmp_path / "candidates.json")

    with pytest.raises(ValueError, match="Unknown candidate ID"):
        cli.export(
            snapshot_path=bundle_path,
            candidate_id="candidate_99",
            output_format="html",
            output_path=tmp_path / "missing.html",
        )


@pytest.mark.parametrize(
    ("output_format", "dependency", "filename"),
    [("excel", "openpyxl", "candidate.xlsx"), ("png", "PIL", "candidate.png")],
)
def test_candidate_set_optional_exports_do_not_regress(
    tmp_path, output_format: str, dependency: str, filename: str
) -> None:
    pytest.importorskip(dependency)
    candidate_set, _layout = _generate(2)
    bundle_path = write_json_model(candidate_set, tmp_path / "candidates.json")

    output = cli.export(
        snapshot_path=bundle_path,
        candidate_id="candidate_02",
        output_format=output_format,
        output_path=tmp_path / filename,
    )

    assert output.exists()
    assert output.stat().st_size > 0


def test_ortools_multi_candidate_generation_supports_assignment_exclusion(monkeypatch) -> None:
    ortools_cp_model = pytest.importorskip("ortools.sat.python.cp_model")
    monkeypatch.setenv("SEATTRELLIS_USE_ORTOOLS", "1")
    monkeypatch.setattr(cp_sat, "cp_model", ortools_cp_model)
    monkeypatch.setattr(cp_sat, "_cp_model_unavailable", False)
    students = [Student(student_id=f"S{index}", name=f"Student{index}") for index in range(1, 4)]
    layout = ClassroomLayout(
        seats=[SeatNode(seat_id=f"A{index}", row=1, col=index) for index in range(1, 4)]
    )

    candidate_set = generate_candidate_set(
        students,
        layout,
        RuleSet(seed=9),
        options=MultiSolveOptions(candidate_count=3, seed=9),
    )

    assert len(candidate_set.candidates) == 3
    assert len({_assignment_tuple(candidate) for candidate in candidate_set.candidates}) == 3
    assert {
        candidate.metadata["solver_backend"] for candidate in candidate_set.candidates
    } == {"ortools-cp-sat"}
