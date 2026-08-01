"""Deterministic roster column mapping for interactive imports.

Mappings use physical column indices so duplicate or translated headers never
collapse into one dictionary key. Automatic suggestions are deliberately
conservative: an ambiguous alias is left for the teacher to resolve instead of
silently choosing the first column.
"""

from __future__ import annotations

import hashlib
import json
import re
import unicodedata
from collections.abc import Iterable, Mapping
from dataclasses import dataclass
from typing import Any, Literal, cast

from seattrellis.io.roster_table import RosterTable
from seattrellis.io.students import COLUMN_ALIASES, students_from_records
from seattrellis.models.student import Student


ROSTER_MAPPING_TEMPLATE_KIND = "seattrellis_roster_mapping"
ROSTER_MAPPING_TEMPLATE_VERSION = 1

RosterField = Literal[
    "student_id",
    "name",
    "gender",
    "height_cm",
    "score",
    "vision",
    "tags",
    "needs",
    "notes",
]

ROSTER_FIELDS: tuple[RosterField, ...] = (
    "student_id",
    "name",
    "gender",
    "height_cm",
    "score",
    "vision",
    "tags",
    "needs",
    "notes",
)
_ROSTER_FIELD_SET = frozenset(ROSTER_FIELDS)


@dataclass(frozen=True)
class ColumnMapping:
    """Assign one canonical student field to one physical source column."""

    field: RosterField
    column_index: int

    def __post_init__(self) -> None:
        if self.field not in _ROSTER_FIELD_SET:
            raise ValueError(f"Unknown roster field: {self.field!r}")
        if (
            isinstance(self.column_index, bool)
            or not isinstance(self.column_index, int)
            or self.column_index < 0
        ):
            raise ValueError("column_index must be a non-negative integer")


@dataclass(frozen=True)
class RosterMapping:
    """Validated assignments for one table header structure."""

    assignments: tuple[ColumnMapping, ...]
    header_fingerprint: str

    def __post_init__(self) -> None:
        object.__setattr__(self, "assignments", _ordered_assignments(self.assignments))
        _validate_unique_assignments(self.assignments)
        _validate_header_fingerprint(self.header_fingerprint)

    @property
    def mapped_fields(self) -> tuple[RosterField, ...]:
        return tuple(assignment.field for assignment in self.assignments)

    def column_for(self, field: RosterField) -> int | None:
        for assignment in self.assignments:
            if assignment.field == field:
                return assignment.column_index
        return None

    def as_dict(self) -> dict[RosterField, int]:
        return {
            assignment.field: assignment.column_index
            for assignment in self.assignments
        }


@dataclass(frozen=True)
class MappingIssue:
    """A stable, UI-friendly explanation of a mapping decision."""

    code: str
    message: str
    field: RosterField | None = None
    column_indices: tuple[int, ...] = ()


@dataclass(frozen=True)
class RosterMappingSuggestion:
    """Conservative automatic assignments plus unresolved issues."""

    mapping: RosterMapping
    issues: tuple[MappingIssue, ...] = ()

    @property
    def requires_input(self) -> bool:
        return bool(self.issues)


@dataclass(frozen=True)
class RosterMappingTemplate:
    """A reusable mapping containing no source path or student cell values."""

    header_fingerprint: str
    assignments: tuple[ColumnMapping, ...]
    schema_version: int = ROSTER_MAPPING_TEMPLATE_VERSION
    kind: str = ROSTER_MAPPING_TEMPLATE_KIND

    def __post_init__(self) -> None:
        if self.kind != ROSTER_MAPPING_TEMPLATE_KIND:
            raise ValueError(f"Unsupported roster mapping kind: {self.kind!r}")
        if self.schema_version != ROSTER_MAPPING_TEMPLATE_VERSION:
            raise ValueError(
                "Unsupported roster mapping schema version: "
                f"{self.schema_version!r}"
            )
        _validate_header_fingerprint(self.header_fingerprint)
        object.__setattr__(self, "assignments", _ordered_assignments(self.assignments))
        _validate_unique_assignments(self.assignments)

    def to_dict(self) -> dict[str, Any]:
        """Return a deterministic, privacy-safe JSON representation."""

        return {
            "kind": self.kind,
            "schema_version": self.schema_version,
            "header_fingerprint": self.header_fingerprint,
            "mappings": [
                {
                    "field": assignment.field,
                    "column_index": assignment.column_index,
                }
                for assignment in self.assignments
            ],
        }

    @classmethod
    def from_dict(cls, data: Mapping[str, Any]) -> RosterMappingTemplate:
        """Parse a template strictly so corrupted settings fail visibly."""

        if not isinstance(data, Mapping):
            raise TypeError("Roster mapping template must be an object")
        allowed = {"kind", "schema_version", "header_fingerprint", "mappings"}
        unknown = sorted(str(key) for key in data.keys() if key not in allowed)
        if unknown:
            raise ValueError(
                "Unknown roster mapping template fields: " + ", ".join(unknown)
            )
        if set(data.keys()) != allowed:
            missing = sorted(allowed.difference(data.keys()))
            raise ValueError(
                "Missing roster mapping template fields: " + ", ".join(missing)
            )
        raw_mappings = data["mappings"]
        if not isinstance(raw_mappings, list):
            raise TypeError("Roster mapping template mappings must be a list")
        assignments: list[ColumnMapping] = []
        for index, item in enumerate(raw_mappings):
            if not isinstance(item, Mapping):
                raise TypeError(f"Mapping item {index} must be an object")
            if set(item.keys()) != {"field", "column_index"}:
                raise ValueError(
                    f"Mapping item {index} must contain only field and column_index"
                )
            field = item["field"]
            column_index = item["column_index"]
            if not isinstance(field, str):
                raise TypeError(f"Mapping item {index} field must be a string")
            assignments.append(
                ColumnMapping(
                    field=cast(RosterField, field),
                    column_index=column_index,
                )
            )
        schema_version = data["schema_version"]
        if isinstance(schema_version, bool) or not isinstance(schema_version, int):
            raise TypeError("schema_version must be an integer")
        kind = data["kind"]
        fingerprint = data["header_fingerprint"]
        if not isinstance(kind, str):
            raise TypeError("kind must be a string")
        if not isinstance(fingerprint, str):
            raise TypeError("header_fingerprint must be a string")
        return cls(
            kind=kind,
            schema_version=schema_version,
            header_fingerprint=fingerprint,
            assignments=tuple(assignments),
        )


def normalize_roster_header(value: Any) -> str:
    """Normalize header text only; cell values are never normalized here."""

    if value is None:
        return ""
    text = unicodedata.normalize("NFKC", str(value)).strip().casefold()
    # Ignore separators and punctuation commonly introduced by spreadsheet
    # templates (``student id``, ``student_id``, ``身高(cm)``). Retaining only
    # letters and numbers also gives deterministic behavior across locales.
    return "".join(character for character in text if character.isalnum())


def roster_header_fingerprint(table: RosterTable) -> str:
    """Hash ordered normalized headers without retaining their source text."""

    normalized = [normalize_roster_header(column.raw_header) for column in table.columns]
    payload = json.dumps(
        normalized,
        ensure_ascii=False,
        separators=(",", ":"),
    ).encode("utf-8")
    return hashlib.sha256(payload).hexdigest()


def suggest_roster_mapping(table: RosterTable) -> RosterMappingSuggestion:
    """Suggest only uniquely identifiable fields in a deterministic order."""

    aliases = _normalized_aliases()
    assignments: list[ColumnMapping] = []
    issues: list[MappingIssue] = []
    for field in ROSTER_FIELDS:
        matches = tuple(
            column.index
            for column in table.columns
            if normalize_roster_header(column.raw_header) in aliases[field]
        )
        if len(matches) == 1:
            assignments.append(ColumnMapping(field=field, column_index=matches[0]))
        elif len(matches) > 1:
            issues.append(
                MappingIssue(
                    code="ambiguous_header",
                    field=field,
                    column_indices=matches,
                    message=(
                        f"More than one column looks like {field}; choose one "
                        "column explicitly."
                    ),
                )
            )

    if table.headerless:
        _add_headerless_identity_suggestions(table, assignments)

    mapping = RosterMapping(
        assignments=tuple(assignments),
        header_fingerprint=roster_header_fingerprint(table),
    )
    if mapping.column_for("student_id") is None and mapping.column_for("name") is None:
        issues.append(
            MappingIssue(
                code="missing_identity",
                message="Map at least one Student ID or Name column.",
            )
        )
    return RosterMappingSuggestion(mapping=mapping, issues=tuple(issues))


def _add_headerless_identity_suggestions(
    table: RosterTable,
    assignments: list[ColumnMapping],
) -> None:
    """Infer only name/ID columns for a headerless file from value shapes."""

    assigned_fields = {assignment.field for assignment in assignments}
    assigned_columns = {assignment.column_index for assignment in assignments}
    samples = table.rows[: min(20, len(table.rows))]
    if not samples:
        return

    candidates: list[tuple[int, float, float]] = []
    for column in table.columns:
        if column.index in assigned_columns:
            continue
        values = [row.cell(column.index) for row in samples]
        non_empty = [str(value).strip() for value in values if str(value).strip()]
        if not non_empty:
            continue
        identifier_ratio = sum(
            _looks_like_identifier(value) for value in non_empty
        ) / len(non_empty)
        name_ratio = sum(_looks_like_person_name(value) for value in non_empty) / len(
            non_empty
        )
        candidates.append((column.index, identifier_ratio, name_ratio))

    if "student_id" not in assigned_fields:
        id_candidates = sorted(
            (item for item in candidates if item[1] >= 0.6),
            key=lambda item: (-item[1], item[0]),
        )
        if id_candidates:
            assignments.append(
                ColumnMapping(field="student_id", column_index=id_candidates[0][0])
            )
            assigned_columns.add(id_candidates[0][0])

    if "name" not in assigned_fields:
        name_candidates = sorted(
            (
                item
                for item in candidates
                if item[2] >= 0.6 and item[0] not in assigned_columns
            ),
            key=lambda item: (-item[2], item[0]),
        )
        if name_candidates:
            assignments.append(
                ColumnMapping(field="name", column_index=name_candidates[0][0])
            )


def _looks_like_identifier(value: str) -> bool:
    text = value.strip()
    return bool(re.fullmatch(r"[A-Za-z]*\d{4,}", text))


def _looks_like_person_name(value: str) -> bool:
    text = value.strip()
    if _looks_like_identifier(text) or any(character.isdigit() for character in text):
        return False
    return bool(text) and all(
        character.isalpha() or "\u4e00" <= character <= "\u9fff"
        for character in text
    )


def create_roster_mapping(
    table: RosterTable,
    assignments: Mapping[str, int | None] | Iterable[ColumnMapping],
    *,
    require_identity: bool = True,
) -> RosterMapping:
    """Create a mapping from manual UI selections and validate column reuse."""

    if isinstance(assignments, Mapping):
        parsed: list[ColumnMapping] = []
        for field in ROSTER_FIELDS:
            if field not in assignments or assignments[field] is None:
                continue
            parsed.append(
                ColumnMapping(
                    field=field,
                    column_index=cast(int, assignments[field]),
                )
            )
        unknown = sorted(str(key) for key in assignments if key not in _ROSTER_FIELD_SET)
        if unknown:
            raise ValueError("Unknown roster fields: " + ", ".join(unknown))
    else:
        parsed = list(assignments)

    mapping = RosterMapping(
        assignments=tuple(parsed),
        header_fingerprint=roster_header_fingerprint(table),
    )
    for assignment in mapping.assignments:
        if assignment.column_index >= table.column_count:
            raise ValueError(
                f"Column {assignment.column_index} is outside this "
                f"{table.column_count}-column roster."
            )
    if require_identity and (
        mapping.column_for("student_id") is None
        and mapping.column_for("name") is None
    ):
        raise ValueError("Map at least one of student_id or name")
    return mapping


def create_roster_mapping_template(
    table: RosterTable,
    mapping: RosterMapping,
) -> RosterMappingTemplate:
    """Create a versioned reusable template after checking its source table."""

    _ensure_mapping_matches_table(table, mapping)
    return RosterMappingTemplate(
        header_fingerprint=mapping.header_fingerprint,
        assignments=mapping.assignments,
    )


def apply_roster_mapping_template(
    table: RosterTable,
    template: RosterMappingTemplate | Mapping[str, Any],
) -> RosterMapping:
    """Apply a template only to the exact ordered header structure it describes."""

    parsed = (
        template
        if isinstance(template, RosterMappingTemplate)
        else RosterMappingTemplate.from_dict(template)
    )
    actual_fingerprint = roster_header_fingerprint(table)
    if parsed.header_fingerprint != actual_fingerprint:
        raise ValueError(
            "This mapping template was created for a different column layout."
        )
    return create_roster_mapping(table, parsed.assignments)


def records_from_roster_mapping(
    table: RosterTable,
    mapping: RosterMapping,
) -> tuple[dict[str, Any], ...]:
    """Project raw table rows into canonical records without changing values.

    Blank physical rows remain as empty records. The legacy student converter
    skips them, and retaining their positions keeps any validation row numbers
    aligned with the original spreadsheet.
    """

    _ensure_mapping_matches_table(table, mapping)
    if mapping.column_for("student_id") is None and mapping.column_for("name") is None:
        raise ValueError("Map at least one of student_id or name")
    return tuple(
        {
            assignment.field: row.cell(assignment.column_index)
            for assignment in mapping.assignments
            if not _is_empty_cell(row.cell(assignment.column_index))
        }
        for row in table.rows
    )


def students_from_roster_mapping(
    table: RosterTable,
    mapping: RosterMapping,
) -> tuple[Student, ...]:
    """Build validated students through the established conversion behavior."""

    records = records_from_roster_mapping(table, mapping)
    return tuple(students_from_records(records))


def _normalized_aliases() -> dict[RosterField, frozenset[str]]:
    aliases: dict[RosterField, frozenset[str]] = {}
    for field in ROSTER_FIELDS:
        values = set(COLUMN_ALIASES.get(field, ()))
        values.add(field)
        aliases[field] = frozenset(normalize_roster_header(value) for value in values)
    return aliases


def _ordered_assignments(
    assignments: Iterable[ColumnMapping],
) -> tuple[ColumnMapping, ...]:
    order = {field: index for index, field in enumerate(ROSTER_FIELDS)}
    return tuple(sorted(assignments, key=lambda item: order[item.field]))


def _validate_unique_assignments(assignments: tuple[ColumnMapping, ...]) -> None:
    fields = [assignment.field for assignment in assignments]
    duplicate_fields = sorted({field for field in fields if fields.count(field) > 1})
    if duplicate_fields:
        raise ValueError("Roster fields mapped more than once: " + ", ".join(duplicate_fields))
    columns = [assignment.column_index for assignment in assignments]
    duplicate_columns = sorted(
        {column for column in columns if columns.count(column) > 1}
    )
    if duplicate_columns:
        raise ValueError(
            "Source columns mapped more than once: "
            + ", ".join(str(column) for column in duplicate_columns)
        )


def _validate_header_fingerprint(value: str) -> None:
    if not isinstance(value, str) or len(value) != 64:
        raise ValueError("header_fingerprint must be a SHA-256 hexadecimal digest")
    try:
        int(value, 16)
    except ValueError as exc:
        raise ValueError(
            "header_fingerprint must be a SHA-256 hexadecimal digest"
        ) from exc


def _ensure_mapping_matches_table(
    table: RosterTable,
    mapping: RosterMapping,
) -> None:
    if mapping.header_fingerprint != roster_header_fingerprint(table):
        raise ValueError("Roster mapping does not match this table's columns.")
    for assignment in mapping.assignments:
        if assignment.column_index >= table.column_count:
            raise ValueError(
                f"Mapped column {assignment.column_index} is outside this roster."
            )


def _is_empty_cell(value: Any) -> bool:
    return value is None or (isinstance(value, str) and not value.strip())


__all__ = [
    "ColumnMapping",
    "MappingIssue",
    "ROSTER_FIELDS",
    "ROSTER_MAPPING_TEMPLATE_KIND",
    "ROSTER_MAPPING_TEMPLATE_VERSION",
    "RosterField",
    "RosterMapping",
    "RosterMappingSuggestion",
    "RosterMappingTemplate",
    "apply_roster_mapping_template",
    "create_roster_mapping",
    "create_roster_mapping_template",
    "normalize_roster_header",
    "records_from_roster_mapping",
    "roster_header_fingerprint",
    "students_from_roster_mapping",
    "suggest_roster_mapping",
]
