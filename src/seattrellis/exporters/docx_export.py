"""Word (.docx) export via python-docx.

Depends on the ``docx`` optional extra (python-docx).
Designed for teachers who want to edit seating plans further in Word.
"""

from __future__ import annotations

from pathlib import Path

from seattrellis.exporters.print_html import (
    PrintPrivacyOptions,
    _default_privacy,
    _student_detail_fields,
    _validate_template,
)
from seattrellis.models.candidate import CandidatePlan
from seattrellis.models.snapshot import SeatingSnapshot


def export_docx(
    snapshot: SeatingSnapshot,
    output: str | Path,
    *,
    template: str = "public",
    privacy: PrintPrivacyOptions | None = None,
    candidate: CandidatePlan | None = None,
) -> Path:
    """Export a seating snapshot as a .docx file.

    Parameters
    ----------
    snapshot:
        The seating snapshot.
    output:
        Output ``.docx`` path.
    template:
        One of ``"public"``, ``"teacher"``, ``"report"``.
    privacy:
        Privacy options.
    candidate:
        Candidate plan for the ``"report"`` template.
    """
    template = _validate_template(template)
    if template == "report" and candidate is None:
        raise ValueError("The report template requires a candidate plan.")

    try:
        from docx import Document  # type: ignore[import-untyped]
        from docx.shared import Inches, Pt  # type: ignore[import-untyped]
        from docx.enum.text import WD_ALIGN_PARAGRAPH  # type: ignore[import-untyped]
    except ImportError as exc:  # pragma: no cover
        from seattrellis.optional import MissingOptionalDependencyError

        raise MissingOptionalDependencyError("Word export", "docx") from exc

    if privacy is None:
        privacy = _default_privacy(template)

    doc = Document()

    # Title
    title = doc.add_heading(snapshot.layout.name, level=1)
    title.alignment = WD_ALIGN_PARAGRAPH.CENTER

    # Meta
    meta = doc.add_paragraph()
    meta.alignment = WD_ALIGN_PARAGRAPH.CENTER
    meta_run = meta.add_run(f"生成时间: {snapshot.created_at}")
    meta_run.font.size = Pt(9)
    meta_run.font.color.rgb = None  # default

    # Candidate info if available
    if candidate is not None:
        cand_para = doc.add_paragraph()
        cand_para.alignment = WD_ALIGN_PARAGRAPH.CENTER
        cand_run = cand_para.add_run(
            f"方案: {candidate.candidate_id}  |  总分: {candidate.total_score:.1f}"
        )
        cand_run.font.size = Pt(10)
        cand_run.bold = True

    doc.add_paragraph()  # spacer

    # Seat table
    min_row, max_row, min_col, max_col = _bounds(snapshot)
    seat_by_pos = {(s.row, s.col): s for s in snapshot.layout.seats}
    assign_by_seat = {a.seat_id: a for a in snapshot.assignments}

    table = doc.add_table(rows=max_row - min_row + 1, cols=max_col - min_col + 1)
    table.style = "Table Grid"

    display_names = {
        assignment.student_key: (
            f"学生 {index:02d}"
            if privacy.anonymize
            else assignment.student_name or assignment.student_key
        )
        for index, assignment in enumerate(snapshot.assignments, start=1)
    }

    for r in range(min_row, max_row + 1):
        for c in range(min_col, max_col + 1):
            cell = table.cell(r - min_row, c - min_col)
            seat = seat_by_pos.get((r, c))
            if seat is None:
                cell.text = ""
                continue
            a = assign_by_seat.get(seat.seat_id)
            name = display_names.get(a.student_key, "") if a else ""
            cell.text = name if (name and seat.enabled) else seat.seat_id
            for para in cell.paragraphs:
                para.alignment = WD_ALIGN_PARAGRAPH.CENTER

    # Teacher section
    if template == "teacher":
        student_by_key = {s.key: s for s in snapshot.students}
        detail_headers = [
            header for header, _value in _student_detail_fields(None, privacy)
        ]
        doc.add_heading("学生明细", level=2)
        detail_table = doc.add_table(
            rows=len(snapshot.assignments) + 1,
            cols=2 + len(detail_headers),
        )
        detail_table.style = "Table Grid"
        headers = ["座位", "姓名", *detail_headers]
        for i, h in enumerate(headers):
            detail_table.cell(0, i).text = h
        for i, a in enumerate(snapshot.assignments):
            stu = student_by_key.get(a.student_key)
            detail_table.cell(i + 1, 0).text = a.seat_id
            detail_table.cell(i + 1, 1).text = display_names[a.student_key]
            for column, (_header, value) in enumerate(
                _student_detail_fields(stu, privacy),
                start=2,
            ):
                detail_table.cell(i + 1, column).text = value

    # Report section
    if template == "report" and candidate is not None:
        doc.add_heading("评分明细", level=2)
        b = candidate.score.breakdown
        for dim_name, dim_score in [
            ("公平轮换", b.fair_rotation_score),
            ("关系回避", b.avoid_recent_neighbors_score),
            ("成绩均衡", b.score_balance_score),
            ("身高偏好", b.height_preference_score),
            ("视力偏好", b.vision_preference_score),
            ("多样性", b.diversity_score),
            ("稳定性", b.stability_score),
        ]:
            score_str = f"{dim_score.score:.1f}" if dim_score.score is not None else "n/a"
            doc.add_paragraph(
                f"{dim_name}: {score_str} (weight={dim_score.weight})",
                style="List Bullet",
            )

    path = Path(output)
    path.parent.mkdir(parents=True, exist_ok=True)
    doc.save(str(path))
    return path


def _bounds(snapshot: SeatingSnapshot) -> tuple[int, int, int, int]:
    rows = [s.row for s in snapshot.layout.seats]
    cols = [s.col for s in snapshot.layout.seats]
    return min(rows), max(rows), min(cols), max(cols)
