"""Execution helpers for the synthetic solver benchmark matrix."""

from __future__ import annotations

import json
import time
from dataclasses import dataclass
from itertools import combinations
from typing import Any, Iterable

from seattrellis.benchmark_report import (
    BENCHMARK_PHASES,
    BenchmarkResult,
    average,
    ratio,
)
from seattrellis.benchmarks import (
    BENCHMARK_DATASET_VERSION,
    benchmark_case_id,
    benchmark_layout,
    benchmark_layout_shape,
    benchmark_rules,
    benchmark_run_id,
    benchmark_students,
    normalize_benchmark_profile,
)
from seattrellis.models import (
    CandidatePlan,
    CandidateSet,
    ClassroomLayout,
    MultiSolveOptions,
    RuleSet,
    Student,
)
from seattrellis.optional import MissingOptionalDependencyError
from seattrellis.presets import get_preset
from seattrellis.scoring import apply_diversity_scores, refresh_recommendation, score_snapshot
from seattrellis.solver import SeatTrellisSolveError
from seattrellis.solver.cp_sat import solve_compiled
from seattrellis.solver.problem import compile_problem


@dataclass(frozen=True)
class BenchmarkCase:
    case_id: str
    size: int
    rows: int
    cols: int
    backend: str
    candidates: int
    time_limit_seconds: float
    profile: str = "light"
    max_attempts: int | None = None


def run_case(case: BenchmarkCase, *, preset_name: str = "daily") -> BenchmarkResult:
    started = time.perf_counter_ns()
    phase_elapsed_seconds: dict[str, float | None] = {
        phase: None for phase in BENCHMARK_PHASES
    }
    solutions: list[tuple[Any, int]] = []
    solve_attempts = 0
    failed_phase = "parse"
    phase_started = time.perf_counter_ns()
    try:
        source_students = benchmark_students(case.size)
        source_layout = benchmark_layout(case.rows, case.cols)
        source_rules = benchmark_rules(
            case.profile,
            source_students,
            source_layout,
            get_preset(preset_name).rules,
        )
        raw_input = json.loads(
            json.dumps(
                {
                    "students": [_model_to_data(student) for student in source_students],
                    "layout": _model_to_data(source_layout),
                    "rules": _model_to_data(source_rules),
                },
                ensure_ascii=False,
            )
        )
        students = [
            _parse_model(Student, student_data)
            for student_data in raw_input["students"]
        ]
        layout = _parse_model(ClassroomLayout, raw_input["layout"])
        rules = _parse_model(RuleSet, raw_input["rules"])
    except Exception as exc:
        phase_elapsed_seconds[failed_phase] = _elapsed_seconds(phase_started)
        return _failure_result(
            case,
            started=started,
            phase_elapsed_seconds=phase_elapsed_seconds,
            failed_phase=failed_phase,
            exc=exc,
        )
    phase_elapsed_seconds[failed_phase] = _elapsed_seconds(phase_started)

    failed_phase = "compile"
    phase_started = time.perf_counter_ns()
    try:
        problem = compile_problem(students, layout, rules)
    except Exception as exc:
        phase_elapsed_seconds[failed_phase] = _elapsed_seconds(phase_started)
        return _failure_result(
            case,
            started=started,
            phase_elapsed_seconds=phase_elapsed_seconds,
            failed_phase=failed_phase,
            exc=exc,
        )
    phase_elapsed_seconds[failed_phase] = _elapsed_seconds(phase_started)

    failed_phase = "solve"
    phase_started = time.perf_counter_ns()
    excluded_assignments: list[dict[str, str]] = []
    options = MultiSolveOptions(
        candidate_count=case.candidates,
        seed=rules.seed,
        max_attempts=case.max_attempts,
    )
    try:
        for attempt_index in range(options.attempt_limit):
            if len(solutions) >= case.candidates:
                break
            solve_attempts += 1
            seed = options.seed + attempt_index
            try:
                solution = solve_compiled(
                    problem,
                    seed=seed,
                    time_limit_seconds=case.time_limit_seconds,
                    excluded_assignments=excluded_assignments,
                    backend=case.backend,
                )
            except (SeatTrellisSolveError, MissingOptionalDependencyError):
                if not solutions:
                    raise
                continue
            assignment_map = solution.assignment_map
            if assignment_map in excluded_assignments:
                continue
            solutions.append((solution, seed))
            excluded_assignments.append(assignment_map)
    except Exception as exc:
        phase_elapsed_seconds[failed_phase] = _elapsed_seconds(phase_started)
        return _failure_result(
            case,
            started=started,
            phase_elapsed_seconds=phase_elapsed_seconds,
            failed_phase=failed_phase,
            exc=exc,
            solve_attempts=solve_attempts,
            generated_candidates=len(solutions),
        )
    phase_elapsed_seconds[failed_phase] = _elapsed_seconds(phase_started)

    failed_phase = "score"
    phase_started = time.perf_counter_ns()
    try:
        candidates: list[CandidatePlan] = []
        for index, (solution, seed) in enumerate(solutions, start=1):
            candidate_id = f"candidate_{index:02d}"
            snapshot = solution.to_snapshot(
                students=students,
                layout=layout,
                rules=rules,
                seed=seed,
                metadata={
                    "benchmark": True,
                    "constraint_profile": case.profile,
                    "candidate_id": candidate_id,
                },
            )
            score = score_snapshot(snapshot)
            candidates.append(
                CandidatePlan(
                    candidate_id=candidate_id,
                    snapshot=snapshot,
                    score=score,
                    hard_constraints_satisfied=(
                        score.breakdown.hard_constraint_summary.satisfied
                    ),
                    metadata={
                        "random_seed": seed,
                        "solver_backend": str(
                            solution.metrics.get("solver", "unknown")
                        ),
                    },
                )
            )
        apply_diversity_scores(candidates)
        candidate_set = CandidateSet(
            metadata={
                "benchmark": True,
                "constraint_profile": case.profile,
                "requested_candidate_count": case.candidates,
                "solve_attempts": solve_attempts,
            },
            candidates=candidates,
            recommended_candidate_id=candidates[0].candidate_id,
            warnings=(
                []
                if len(candidates) == case.candidates
                else [
                    f"Requested {case.candidates} candidates but generated "
                    f"{len(candidates)} distinct candidates."
                ]
            ),
        )
        feasible_candidates = sum(
            candidate.hard_constraints_satisfied for candidate in candidates
        )
        feasible = feasible_candidates > 0
        if feasible:
            refresh_recommendation(candidate_set)
        candidate_diversity = _candidate_diversity(
            [solution.assignment_map for solution, _seed in solutions]
        )
    except Exception as exc:
        phase_elapsed_seconds[failed_phase] = _elapsed_seconds(phase_started)
        return _failure_result(
            case,
            started=started,
            phase_elapsed_seconds=phase_elapsed_seconds,
            failed_phase=failed_phase,
            exc=exc,
            solve_attempts=solve_attempts,
            generated_candidates=len(solutions),
        )
    phase_elapsed_seconds[failed_phase] = _elapsed_seconds(phase_started)

    failed_phase = "serialization"
    phase_started = time.perf_counter_ns()
    try:
        serialized = _model_json(candidate_set)
        json.loads(serialized)
    except Exception as exc:
        phase_elapsed_seconds[failed_phase] = _elapsed_seconds(phase_started)
        return _failure_result(
            case,
            started=started,
            phase_elapsed_seconds=phase_elapsed_seconds,
            failed_phase=failed_phase,
            exc=exc,
            solve_attempts=solve_attempts,
            generated_candidates=len(solutions),
        )
    phase_elapsed_seconds[failed_phase] = _elapsed_seconds(phase_started)

    first_candidate = candidate_set.candidates[0]
    solver_backends = sorted(
        {
            str(candidate.metadata.get("solver_backend", "unknown"))
            for candidate in candidate_set.candidates
        }
    )
    solver_backend_effective = first_candidate.snapshot.metrics.get("solver_backend_effective")
    generated_candidates = len(candidate_set.candidates)
    return BenchmarkResult(
        case_id=case.case_id,
        run_id=benchmark_run_id(
            case.case_id,
            profile=case.profile,
            candidates=case.candidates,
            backend=case.backend,
        ),
        dataset_version=BENCHMARK_DATASET_VERSION,
        size=case.size,
        rows=case.rows,
        cols=case.cols,
        backend=case.backend,
        candidates=case.candidates,
        time_limit_seconds=case.time_limit_seconds,
        ok=feasible,
        elapsed_seconds=_elapsed_seconds(started),
        constraint_profile=case.profile,
        phase_elapsed_seconds=phase_elapsed_seconds,
        feasible=feasible,
        candidate_yield_rate=ratio(generated_candidates, case.candidates),
        candidate_diversity=candidate_diversity,
        solve_attempts=solve_attempts,
        feasible_candidates=feasible_candidates,
        serialized_bytes=len(serialized.encode("utf-8")),
        generated_candidates=generated_candidates,
        recommended_candidate_id=candidate_set.recommended_candidate_id,
        solver_backend=",".join(solver_backends),
        solver_backend_effective=str(solver_backend_effective) if solver_backend_effective else None,
        solver_status=first_candidate.snapshot.solver_status,
        error_type=None if feasible else "HardConstraintVerificationError",
        error=None if feasible else "No generated candidate passed hard-constraint verification.",
    )


def _failure_result(
    case: BenchmarkCase,
    *,
    started: int,
    phase_elapsed_seconds: dict[str, float | None],
    failed_phase: str,
    exc: Exception,
    solve_attempts: int = 0,
    generated_candidates: int = 0,
) -> BenchmarkResult:
    return BenchmarkResult(
        case_id=case.case_id,
        run_id=benchmark_run_id(
            case.case_id,
            profile=case.profile,
            candidates=case.candidates,
            backend=case.backend,
        ),
        dataset_version=BENCHMARK_DATASET_VERSION,
        size=case.size,
        rows=case.rows,
        cols=case.cols,
        backend=case.backend,
        candidates=case.candidates,
        time_limit_seconds=case.time_limit_seconds,
        ok=False,
        elapsed_seconds=_elapsed_seconds(started),
        constraint_profile=case.profile,
        phase_elapsed_seconds=phase_elapsed_seconds,
        # A timeout, unavailable backend, or runtime error does not prove that
        # the constraint set is infeasible.
        feasible=None,
        candidate_yield_rate=ratio(generated_candidates, case.candidates),
        candidate_diversity=_candidate_diversity([]),
        solve_attempts=solve_attempts,
        failed_phase=failed_phase,
        generated_candidates=generated_candidates,
        error_type=exc.__class__.__name__,
        error=str(exc),
    )


def _candidate_diversity(
    assignment_maps: list[dict[str, str]],
) -> dict[str, object]:
    distances: list[float] = []
    for first, second in combinations(assignment_maps, 2):
        student_keys = sorted(set(first) & set(second))
        if not student_keys:
            continue
        changed = sum(first[key] != second[key] for key in student_keys)
        distances.append(100.0 * changed / len(student_keys))
    distinct = {
        tuple(sorted(assignment.items()))
        for assignment in assignment_maps
    }
    return {
        "metric": "pairwise_assignment_difference_percent",
        "distinct_candidates": len(distinct),
        "pair_count": len(distances),
        "mean_pairwise_distance_percent": average(distances),
        "min_pairwise_distance_percent": (
            round(min(distances), 6) if distances else None
        ),
        "max_pairwise_distance_percent": (
            round(max(distances), 6) if distances else None
        ),
    }


def _elapsed_seconds(started: int) -> float:
    return round((time.perf_counter_ns() - started) / 1_000_000_000, 6)


def _model_to_data(model: Any) -> dict[str, Any]:
    if hasattr(model, "model_dump"):
        return model.model_dump(mode="json")
    return json.loads(model.json())


def _model_json(model: Any) -> str:
    if hasattr(model, "model_dump_json"):
        return model.model_dump_json()
    return model.json()


def _parse_model(model_type: type[Any], data: dict[str, Any]) -> Any:
    if hasattr(model_type, "model_validate"):
        return model_type.model_validate(data)
    return model_type.parse_obj(data)


def _cases(
    *,
    sizes: Iterable[int],
    backends: Iterable[str],
    candidates: int,
    time_limit_seconds: float,
    profiles: Iterable[str] = ("light",),
    candidate_counts: Iterable[int] | None = None,
    max_attempts: int | None = None,
) -> Iterable[BenchmarkCase]:
    counts = list(candidate_counts) if candidate_counts is not None else [candidates]
    for size in sizes:
        rows, cols = benchmark_layout_shape(size)
        for profile in profiles:
            normalized_profile = normalize_benchmark_profile(profile)
            for candidate_count in counts:
                for backend in backends:
                    yield BenchmarkCase(
                        case_id=benchmark_case_id(size, rows, cols),
                        size=size,
                        rows=rows,
                        cols=cols,
                        backend=backend,
                        candidates=candidate_count,
                        time_limit_seconds=time_limit_seconds,
                        profile=normalized_profile,
                        max_attempts=max_attempts,
                    )
