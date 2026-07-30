from __future__ import annotations

import os
from datetime import datetime, timezone
from pathlib import Path
from zipfile import ZipFile

import pytest

from seattrellis.exporters import (
    export_docx,
    export_pdf,
    export_print_html,
    export_snapshot,
)
from seattrellis.exporters.candidate_report import render_candidate_report_html
from seattrellis.exporters.print_html import PrintPrivacyOptions
from seattrellis.exporters.pdf import _configure_macos_library_path
from seattrellis.io.json_files import load_layout, load_rules
from seattrellis.io.json_files import write_json_model
from seattrellis.io.students import read_students
from seattrellis.models.candidate import (
    CandidatePlan,
    CandidateSet,
    HardConstraintSummary,
    ScoreDimension,
)
from seattrellis.models.layout import ClassroomLayout, SeatNode
from seattrellis.models.rules import RuleSet
from seattrellis.models.snapshot import SeatAssignment, SeatingSnapshot
from seattrellis.models.student import Student
from seattrellis.optional import MissingOptionalDependencyError
from seattrellis.scoring import build_plan_comparison_report, score_snapshot
from seattrellis.service import export as service_export
from seattrellis.service_types import ExportRequest, PageOptions, PrivacyOptions
from seattrellis.solver import solve_seating


FIXED_CREATED_AT = datetime(2026, 1, 2, 3, 4, 5, tzinfo=timezone.utc)


def _fixture_snapshot() -> SeatingSnapshot:
    students = read_students("tests/fixtures/students.csv")
    layout = load_layout("tests/fixtures/classroom.json")
    rules = load_rules("tests/fixtures/rules.json")
    solution = solve_seating(students, layout, rules, seed=rules.seed)
    snapshot = solution.to_snapshot(
        students=students,
        layout=layout,
        rules=rules,
        seed=rules.seed,
    )
    snapshot.created_at = FIXED_CREATED_AT
    return snapshot


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
        created_at=FIXED_CREATED_AT,
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
    english_teacher_html = export_print_html(
        snapshot,
        tmp_path / "teacher.en.html",
        template="teacher",
        locale="en",
    ).read_text(encoding="utf-8")
    english_report_html = export_print_html(
        snapshot,
        tmp_path / "report.en.html",
        template="report",
        candidate=candidate,
        locale="en",
    ).read_text(encoding="utf-8")

    assert "&lt;script&gt;alert(&quot;student&quot;)&lt;/script&gt;" in public_html
    assert "<script>" not in public_html
    assert "SECRET_NOTE" not in public_html
    assert "SECRET_NEED" not in public_html

    assert "教师信息" in teacher_html
    assert ">S1<" in teacher_html
    assert "SECRET_NOTE&lt;&amp;" in teacher_html
    assert "SECRET_VISION" in teacher_html
    assert "SECRET_NEED、SECRET_TAG" in teacher_html
    assert ">88.0<" in teacher_html
    assert ">172.0<" in teacher_html

    assert "方案解释报告" in report_html
    assert "candidate_&lt;01&gt;" in report_html
    assert "硬约束" in report_html

    assert "Teacher information" in english_teacher_html
    assert "Student details" in english_teacher_html
    assert ">Student ID<" in english_teacher_html
    assert ">Score<" in english_teacher_html
    assert ">Vision<" in english_teacher_html
    assert "教师信息" not in english_teacher_html
    assert "Plan explanation" in english_report_html
    assert "Fair rotation" in english_report_html
    assert "Recommendation" in english_report_html


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
    assert ">S1<" not in html


def test_anonymized_html_omits_identity_bearing_free_form_details(
    tmp_path,
) -> None:
    snapshot = _sensitive_snapshot()
    snapshot.metadata["rules_summary"] = "Keep SECRET_STUDENT near the front"
    snapshot.metadata["warnings"] = ["SECRET_STUDENT has no stable ID"]
    candidate = _candidate(snapshot)
    hard = candidate.score.breakdown.hard_constraint_summary
    hard.satisfied = False
    hard.violation_count = 1
    hard.violations = ["fixed_seats is not satisfied for SECRET_STUDENT"]
    privacy = PrintPrivacyOptions(anonymize=True)

    teacher_html = export_print_html(
        snapshot,
        tmp_path / "anonymous-teacher.html",
        template="teacher",
        privacy=privacy,
    ).read_text(encoding="utf-8")
    report_html = export_print_html(
        snapshot,
        tmp_path / "anonymous-report.html",
        template="report",
        privacy=privacy,
        candidate=candidate,
    ).read_text(encoding="utf-8")

    assert "Student 01" not in teacher_html
    assert "学生 01" in teacher_html
    assert "SECRET_STUDENT" not in teacher_html
    assert "SECRET_STUDENT" not in report_html
    assert "1 违规" in report_html


def test_export_request_public_defaults_are_safe() -> None:
    request = ExportRequest(output_format="print-html")

    assert request.template == "public"
    assert request.candidate_scope == "selected"
    assert request.locale == "zh"
    assert request.page == PageOptions()
    assert request.resolved_privacy == PrivacyOptions(
        hide_scores=True,
        hide_notes=True,
        hide_special_needs=True,
        anonymize=False,
        show_height=False,
        show_vision=False,
    )


def test_export_request_applies_page_options_to_print_html(tmp_path) -> None:
    request = ExportRequest(
        output_format="print-html",
        output_path=tmp_path / "landscape.html",
        page=PageOptions(orientation="landscape", scale=0.8, margin_mm=10),
        locale="en",
    )

    html = export_snapshot(
        _sensitive_snapshot(),
        request=request,
    ).read_text(encoding="utf-8")

    assert '<html lang="en">' in html
    assert "@page { size: A4 landscape; margin: 10mm; }" in html
    assert "font-size: 10.4px" in html
    assert "width: 80px" in html
    for secret in [
        "SECRET_NOTE",
        "SECRET_NEED",
        "SECRET_TAG",
        "SECRET_VISION",
        "88.0",
        "172.0",
    ]:
        assert secret not in html


@pytest.mark.parametrize(
    ("factory", "message"),
    [
        (lambda: PageOptions(orientation="diagonal"), "orientation"),
        (lambda: PageOptions(scale=0.1), "scale"),
        (lambda: PageOptions(paper_size="Letter"), "A4"),
        (
            lambda: ExportRequest(output_format="print-html", template="unknown"),
            "template",
        ),
        (
            lambda: ExportRequest(output_format="print-html", locale="fr"),
            "locale",
        ),
        (
            lambda: ExportRequest(
                output_format="print-html", candidate_scope="unknown"
            ),
            "candidate scope",
        ),
    ],
)
def test_export_request_rejects_invalid_options(factory, message) -> None:
    with pytest.raises(ValueError, match=message):
        factory()


def test_non_print_export_rejects_silently_ignored_options(tmp_path) -> None:
    request = ExportRequest(
        output_format="html",
        output_path=tmp_path / "seating.html",
        privacy=PrivacyOptions(anonymize=True),
    )

    with pytest.raises(ValueError, match="does not yet support"):
        export_snapshot(_sensitive_snapshot(), request=request)


def test_service_export_passes_candidate_to_report_template(tmp_path) -> None:
    snapshot = _sensitive_snapshot()
    candidate = _candidate(snapshot)
    artifact_path = write_json_model(
        CandidateSet(
            candidates=[candidate],
            recommended_candidate_id=candidate.candidate_id,
        ),
        tmp_path / "candidates.json",
    )
    request = ExportRequest(
        output_format="print-html",
        output_path=tmp_path / "report.html",
        template="report",
        candidate_id="recommended",
    )

    report_path = service_export(snapshot_path=artifact_path, request=request)
    report = report_path.read_text(encoding="utf-8")

    assert "方案解释报告" in report
    assert "candidate_&lt;01&gt;" in report


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


def test_docx_applies_landscape_page_options(tmp_path) -> None:
    pytest.importorskip("docx")
    output = export_docx(
        _sensitive_snapshot(),
        tmp_path / "landscape.docx",
        page=PageOptions(orientation="landscape", scale=0.8, margin_mm=10),
    )

    document_xml = _docx_xml(output)
    assert 'w:orient="landscape"' in document_xml
    assert "<w:pgMar" in document_xml


def test_docx_renders_english_teacher_labels(tmp_path) -> None:
    pytest.importorskip("docx")
    output = export_docx(
        _sensitive_snapshot(),
        tmp_path / "teacher.en.docx",
        template="teacher",
        locale="en",
    )

    document_xml = _docx_xml(output)
    assert "Student details" in document_xml
    assert ">Score<" in document_xml
    assert ">Vision<" in document_xml
    assert "学生明细" not in document_xml


def test_service_export_all_candidate_scope_writes_comparison_report(tmp_path) -> None:
    snapshot = _sensitive_snapshot()
    candidate = _candidate(snapshot)
    artifact_path = write_json_model(
        CandidateSet(
            candidates=[candidate],
            recommended_candidate_id=candidate.candidate_id,
            warnings=["SECRET_STUDENT appears in a source warning"],
        ),
        tmp_path / "candidates.json",
    )
    request = ExportRequest(
        output_format="html",
        output_path=tmp_path / "all.html",
        candidate_scope="all",
        locale="en",
        template="teacher",
        privacy=PrivacyOptions(
            hide_scores=False,
            hide_notes=False,
            hide_special_needs=False,
            anonymize=False,
            show_height=True,
            show_vision=True,
        ),
    )

    report_path = service_export(snapshot_path=artifact_path, request=request)
    report = report_path.read_text(encoding="utf-8")

    assert "Candidate comparison report" in report
    assert "candidate_&lt;01&gt;" in report
    assert "Recommended" in report
    assert "Score comparison" in report
    assert "1 warning is attached to this candidate set" in report
    assert "SECRET_STUDENT" not in report
    for sensitive_value in (
        "alert(&quot;student&quot;)",
        "SECRET_NOTE",
        "SECRET_NEED",
        "SECRET_TAG",
        "SECRET_VISION",
        "&lt;Unsafe Classroom&gt;",
    ):
        assert sensitive_value not in report


def test_candidate_report_localizes_structured_explanations_and_summaries() -> None:
    snapshot = _sensitive_snapshot()
    recommended = _candidate(snapshot)
    recommended.candidate_id = "candidate_01"
    recommended.score.total = 90.0
    recommended.score.breakdown.fair_rotation_score = ScoreDimension(
        status="available",
        score=88.0,
        raw_value=88.0,
        weight=5,
        rating="high",
    )
    recommended.score.breakdown.hard_constraint_summary = HardConstraintSummary(
        satisfied=True,
        checked_rule_count=7,
        violation_count=0,
    )

    alternate = _candidate(snapshot)
    alternate.candidate_id = "candidate_02"
    alternate.score.total = 81.5
    alternate.hard_constraints_satisfied = False
    alternate.score.breakdown.fair_rotation_score = ScoreDimension(
        status="available",
        score=70.0,
        raw_value=70.0,
        weight=5,
        rating="medium",
    )
    alternate.score.breakdown.score_balance_score = ScoreDimension(
        status="available",
        score=40.0,
        raw_value=40.0,
        weight=5,
        rating="low",
    )
    alternate.score.breakdown.hard_constraint_summary = HardConstraintSummary(
        satisfied=False,
        checked_rule_count=7,
        violation_count=2,
        violations=[
            "fixed_seats is not satisfied for SECRET_STUDENT",
            "SECRET_STUDENT is assigned twice",
        ],
    )
    candidate_set = CandidateSet(
        created_at=FIXED_CREATED_AT,
        candidates=[recommended, alternate],
        recommended_candidate_id=recommended.candidate_id,
        warnings=["SECRET_STUDENT appears in a source warning"],
    )
    report = build_plan_comparison_report(candidate_set)

    chinese = render_candidate_report_html(candidate_set, report, locale="zh")
    english = render_candidate_report_html(candidate_set, report, locale="en")

    assert "与推荐差值" in chinese
    assert "-8.5" in chinese
    assert "已检查 7 项，违反 2 项" in chinese
    assert "公平轮换：中等（70.0）" in chinese
    assert "成绩搭配：较低（40.0）" in chinese
    assert "无可比历史" in chinese
    assert "在满足全部硬约束的方案中选择加权总分最高者" in chinese
    assert "fair rotation: medium" not in chinese.lower()

    assert "Difference from recommended" in english
    assert "7 checked, 2 violations" in english
    assert "Fair rotation: medium (70.0)" in english
    assert "Score mixing: low (40.0)" in english
    assert "No comparable history" in english
    assert "Select the highest weighted total" in english

    assert "SECRET_STUDENT" not in chinese
    assert "SECRET_STUDENT" not in english

    legacy_report = report.copy(deep=True)
    for entry in legacy_report.candidates:
        entry.score_delta_from_recommended = None
        entry.hard_constraint_checked_count = None
        entry.hard_constraint_violation_count = None
    legacy_chinese = render_candidate_report_html(
        candidate_set,
        legacy_report,
        locale="zh",
    )
    assert "-8.5" in legacy_chinese
    assert "已检查" not in legacy_chinese


def test_service_export_all_candidate_scope_requires_candidate_set(tmp_path) -> None:
    snapshot_path = write_json_model(
        _sensitive_snapshot(),
        tmp_path / "snapshot.json",
    )
    request = ExportRequest(
        output_format="print-html",
        output_path=tmp_path / "all.html",
        candidate_scope="all",
    )

    with pytest.raises(ValueError, match="requires a candidate set"):
        service_export(snapshot_path=snapshot_path, request=request)


def test_service_export_all_candidate_scope_rejects_non_html_formats(tmp_path) -> None:
    snapshot = _sensitive_snapshot()
    artifact_path = write_json_model(
        CandidateSet(
            candidates=[_candidate(snapshot)],
            recommended_candidate_id="candidate_<01>",
        ),
        tmp_path / "candidates.json",
    )
    request = ExportRequest(
        output_format="docx",
        output_path=tmp_path / "all.docx",
        candidate_scope="all",
    )

    with pytest.raises(ValueError, match="supports only html and print-html"):
        service_export(snapshot_path=artifact_path, request=request)


def test_pdf_has_valid_header_and_nonempty_content(tmp_path) -> None:
    snapshot = _sensitive_snapshot()
    try:
        output = export_pdf(snapshot, tmp_path / "public.pdf", template="public")
    except MissingOptionalDependencyError as exc:
        pytest.skip(str(exc))

    data = output.read_bytes()
    assert data.startswith(b"%PDF-")
    assert len(data) > 1_000


def test_pdf_page_orientation_regression(tmp_path) -> None:
    pypdf = pytest.importorskip("pypdf")
    snapshot = _sensitive_snapshot()
    try:
        portrait = export_pdf(
            snapshot,
            tmp_path / "portrait.pdf",
            page=PageOptions(orientation="portrait"),
        )
        landscape = export_pdf(
            snapshot,
            tmp_path / "landscape.pdf",
            page=PageOptions(orientation="landscape"),
        )
    except MissingOptionalDependencyError as exc:
        pytest.skip(str(exc))

    portrait_box = pypdf.PdfReader(portrait).pages[0].mediabox
    landscape_box = pypdf.PdfReader(landscape).pages[0].mediabox
    assert float(portrait_box.width) < float(portrait_box.height)
    assert float(landscape_box.width) > float(landscape_box.height)


def test_pdf_configures_homebrew_library_path_on_macos(
    monkeypatch,
) -> None:
    library_dir = Path("mock-homebrew/lib")
    pango_library = library_dir / "libpango-1.0.dylib"
    monkeypatch.setattr("seattrellis.exporters.pdf.sys.platform", "darwin")
    monkeypatch.setattr(
        Path,
        "exists",
        lambda path: path == pango_library,
    )
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
