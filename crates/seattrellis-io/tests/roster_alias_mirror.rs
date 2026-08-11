//! Mirror gate for ledger §16 D.15: the header-alias table must exactly match
//! the Python oracle's COLUMN_ALIASES (io/students.py). Each alias below is
//! transcribed from that table; a missing alias is a mapping-parity regression.

use seattrellis_io::roster::{parse_roster_csv, RosterField};

fn assert_alias(field: RosterField, alias: &str) {
    let draft = parse_roster_csv(format!("{alias}\n1\n").as_bytes())
        .unwrap_or_else(|error| panic!("alias {alias:?} failed to parse: {error}"));
    let found = draft
        .suggested_mapping
        .iter()
        .find(|item| item.field == field);
    assert!(
        found.is_some(),
        "alias {alias:?} must map to {} (mirrors Python COLUMN_ALIASES); actual: {:?}",
        field.as_str(),
        draft
            .suggested_mapping
            .iter()
            .map(|i| (i.field.as_str(), i.column_index))
            .collect::<Vec<_>>()
    );
}

#[test]
fn mirrors_python_column_aliases_exactly() {
    let table: &[(RosterField, &[&str])] = &[
        (
            RosterField::StudentId,
            &["student_id", "id", "sid", "学号", "学生编号", "编号"],
        ),
        (RosterField::Name, &["name", "姓名", "学生姓名"]),
        (RosterField::Gender, &["gender", "sex", "性别"]),
        (
            RosterField::HeightCm,
            &["height_cm", "height", "身高", "身高cm", "身高_cm"],
        ),
        (RosterField::Score, &["score", "成绩", "总分", "分数"]),
        (RosterField::Vision, &["vision", "视力"]),
        (RosterField::Notes, &["notes", "note", "备注", "说明"]),
        (RosterField::Tags, &["tags", "tag", "标签"]),
        (RosterField::Needs, &["needs", "need", "特殊需求", "需求"]),
    ];
    for &(field, aliases) in table {
        for alias in aliases {
            assert_alias(field, alias);
        }
    }
}

#[test]
fn normalization_matches_python_rule() {
    // Python: alias.lower().replace(" ", "").replace("_", "")
    for (raw, expected) in [
        (" 学号 ", "学号"),
        ("Height_CM", "heightcm"),
        ("student id", "studentid"),
        ("特殊 需求", "特殊需求"),
    ] {
        assert_eq!(
            seattrellis_io::roster::normalize_header(raw),
            expected,
            "raw {raw:?}"
        );
    }
}
