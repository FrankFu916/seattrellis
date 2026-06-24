from __future__ import annotations

from typing import Sequence

from seattrellis import __version__
from seattrellis.history import fairness_metadata
from seattrellis.models.candidate import CandidatePlan, CandidateSet, MultiSolveOptions
from seattrellis.models.history import PairHistory, SeatHistory
from seattrellis.models.layout import ClassroomLayout
from seattrellis.models.rules import RuleSet
from seattrellis.models.snapshot import SeatingSnapshot
from seattrellis.models.student import Student
from seattrellis.scoring import apply_diversity_scores, refresh_recommendation, score_snapshot
from seattrellis.solver import SeatTrellisSolveError, solve_seating


def generate_candidate_set(
    students: list[Student],
    layout: ClassroomLayout,
    rules: RuleSet,
    *,
    history: SeatHistory | None = None,
    pair_history: PairHistory | None = None,
    history_snapshots: Sequence[SeatingSnapshot] | None = None,
    options: MultiSolveOptions | None = None,
    time_limit_seconds: float = 3.0,
) -> CandidateSet:
    options = options or MultiSolveOptions(seed=rules.seed)
    snapshots = list(history_snapshots or [])
    latest_snapshot = snapshots[-1] if snapshots else None
    candidates: list[CandidatePlan] = []
    excluded_assignments: list[dict[str, str]] = []
    warnings: list[str] = []
    failed_attempts = 0

    for attempt_index in range(options.attempt_limit):
        if len(candidates) >= options.candidate_count:
            break
        candidate_seed = options.seed + attempt_index
        try:
            solution = solve_seating(
                students,
                layout,
                rules,
                history=history,
                pair_history=pair_history,
                seed=candidate_seed,
                time_limit_seconds=time_limit_seconds,
                excluded_assignments=excluded_assignments,
            )
        except SeatTrellisSolveError:
            if not candidates:
                raise
            failed_attempts += 1
            continue

        assignment_map = solution.assignment_map
        if assignment_map in excluded_assignments:
            failed_attempts += 1
            continue

        candidate_id = f"candidate_{len(candidates) + 1:02d}"
        solver_backend = str(solution.metrics.get("solver", "unknown"))
        snapshot = solution.to_snapshot(
            students=students,
            layout=layout,
            rules=rules,
            seed=candidate_seed,
            metadata={
                "version": __version__,
                "candidate_id": candidate_id,
                "fairness": fairness_metadata(rules, history, pair_history),
            },
        )
        score = score_snapshot(
            snapshot,
            history=history,
            pair_history=pair_history,
            latest_snapshot=latest_snapshot,
        )
        if not score.breakdown.hard_constraint_summary.satisfied:
            raise SeatTrellisSolveError(
                f"{candidate_id} failed hard-constraint verification after solving."
            )
        candidates.append(
            CandidatePlan(
                candidate_id=candidate_id,
                snapshot=snapshot,
                score=score,
                hard_constraints_satisfied=True,
                warnings=[],
                metadata={
                    "random_seed": candidate_seed,
                    "solver_backend": solver_backend,
                    "history_count": history.history_count if history is not None else 0,
                    "pair_history_count": pair_history.history_count if pair_history is not None else 0,
                    "created_at": snapshot.created_at,
                },
            )
        )
        excluded_assignments.append(assignment_map)

    if len(candidates) < options.candidate_count:
        warnings.append(
            f"Requested {options.candidate_count} candidates but generated {len(candidates)} distinct "
            "hard-constraint-satisfying candidates."
        )
    if failed_attempts:
        warnings.append(
            f"{failed_attempts} generation attempts did not produce an additional distinct feasible plan."
        )

    apply_diversity_scores(candidates)
    candidate_set = CandidateSet(
        metadata={
            "project": "SeatTrellis",
            "version": __version__,
            "solver_backend": _solver_backend_summary(candidates),
            "candidate_count": len(candidates),
            "requested_candidate_count": options.candidate_count,
            "base_seed": options.seed,
            "history_count": history.history_count if history is not None else 0,
            "pair_history_count": pair_history.history_count if pair_history is not None else 0,
            "generation_method": "seeded repeated solve with exact-assignment exclusion",
        },
        candidates=candidates,
        recommended_candidate_id=candidates[0].candidate_id,
        warnings=warnings,
    )
    refresh_recommendation(candidate_set)
    return candidate_set


def _solver_backend_summary(candidates: Sequence[CandidatePlan]) -> str:
    backends = sorted(
        {str(candidate.metadata.get("solver_backend", "unknown")) for candidate in candidates}
    )
    return backends[0] if len(backends) == 1 else ",".join(backends)
