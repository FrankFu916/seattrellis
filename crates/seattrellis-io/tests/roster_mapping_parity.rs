//! Roster-mapping parity: the Rust suggestion engine must reproduce the
//! Python oracle's `suggest_roster_mapping` (roster_mapping.py:238) on the
//! shared fixture corpus `fixtures/roster-mapping/` — assignments as
//! `{canonical_field: column_index}` plus issue codes/fields/indices. The
//! `expected.json` is recorded from the Python oracle (ledger §19.36), so
//! this test IS the automatic differential: any divergence in alias
//! coverage, headerless heuristics, or issue emission fails here.

use std::collections::BTreeMap;
use std::path::PathBuf;

use seattrellis_io::roster::parse_roster_csv;
use serde_json::Value;

fn fixtures_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/roster-mapping")
}

fn expected() -> Value {
    let text = std::fs::read_to_string(fixtures_root().join("expected.json")).unwrap();
    serde_json::from_str(&text).unwrap()
}

#[test]
fn roster_mapping_matches_the_python_oracle_corpus() {
    let expected = expected();
    let cases = expected.as_object().unwrap();
    assert!(cases.len() >= 8, "corpus should have a meaningful matrix");
    let mut failures: Vec<String> = Vec::new();

    for (case_name, case) in cases {
        let csv = fixtures_root().join(format!("{case_name}.csv"));
        let draft = match parse_roster_csv(&std::fs::read(&csv).unwrap()) {
            Ok(draft) => draft,
            Err(error) => {
                failures.push(format!("{case_name}: parse failed: {error}"));
                continue;
            }
        };
        let response = draft.to_response();

        // assignments: {field: column_index} sorted by field name.
        let mut rust_assignments: BTreeMap<String, usize> = BTreeMap::new();
        for item in &response.suggested_mapping {
            let field = serde_json::to_value(&item.field).unwrap();
            rust_assignments.insert(field.as_str().unwrap().to_string(), item.column_index);
        }
        let oracle_assignments: BTreeMap<String, usize> = case["assignments"]
            .as_object()
            .unwrap()
            .iter()
            .map(|(field, index)| (field.clone(), index.as_u64().unwrap() as usize))
            .collect();
        if rust_assignments != oracle_assignments {
            failures.push(format!(
                "{case_name}: assignments rust={rust_assignments:?} oracle={oracle_assignments:?}"
            ));
        }

        // issues: (code, field, column_indices) as tuples for ordering.
        let rust_issues: Vec<(String, Option<String>, Vec<usize>)> = response
            .mapping_issues
            .iter()
            .map(|issue| {
                let field = issue
                    .field
                    .as_ref()
                    .map(|field| serde_json::to_value(field).unwrap())
                    .map(|value| value.as_str().unwrap().to_string());
                (issue.code.clone(), field, issue.column_indices.clone())
            })
            .collect();
        let oracle_issues: Vec<(String, Option<String>, Vec<usize>)> = case["issues"]
            .as_array()
            .unwrap()
            .iter()
            .map(|issue| {
                (
                    issue["code"].as_str().unwrap().to_string(),
                    issue["field"].as_str().map(|value| value.to_string()),
                    issue["column_indices"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|index| index.as_u64().unwrap() as usize)
                        .collect(),
                )
            })
            .collect();
        if rust_issues != oracle_issues {
            failures.push(format!(
                "{case_name}: issues rust={rust_issues:?} oracle={oracle_issues:?}"
            ));
        }
    }

    assert!(
        failures.is_empty(),
        "roster-mapping diverges from the Python oracle:\n{}",
        failures.join("\n")
    );
}
