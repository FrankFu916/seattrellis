from __future__ import annotations

import json

import pytest

from seattrellis.application.roster_mapping import (
    RosterMappingTemplate,
    apply_roster_mapping_template,
    create_roster_mapping,
    create_roster_mapping_template,
    records_from_roster_mapping,
    students_from_roster_mapping,
    suggest_roster_mapping,
)
from seattrellis.io.roster_table import read_roster_table_bytes


def _table(content: str):
    return read_roster_table_bytes(content.encode(), filename="roster.csv")


def test_suggestions_are_deterministic_and_cover_known_chinese_aliases() -> None:
    table = _table("姓名,学号,身高(cm),总分,特殊需求\n小林,001,158,91,靠前\n")

    first = suggest_roster_mapping(table)
    second = suggest_roster_mapping(table)

    assert first == second
    assert first.requires_input is False
    assert first.mapping.as_dict() == {
        "student_id": 1,
        "name": 0,
        "height_cm": 2,
        "score": 3,
        "needs": 4,
    }


def test_headerless_roster_suggests_name_and_long_numeric_id_columns() -> None:
    table = _table("小林,18513806422\n小周,18513806423\n")

    suggestion = suggest_roster_mapping(table)

    assert suggestion.mapping.as_dict() == {"student_id": 1, "name": 0}
    assert suggestion.requires_input is False


def test_duplicate_alias_headers_are_left_for_manual_mapping() -> None:
    table = _table("学号,姓名,姓名\n1,Alice,Alias\n")

    suggestion = suggest_roster_mapping(table)

    assert suggestion.mapping.column_for("student_id") == 0
    assert suggestion.mapping.column_for("name") is None
    assert suggestion.requires_input is True
    issue = next(issue for issue in suggestion.issues if issue.field == "name")
    assert issue.code == "ambiguous_header"
    assert issue.column_indices == (1, 2)

    mapping = create_roster_mapping(table, {"student_id": 0, "name": 2})
    assert mapping.column_for("name") == 2


def test_manual_mapping_rejects_column_reuse_invalid_columns_and_missing_identity() -> None:
    table = _table("A,B\n1,Alice\n")

    with pytest.raises(ValueError, match="Source columns mapped more than once"):
        create_roster_mapping(table, {"student_id": 0, "name": 0})
    with pytest.raises(ValueError, match="outside this 2-column roster"):
        create_roster_mapping(table, {"name": 5})
    with pytest.raises(ValueError, match="at least one of student_id or name"):
        create_roster_mapping(table, {"score": 1})


def test_mapping_template_is_versioned_and_contains_no_source_or_student_values() -> None:
    table = _table("Internal number,Legal name,Score\nS-001,Alice Secret,91\n")
    mapping = create_roster_mapping(
        table,
        {"student_id": 0, "name": 1, "score": 2},
    )

    template = create_roster_mapping_template(table, mapping)
    data = template.to_dict()
    serialized = json.dumps(data)

    assert data["schema_version"] == 1
    assert data["kind"] == "seattrellis_roster_mapping"
    assert "Alice Secret" not in serialized
    assert "S-001" not in serialized
    assert "roster.csv" not in serialized
    assert "Internal number" not in serialized

    restored = RosterMappingTemplate.from_dict(data)
    assert apply_roster_mapping_template(table, restored) == mapping


def test_mapping_template_rejects_a_changed_header_structure() -> None:
    original = _table("id,name\n1,Alice\n")
    changed = _table("name,id\nAlice,1\n")
    mapping = create_roster_mapping(original, {"student_id": 0, "name": 1})
    template = create_roster_mapping_template(original, mapping)

    with pytest.raises(ValueError, match="different column layout"):
        apply_roster_mapping_template(changed, template)


def test_mapping_preserves_raw_values_until_established_student_conversion() -> None:
    table = _table("Identifier,Display,Mark\n 001 , Alice ,091\n,,\n002,Bob,80\n")
    mapping = create_roster_mapping(
        table,
        {"student_id": 0, "name": 1, "score": 2},
    )

    records = records_from_roster_mapping(table, mapping)
    students = students_from_roster_mapping(table, mapping)

    assert records == (
        {"student_id": " 001 ", "name": " Alice ", "score": "091"},
        {},
        {"student_id": "002", "name": "Bob", "score": "80"},
    )
    assert [student.student_id for student in students] == ["001", "002"]
    assert students[0].name == "Alice"
    assert students[0].score == 91


def test_mapping_template_parser_is_strict() -> None:
    table = _table("id,name\n1,Alice\n")
    mapping = create_roster_mapping(table, {"student_id": 0, "name": 1})
    data = create_roster_mapping_template(table, mapping).to_dict()

    with pytest.raises(ValueError, match="Unsupported roster mapping schema"):
        RosterMappingTemplate.from_dict({**data, "schema_version": 2})
    with pytest.raises(ValueError, match="Unknown roster mapping template fields"):
        RosterMappingTemplate.from_dict({**data, "source_path": "/private/class.xlsx"})
