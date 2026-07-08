"""Synthetic benchmark datasets used for solver performance checks."""

from __future__ import annotations

from math import ceil

from seattrellis.models import ClassroomLayout, SeatNode, Student


BENCHMARK_DATASET_NAME = "synthetic-classroom"
BENCHMARK_DATASET_VERSION = "synthetic-v1"
BENCHMARK_DEFAULT_SIZES = (40, 50, 60)


def benchmark_case_id(size: int, rows: int, cols: int) -> str:
    """Stable identifier for one synthetic benchmark case."""

    return f"{BENCHMARK_DATASET_VERSION}-{size}-students-{rows}x{cols}"


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
