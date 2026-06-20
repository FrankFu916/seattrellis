from __future__ import annotations

from seattrellis.exporters import export_snapshot
from seattrellis.io.json_files import load_layout, load_rules
from seattrellis.io.students import read_students
from seattrellis.solver import solve_seating


def test_export_excel_png_and_html(tmp_path) -> None:
    students = read_students("tests/fixtures/students.csv")
    layout = load_layout("tests/fixtures/classroom.json")
    rules = load_rules("tests/fixtures/rules.json")
    solution = solve_seating(students, layout, rules, seed=rules.seed)
    snapshot = solution.to_snapshot(students=students, layout=layout, rules=rules, seed=rules.seed)

    for output_format, filename in [("excel", "seating.xlsx"), ("png", "seating.png"), ("html", "seating.html")]:
        output = export_snapshot(snapshot, output_format, tmp_path / filename)
        assert output.exists()
        assert output.stat().st_size > 0
