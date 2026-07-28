from __future__ import annotations

import sys

from seattrellis.benchmarks import (
    BENCHMARK_DEFAULT_CANDIDATE_COUNTS,
    BENCHMARK_DEFAULT_PROFILES,
    BENCHMARK_DATASET_NAME,
    BENCHMARK_DATASET_VERSION,
    BENCHMARK_DEFAULT_SIZES,
    benchmark_case_id,
    benchmark_layout,
    benchmark_layout_shape,
    benchmark_rules,
    benchmark_run_id,
    benchmark_students,
)
from seattrellis.models.rules import RuleSet
from scripts.benchmark_solver import (
    BENCHMARK_PHASES,
    BenchmarkCase,
    BenchmarkResult,
    _candidate_diversity,
    _cases,
    _parse_args,
    run_case,
    summarize_results,
)


def test_quiet_report_output_is_opt_in(monkeypatch) -> None:
    monkeypatch.setattr(sys, "argv", ["benchmark_solver.py"])
    assert _parse_args().quiet is False

    monkeypatch.setattr(sys, "argv", ["benchmark_solver.py", "--quiet"])
    assert _parse_args().quiet is True


def test_default_benchmark_shapes_are_stable() -> None:
    assert BENCHMARK_DATASET_NAME == "synthetic-classroom"
    assert BENCHMARK_DATASET_VERSION == "synthetic-v1"
    assert BENCHMARK_DEFAULT_SIZES == (40, 50, 60)
    assert BENCHMARK_DEFAULT_PROFILES == ("light", "dense")
    assert BENCHMARK_DEFAULT_CANDIDATE_COUNTS == (1, 5, 20)
    assert {size: benchmark_layout_shape(size) for size in BENCHMARK_DEFAULT_SIZES} == {
        40: (5, 8),
        50: (5, 10),
        60: (6, 10),
    }


def test_benchmark_inputs_are_deterministic_and_fictional() -> None:
    students = benchmark_students(40)
    repeated_students = benchmark_students(40)
    layout = benchmark_layout(5, 8)
    repeated_layout = benchmark_layout(5, 8)

    assert [student.dict() for student in students] == [
        student.dict() for student in repeated_students
    ]
    assert layout.dict() == repeated_layout.dict()
    assert len(students) == 40
    assert len(layout.enabled_seats) == 40
    assert students[0].student_id == "STU001"
    assert students[-1].name == "Student040"
    assert all((student.name or "").startswith("Student") for student in students)
    assert layout.layout_id == "benchmark-5x8"


def test_benchmark_case_id_includes_dataset_version() -> None:
    assert benchmark_case_id(40, 5, 8) == "synthetic-v1-40-students-5x8"


def test_constraint_profiles_are_deterministic_and_dense_is_additive() -> None:
    students = benchmark_students(40)
    layout = benchmark_layout(5, 8)
    base_rules = RuleSet()

    light = benchmark_rules("light", students, layout, base_rules)
    dense = benchmark_rules("dense", students, layout, base_rules)
    repeated_dense = benchmark_rules("dense", students, layout, base_rules)

    assert light.dict() == base_rules.dict()
    assert dense.dict() == repeated_dense.dict()
    assert dense.soft.dict() == base_rules.soft.dict()
    assert len(dense.hard.fixed_seats) == 2
    assert len(dense.hard.cannot_be_adjacent) == 10
    assert len(dense.hard.min_distance) == 4
    assert base_rules.hard.dict() == RuleSet().hard.dict()


def test_case_matrix_keeps_scenario_id_and_has_unique_run_ids() -> None:
    cases = list(
        _cases(
            sizes=[40],
            backends=["fallback"],
            candidates=1,
            time_limit_seconds=1,
            profiles=["light", "dense"],
            candidate_counts=[1, 5, 20],
        )
    )

    assert len(cases) == 6
    assert {case.case_id for case in cases} == {
        "synthetic-v1-40-students-5x8"
    }
    run_ids = {
        benchmark_run_id(
            case.case_id,
            profile=case.profile,
            candidates=case.candidates,
            backend=case.backend,
        )
        for case in cases
    }
    assert len(run_ids) == len(cases)


def test_candidate_diversity_reports_pairwise_assignment_change() -> None:
    diversity = _candidate_diversity(
        [
            {"s1": "A1", "s2": "A2"},
            {"s1": "A2", "s2": "A1"},
            {"s1": "A1", "s2": "A2"},
        ]
    )

    assert diversity == {
        "metric": "pairwise_assignment_difference_percent",
        "distinct_candidates": 2,
        "pair_count": 3,
        "mean_pairwise_distance_percent": 66.666667,
        "min_pairwise_distance_percent": 0.0,
        "max_pairwise_distance_percent": 100.0,
    }


def test_run_case_reports_all_phases_feasibility_and_serialization() -> None:
    result = run_case(
        BenchmarkCase(
            case_id=benchmark_case_id(4, 5, 8),
            size=4,
            rows=5,
            cols=8,
            backend="fallback",
            candidates=1,
            time_limit_seconds=0.1,
        )
    )

    assert result.ok is True
    assert result.feasible is True
    assert result.candidate_yield_rate == 1.0
    assert result.generated_candidates == 1
    assert result.candidate_diversity["mean_pairwise_distance_percent"] is None
    assert result.serialized_bytes > 0
    assert result.failed_phase is None
    assert set(result.phase_elapsed_seconds) == set(BENCHMARK_PHASES)
    assert all(
        value is not None and value >= 0
        for value in result.phase_elapsed_seconds.values()
    )


def test_summary_does_not_count_unknown_results_as_infeasible() -> None:
    known = BenchmarkResult(
        case_id="known",
        dataset_version=BENCHMARK_DATASET_VERSION,
        size=40,
        rows=5,
        cols=8,
        backend="fallback",
        candidates=1,
        time_limit_seconds=1,
        ok=True,
        elapsed_seconds=0.1,
        feasible=True,
    )
    unknown = BenchmarkResult(
        case_id="unknown",
        dataset_version=BENCHMARK_DATASET_VERSION,
        size=40,
        rows=5,
        cols=8,
        backend="ortools",
        candidates=1,
        time_limit_seconds=1,
        ok=False,
        elapsed_seconds=1,
        feasible=None,
    )

    summary = summarize_results([known, unknown])

    assert summary["feasible_cases"] == 1
    assert summary["infeasible_cases"] == 0
    assert summary["unknown_feasibility_cases"] == 1
    assert summary["feasibility_rate"] == 1.0
