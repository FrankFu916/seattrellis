from __future__ import annotations

from seattrellis.benchmarks import (
    BENCHMARK_DATASET_NAME,
    BENCHMARK_DATASET_VERSION,
    BENCHMARK_DEFAULT_SIZES,
    benchmark_case_id,
    benchmark_layout,
    benchmark_layout_shape,
    benchmark_students,
)


def test_default_benchmark_shapes_are_stable() -> None:
    assert BENCHMARK_DATASET_NAME == "synthetic-classroom"
    assert BENCHMARK_DATASET_VERSION == "synthetic-v1"
    assert BENCHMARK_DEFAULT_SIZES == (40, 50, 60)
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
