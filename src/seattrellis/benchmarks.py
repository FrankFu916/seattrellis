"""Synthetic benchmark datasets used for solver performance checks."""

from __future__ import annotations

from math import ceil

from seattrellis.models import (
    ClassroomLayout,
    FixedSeatRule,
    HardRules,
    MinDistanceRule,
    PairRule,
    RuleSet,
    SeatNode,
    Student,
)


BENCHMARK_DATASET_NAME = "synthetic-classroom"
BENCHMARK_DATASET_VERSION = "synthetic-v1"
BENCHMARK_DEFAULT_SIZES = (40, 50, 60)
BENCHMARK_DEFAULT_PROFILES = ("light", "dense")
BENCHMARK_DEFAULT_CANDIDATE_COUNTS = (1, 5, 20)

BENCHMARK_PROFILE_DESCRIPTIONS = {
    "light": "Rich fictional student data with the selected preset and no extra hard rules.",
    "dense": "The light profile plus deterministic fixed-seat, separation, and distance rules.",
}


def benchmark_case_id(
    size: int,
    rows: int,
    cols: int,
    *,
    profile: str = "light",
    candidates: int = 1,
) -> str:
    """Stable identifier for one synthetic dataset scenario.

    ``profile`` and ``candidates`` are accepted for source compatibility with
    early harness prototypes. They intentionally do not change the dataset
    scenario identifier; benchmark reports use a separate run ID for matrix
    dimensions and backend selection.
    """

    normalize_benchmark_profile(profile)
    if candidates <= 0:
        raise ValueError("benchmark candidates must be positive")
    return f"{BENCHMARK_DATASET_VERSION}-{size}-students-{rows}x{cols}"


def benchmark_run_id(
    case_id: str,
    *,
    profile: str,
    candidates: int,
    backend: str,
) -> str:
    """Return a unique stable ID for one benchmark matrix run."""

    normalized_profile = normalize_benchmark_profile(profile)
    return f"{case_id}-{normalized_profile}-{candidates}-candidates-{backend}"


def normalize_benchmark_profile(profile: str) -> str:
    """Return a supported benchmark profile name."""

    normalized = str(profile).strip().lower().replace("_", "-")
    if normalized not in BENCHMARK_DEFAULT_PROFILES:
        supported = ", ".join(BENCHMARK_DEFAULT_PROFILES)
        raise ValueError(
            f"Unsupported benchmark profile {profile!r}. Supported profiles: {supported}."
        )
    return normalized


def benchmark_layout_shape(size: int) -> tuple[int, int]:
    """Return the fixed layout shape for a benchmark class size."""

    if size <= 0:
        raise ValueError("benchmark size must be positive")
    if size <= 40:
        return 5, 8
    if size <= 50:
        return 5, 10
    if size <= 60:
        return 6, 10
    return ceil(size / 10), 10


def benchmark_students(count: int) -> list[Student]:
    """Return deterministic fictional students for benchmark runs."""

    if count <= 0:
        raise ValueError("student count must be positive")
    return [
        Student(
            student_id=f"STU{i:03d}",
            name=f"Student{i:03d}",
            gender="F" if i % 2 else "M",
            height_cm=float(145 + (i * 7) % 42),
            score=float(55 + (i * 11) % 45),
            vision="poor" if i % 13 == 0 else None,
            tags=["leader"] if i % 17 == 0 else [],
            needs=["vision_front"] if i % 19 == 0 else [],
        )
        for i in range(1, count + 1)
    ]


def benchmark_layout(rows: int, cols: int) -> ClassroomLayout:
    """Return a deterministic fictional rectangular benchmark layout."""

    if rows <= 0 or cols <= 0:
        raise ValueError("benchmark layout rows and cols must be positive")
    seats: list[SeatNode] = []
    for row in range(1, rows + 1):
        zone = "front" if row == 1 else "back" if row == rows else "middle"
        for col in range(1, cols + 1):
            seats.append(
                SeatNode(
                    seat_id=f"R{row}C{col}",
                    row=row,
                    col=col,
                    x=float(col),
                    y=float(row),
                    zone=zone,
                    near_window=col == 1,
                    near_door=col == cols,
                    near_platform=row == 1,
                    near_ac=row == rows and col in {cols - 1, cols},
                )
            )
    return ClassroomLayout(layout_id=f"benchmark-{rows}x{cols}", seats=seats)


def benchmark_rules(
    profile: str,
    students: list[Student],
    layout: ClassroomLayout,
    base_rules: RuleSet | None = None,
) -> RuleSet:
    """Return deterministic rules for a light or dense synthetic workload.

    ``light`` preserves the caller's selected preset. ``dense`` overlays a
    feasible, deterministic set of hard rules while retaining the same soft
    rules, seed, and groups. The overlay is derived only from fictional
    benchmark identifiers.
    """

    normalized = normalize_benchmark_profile(profile)
    rules = (base_rules or RuleSet()).model_copy(deep=True)
    if normalized == "light":
        return rules
    if not students or not layout.enabled_seats:
        raise ValueError("dense benchmark profile requires students and enabled seats")

    student_keys = [student.key for student in students]
    seat_ids = [seat.seat_id for seat in layout.enabled_seats]
    fixed_count = min(len(student_keys), max(1, len(student_keys) // 20))
    fixed_indexes = _spread_indexes(len(student_keys), fixed_count)
    fixed_index_set = set(fixed_indexes)
    fixed_seats = [
        FixedSeatRule(student=student_keys[index], seat_id=seat_ids[index])
        for index in fixed_indexes
    ]

    remaining = [
        student_key
        for index, student_key in enumerate(student_keys)
        if index not in fixed_index_set
    ]
    cannot_pair_limit = min(len(remaining) // 2, max(1, len(student_keys) // 4))
    cannot_be_adjacent = [
        PairRule(students=(remaining[index], remaining[-(index + 1)]))
        for index in range(cannot_pair_limit)
    ]

    min_distance_limit = min(len(remaining) // 2, max(1, len(student_keys) // 10))
    min_distance = [
        MinDistanceRule(
            students=(remaining[index], remaining[-(index + 1)]),
            distance=2.0,
            metric="graph",
        )
        for index in range(min_distance_limit)
        if remaining[index] != remaining[-(index + 1)]
    ]

    rules.hard = HardRules(
        fixed_seats=fixed_seats,
        cannot_be_adjacent=cannot_be_adjacent,
        min_distance=min_distance,
    )
    return rules


def benchmark_profile_metadata(profile: str) -> dict[str, str]:
    """Return stable public metadata for one synthetic workload profile."""

    normalized = normalize_benchmark_profile(profile)
    return {
        "name": normalized,
        "description": BENCHMARK_PROFILE_DESCRIPTIONS[normalized],
    }


def _spread_indexes(length: int, count: int) -> list[int]:
    if count <= 0:
        return []
    if count == 1:
        return [0]
    return [round(index * (length - 1) / (count - 1)) for index in range(count)]
