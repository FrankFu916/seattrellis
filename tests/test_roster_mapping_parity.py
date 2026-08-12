"""Oracle guard for the roster-mapping parity corpus (ledger §19.36).

`fixtures/roster-mapping/expected.json` is the recorded Python oracle
output; the Rust integration test `roster_mapping_parity.rs` compares
the Rust suggestion engine against it. This test keeps the golden in
sync with the oracle implementation — if the mapping heuristics change
intentionally, regenerate the golden and update both sides together.
"""

from __future__ import annotations

import json
from pathlib import Path

from seattrellis.application.roster_mapping import suggest_roster_mapping
from seattrellis.io.roster_table import read_roster_table_bytes

FIXTURES = Path(__file__).resolve().parents[1] / "fixtures" / "roster-mapping"


def _recorded():
    return json.loads((FIXTURES / "expected.json").read_text(encoding="utf-8"))


def _current():
    recorded = _recorded()
    current = {}
    for case_name in recorded:
        content = (FIXTURES / f"{case_name}.csv").read_text(encoding="utf-8")
        table = read_roster_table_bytes(content.encode(), filename=f"{case_name}.csv")
        suggestion = suggest_roster_mapping(table)
        current[case_name] = {
            "assignments": dict(suggestion.mapping.as_dict()),
            "issues": [
                {
                    "code": issue.code,
                    "field": issue.field,
                    "column_indices": list(issue.column_indices),
                }
                for issue in suggestion.issues
            ],
        }
    return current


def test_recorded_golden_matches_the_oracle() -> None:
    assert _current() == _recorded(), (
        "fixtures/roster-mapping/expected.json drifted from "
        "suggest_roster_mapping; regenerate it from the oracle and "
        "re-verify the Rust parity test"
    )
