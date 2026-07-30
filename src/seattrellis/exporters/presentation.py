"""Shared student presentation rules for configurable exports.

This module is deliberately independent of any output format.  Exporters can
therefore share the same anonymisation and field-visibility decisions without
reimplementing privacy behaviour in HTML, Word, SVG, or PowerPoint code.
"""

from __future__ import annotations

from typing import TYPE_CHECKING

from seattrellis.service_types import PrivacyOptions, normalize_export_locale

if TYPE_CHECKING:
    from seattrellis.models.snapshot import SeatingSnapshot
    from seattrellis.models.student import Student


_ANONYMOUS_STUDENT = {
    "zh": "学生 {index:02d}",
    "en": "Student {index:02d}",
}

_FIELD_LABELS: dict[str, dict[str, str]] = {
    "student_id": {"zh": "学号", "en": "Student ID"},
    "score": {"zh": "成绩", "en": "Score"},
    "height": {"zh": "身高", "en": "Height"},
    "vision": {"zh": "视力需求", "en": "Vision"},
    "special_needs": {"zh": "特殊需求", "en": "Special needs"},
    "notes": {"zh": "备注", "en": "Notes"},
}


def student_display_names(
    snapshot: "SeatingSnapshot",
    privacy: PrivacyOptions,
    locale: str = "zh",
) -> dict[str, str]:
    """Return the display name for every assigned student.

    Assignment order is used for anonymous labels so the result stays stable
    across all exporters for the same snapshot.
    """

    locale = normalize_export_locale(locale)
    anonymous_template = _ANONYMOUS_STUDENT[locale]
    return {
        assignment.student_key: (
            anonymous_template.format(index=index)
            if privacy.anonymize
            else assignment.student_name or assignment.student_key
        )
        for index, assignment in enumerate(snapshot.assignments, start=1)
    }


def student_detail_fields(
    student: "Student | None",
    privacy: PrivacyOptions,
    locale: str = "zh",
) -> list[tuple[str, str]]:
    """Return teacher-visible student fields allowed by ``privacy``.

    The returned labels are localized while values stay faithful to the source
    data.  Exporters should call this helper instead of testing privacy flags
    themselves.
    """

    locale = normalize_export_locale(locale)
    labels = {key: values[locale] for key, values in _FIELD_LABELS.items()}
    fields: list[tuple[str, str]] = []
    if not privacy.anonymize:
        fields.append(
            (
                labels["student_id"],
                student.student_id if student and student.student_id else "-",
            )
        )
    if not privacy.hide_scores:
        fields.append(
            (
                labels["score"],
                str(student.score) if student and student.score is not None else "-",
            )
        )
    if privacy.show_height:
        fields.append(
            (
                labels["height"],
                str(student.height_cm)
                if student and student.height_cm is not None
                else "-",
            )
        )
    if privacy.show_vision:
        fields.append(
            (
                labels["vision"],
                str(student.vision) if student and student.vision is not None else "-",
            )
        )
    if not privacy.hide_special_needs:
        needs = list(student.needs) + list(student.tags) if student else []
        separator = ", " if locale == "en" else "、"
        fields.append(
            (
                labels["special_needs"],
                separator.join(needs) if needs else "-",
            )
        )
    if not privacy.hide_notes:
        fields.append(
            (
                labels["notes"],
                student.notes if student and student.notes else "-",
            )
        )
    return fields


def xml_safe_text(value: object) -> str:
    """Replace characters that XML 1.0 cannot represent.

    Escaping remains the responsibility of the serializer.  Keeping character
    validation here lets SVG and Office Open XML exporters use the same input
    boundary without importing either rendering dependency.
    """

    return "".join(
        character if _is_xml_character(ord(character)) else "\N{REPLACEMENT CHARACTER}"
        for character in str(value)
    )


def _is_xml_character(codepoint: int) -> bool:
    return (
        codepoint in {0x9, 0xA, 0xD}
        or 0x20 <= codepoint <= 0xD7FF
        or 0xE000 <= codepoint <= 0xFFFD
        or 0x10000 <= codepoint <= 0x10FFFF
    )
