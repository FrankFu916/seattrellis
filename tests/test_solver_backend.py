from __future__ import annotations

import builtins
import json
import subprocess
import sys

import pytest

from seattrellis.models import ClassroomLayout, RuleSet, SeatNode, Student
from seattrellis.optional import MissingOptionalDependencyError
from seattrellis.service import run_doctor
from seattrellis.solver import resolve_solver_backend, solve_seating
from seattrellis.solver import native_backend, ortools_backend


def test_backend_resolution_prefers_explicit_request(monkeypatch) -> None:
    monkeypatch.setenv("SEATTRELLIS_BACKEND", "ortools")
    monkeypatch.setenv("SEATTRELLIS_USE_ORTOOLS", "1")

    assert resolve_solver_backend("fallback") == "fallback"
    assert resolve_solver_backend("ortools") == "ortools"


def test_backend_resolution_keeps_legacy_ortools_env(monkeypatch) -> None:
    monkeypatch.delenv("SEATTRELLIS_BACKEND", raising=False)
    monkeypatch.setenv("SEATTRELLIS_USE_ORTOOLS", "1")

    assert resolve_solver_backend("auto") == "ortools"


def test_explicit_fallback_ignores_ortools_env(monkeypatch) -> None:
    monkeypatch.setenv("SEATTRELLIS_USE_ORTOOLS", "1")
    students = [Student(student_id="S1")]
    layout = ClassroomLayout(seats=[SeatNode(seat_id="A1", row=1, col=1)])

    solution = solve_seating(students, layout, RuleSet(), backend="fallback")

    assert solution.metrics["solver"] == "fallback-heuristic"
    assert solution.metrics["solver_backend_requested"] == "fallback"
    assert solution.metrics["solver_backend_effective"] == "fallback"


def test_explicit_ortools_reports_missing_extra_without_env(monkeypatch) -> None:
    _block_import(monkeypatch, "ortools")
    monkeypatch.delenv("SEATTRELLIS_USE_ORTOOLS", raising=False)
    monkeypatch.setattr(ortools_backend, "cp_model", None)
    monkeypatch.setattr(ortools_backend, "_cp_model_unavailable", False)
    students = [Student(student_id="S1")]
    layout = ClassroomLayout(seats=[SeatNode(seat_id="A1", row=1, col=1)])

    with pytest.raises(MissingOptionalDependencyError, match="OR-Tools solver"):
        solve_seating(students, layout, RuleSet(), backend="ortools")


def test_explicit_native_reports_missing_extension(monkeypatch) -> None:
    monkeypatch.setattr(native_backend, "require_native_core", lambda: (_ for _ in ()).throw(
        MissingOptionalDependencyError("Rust native backend", "native")
    ))
    students = [Student(student_id="S1")]
    layout = ClassroomLayout(seats=[SeatNode(seat_id="A1", row=1, col=1)])

    with pytest.raises(MissingOptionalDependencyError, match="Rust native backend"):
        solve_seating(students, layout, RuleSet(), backend="native")


def test_ortools_unknown_status_is_not_reported_as_infeasible(monkeypatch) -> None:
    class FakeCpModel:
        OPTIMAL = 4
        FEASIBLE = 2
        INFEASIBLE = 3
        MODEL_INVALID = 1
        UNKNOWN = 0

    monkeypatch.setattr(ortools_backend, "cp_model", FakeCpModel)
    students = [Student(student_id="S1")]
    layout = ClassroomLayout(seats=[SeatNode(seat_id="A1", row=1, col=1)])

    message = ortools_backend.format_ortools_failure(
        status=FakeCpModel.UNKNOWN,
        students=students,
        layout=layout,
        rules=RuleSet(),
        time_limit_seconds=1.0,
    )

    assert "did not find a feasible seating plan within 1 seconds" in message
    assert "not proof that the problem is infeasible" in message
    assert "No feasible seating plan was found" not in message


def test_doctor_reports_solver_backend(monkeypatch) -> None:
    monkeypatch.delenv("SEATTRELLIS_BACKEND", raising=False)
    monkeypatch.delenv("SEATTRELLIS_USE_ORTOOLS", raising=False)

    output = run_doctor()

    assert "Solver backend:" in output
    assert "Effective default: fallback" in output
    assert "Supported: auto, fallback, ortools, native" in output
    assert "SEATTRELLIS_BACKEND: (not set)" in output
    assert "Native core:" in output


def test_benchmark_script_smoke(tmp_path) -> None:
    output = tmp_path / "benchmark.json"
    markdown_output = tmp_path / "benchmark.md"
    result = subprocess.run(
        [
            sys.executable,
            "scripts/benchmark_solver.py",
            "--sizes",
            "4",
            "--backends",
            "fallback",
            "--candidates",
            "1",
            "--time-limit",
            "1",
            "--output",
            str(output),
            "--markdown-output",
            str(markdown_output),
        ],
        check=False,
        text=True,
        capture_output=True,
    )

    assert result.returncode == 0, result.stderr
    payload = json.loads(output.read_text(encoding="utf-8"))
    assert payload["dataset"] == {
        "name": "synthetic-classroom",
        "version": "synthetic-v1",
        "default_sizes": [40, 50, 60],
        "fictional": True,
    }
    assert payload["results"][0]["ok"] is True
    assert payload["results"][0]["backend"] == "fallback"
    assert payload["benchmark_version"] == 1
    assert payload["environment"]["seattrellis_version"]
    assert payload["summary"]["total_cases"] == 1
    assert payload["summary"]["successful_cases"] == 1
    assert payload["summary"]["failed_cases"] == 0
    assert payload["summary"]["by_backend"][0]["backend"] == "fallback"
    assert payload["summary"]["by_size"][0]["fastest_backend"] == "fallback"
    assert payload["results"][0]["case_id"] == "synthetic-v1-4-students-5x8"
    assert payload["results"][0]["dataset_version"] == "synthetic-v1"
    assert payload["results"][0]["solver_backend_effective"] == "fallback"
    markdown = markdown_output.read_text(encoding="utf-8")
    assert "# SeatTrellis benchmark report" in markdown
    assert "| Backend | Success | Avg elapsed | Max elapsed | Effective backend |" in markdown
    assert "synthetic-v1-4-students-5x8" in markdown


def test_benchmark_script_rejects_unknown_backend() -> None:
    result = subprocess.run(
        [
            sys.executable,
            "scripts/benchmark_solver.py",
            "--sizes",
            "4",
            "--backends",
            "made-up",
        ],
        check=False,
        text=True,
        capture_output=True,
    )

    assert result.returncode != 0
    assert "Unsupported solver backend" in result.stderr


def _block_import(monkeypatch, blocked_root: str) -> None:
    original_import = builtins.__import__

    def guarded_import(name, globals=None, locals=None, fromlist=(), level=0):
        if name == blocked_root or name.startswith(f"{blocked_root}."):
            raise ImportError(f"No module named {blocked_root}")
        return original_import(name, globals, locals, fromlist, level)

    monkeypatch.setattr(builtins, "__import__", guarded_import)
