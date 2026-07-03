from __future__ import annotations

import os
from zipfile import ZipFile

import pytest

from seattrellis.exporters import (
    export_docx,
    export_pdf,
    export_print_html,
    export_snapshot,
)
from seattrellis.exporters.print_html import PrintPrivacyOptions
from seattrellis.exporters.pdf import _configure_macos_library_path
from seattrellis.io.json_files import load_layout, load_rules
from seattrellis.io.students import read_students
from seattrellis.models.candidate import CandidatePlan
from seattrellis.models.layout import ClassroomLayout, SeatNode
from seattrellis.models.rules import RuleSet
from seattrellis.models.snapshot import SeatAssignment, SeatingSnapshot
from seattrellis.models.student import Student
from seattrellis.optional import MissingOptionalDependencyError
from seattrellis.scoring import score_snapshot
from seattrellis.solver import solve_seating


def _fixture_snapshot() -> SeatingSnapshot:
    students = read_students("tests/fixtures/students.csv")
    layout = load_layout("tests/fixtures/classroom.json")
    rules = load_rules("tests/fixtures/rules.json")
    solution = solve_seating(students, layout, rules, seed=rules.seed)
    return solution.to_snapshot(
        students=students,
        layout=layout,
        rules=rules,
        seed=rules.seed,
    )


def _sensitive_snapshot() -> SeatingSnapshot:
    student = Student(
        student_id="S1",
        name='<script>alert("student")</script>',
        score=88,
        height_cm=172,
        vision="SECRET_VISION",
        notes="SECRET_NOTE<&",
        needs=["SECRET_NEED"],
        tags=["SECRET_TAG"],
    )
    layout = ClassroomLayout(
        name="<Unsafe Classroom>",
        seats=[SeatNode(seat_id='S<1>"', row=1, col=1)],
    )
    return SeatingSnapshot(
        students=[student],
        layout=layout,
        rules=RuleSet(),
        assignments=[
            SeatAssignment(
                student_key=student.key,
                student_name=student.display_name,
                seat_id='S<1>"',
            )
        ],
        solver_status="FEASIBLE",
    )


def _candidate(snapshot: SeatingSnapshot) -> CandidatePlan:
    return CandidatePlan(
        candidate_id="candidate_<01>",
        snapshot=snapshot,
        score=score_snapshot(snapshot),
        hard_constraints_satisfied=True,
    )


def _docx_xml(path) -> str:
    with ZipFile(path) as archive:
        return archive.read("word/document.xml").decode("utf-8")


def test_export_excel_png_and_html(tmp_path) -> None:
    snapshot = _fixture_snapshot()

    for output_format, filename in [
        ("excel", "seating.xlsx"),
        ("png", "seating.png"),
        ("html", "seating.html"),
    ]:
        output = export_snapshot(snapshot, output_format, tmp_path / filename)
        assert output.exists()
        assert output.stat().st_size > 0


def test_print_html_templates_render_expected_sections_and_escape_html(
    tmp_path,
) -> None:
    snapshot = _sensitive_snapshot()
    candidate = _candidate(snapshot)

    public_html = export_print_html(
        snapshot,
        tmp_path / "public.html",
        template="public",
    ).read_text(encoding="utf-8")
    teacher_html = export_print_html(
        snapshot,
        tmp_path / "teacher.html",
        template="teacher",
    ).read_text(encoding="utf-8")
    report_html = export_print_html(
        snapshot,
        tmp_path / "report.html",
        template="report",
        candidate=candidate,
    ).read_text(encoding="utf-8")

    assert "&lt;script&gt;alert(&quot;student&quot;)&lt;/script&gt;" in public_html
    assert "<script>" not in public_html
    assert "SECRET_NOTE" not in public_html
    assert "SECRET_NEED" not in public_html

    assert "教师信息" in teacher_html
    assert "SECRET_NOTE&lt;&amp;" in teacher_html
    assert "SECRET_VISION" in teacher_html
    assert "SECRET_NEED、SECRET_TAG" in teacher_html
    assert ">88.0<" in teacher_html
    assert ">172.0<" in teacher_html

    assert "方案解释报告" in report_html
    assert "candidate_&lt;01&gt;" in report_html
    assert "Hard Constraints" in report_html


def test_teacher_html_privacy_options_hide_every_sensitive_field(
    tmp_path,
) -> None:
    snapshot = _sensitive_snapshot()
    privacy = PrintPrivacyOptions(
        hide_scores=True,
        hide_notes=True,
        hide_special_needs=True,
        anonymize=True,
        show_height=False,
        show_vision=False,
    )

    html = export_print_html(
        snapshot,
        tmp_path / "private.html",
        template="teacher",
        privacy=privacy,
    ).read_text(encoding="utf-8")

    for secret in [
        "<script>",
        "&lt;script&gt;",
        "SECRET_NOTE",
        "SECRET_NEED",
        "SECRET_TAG",
        "SECRET_VISION",
        "88.0",
        "172.0",
    ]:
        assert secret not in html
    assert "学生 01" in html


def test_docx_respects_teacher_defaults_and_privacy_options(tmp_path) -> None:
    pytest.importorskip("docx")
    snapshot = _sensitive_snapshot()

    teacher_path = export_docx(
        snapshot,
        tmp_path / "teacher.docx",
        template="teacher",
    )
    teacher_xml = _docx_xml(teacher_path)
    assert "&lt;script&gt;alert(\"student\")&lt;/script&gt;" in teacher_xml
    assert "SECRET_NOTE&lt;&amp;" in teacher_xml
    assert "SECRET_NEED、SECRET_TAG" in teacher_xml
    assert "SECRET_VISION" in teacher_xml
    assert ">88.0<" in teacher_xml
    assert ">172.0<" in teacher_xml

    private_path = export_docx(
        snapshot,
        tmp_path / "private.docx",
        template="teacher",
        privacy=PrintPrivacyOptions(
            hide_scores=True,
            hide_notes=True,
            hide_special_needs=True,
            anonymize=True,
            show_height=False,
            show_vision=False,
        ),
    )
    private_xml = _docx_xml(private_path)
    assert "学生 01" in private_xml
    for secret in [
        "script",
        "SECRET_NOTE",
        "SECRET_NEED",
        "SECRET_TAG",
        "SECRET_VISION",
        "88.0",
        "172.0",
    ]:
        assert secret not in private_xml


def test_pdf_has_valid_header_and_nonempty_content(tmp_path) -> None:
    snapshot = _sensitive_snapshot()
    try:
        output = export_pdf(snapshot, tmp_path / "public.pdf", template="public")
    except MissingOptionalDependencyError as exc:
        pytest.skip(str(exc))

    data = output.read_bytes()
    assert data.startswith(b"%PDF-")
    assert len(data) > 1_000


def test_pdf_configures_homebrew_library_path_on_macos(
    monkeypatch,
    tmp_path,
) -> None:
    library_dir = tmp_path / "lib"
    library_dir.mkdir()
    (library_dir / "libpango-1.0.dylib").touch()
    monkeypatch.setattr("seattrellis.exporters.pdf.sys.platform", "darwin")
    monkeypatch.setenv("DYLD_FALLBACK_LIBRARY_PATH", "/existing/lib")

    assert _configure_macos_library_path((library_dir,)) is True
    assert os.environ["DYLD_FALLBACK_LIBRARY_PATH"] == (
        f"{library_dir}:/existing/lib"
    )

    assert _configure_macos_library_path((library_dir,)) is True
    assert (
        os.environ["DYLD_FALLBACK_LIBRARY_PATH"].count(str(library_dir))
        == 1
    )


@pytest.mark.parametrize("exporter,extension", [
    (export_print_html, "html"),
    (export_docx, "docx"),
    (export_pdf, "pdf"),
])
def test_print_exporters_reject_unknown_templates(
    exporter,
    extension,
    tmp_path,
) -> None:
    with pytest.raises(ValueError, match="Unsupported print template"):
        exporter(
            _sensitive_snapshot(),
            tmp_path / f"invalid.{extension}",
            template="unknown",
        )


@pytest.mark.parametrize("exporter,extension", [
    (export_print_html, "html"),
    (export_docx, "docx"),
    (export_pdf, "pdf"),
])
def test_report_template_requires_candidate(
    exporter,
    extension,
    tmp_path,
) -> None:
    with pytest.raises(ValueError, match="requires a candidate plan"):
        exporter(
            _sensitive_snapshot(),
            tmp_path / f"report.{extension}",
            template="report",
        )
