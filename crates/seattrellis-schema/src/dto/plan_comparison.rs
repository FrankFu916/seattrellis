//! Plan-comparison-report artifact DTO (plan §4.2/§17.2): typed v2 payload
//! mirroring the Python `PlanComparisonReport` model (models/candidate.py)
//! and the oracle `schemas/plan-comparison-report.schema.json` (schema
//! version 0.2.2). Strict `deny_unknown_fields`; cross-field invariants
//! (unique candidate ids, candidate_count match, recommended reference) are
//! validated on parse, mirroring the Python model validators.

use std::collections::HashMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// `PlanComparisonExplanation` from candidate.py.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PlanComparisonExplanation {
    pub kind: String,
    pub dimension: String,
    pub score: f64,
    pub rating: String,
}

/// `PlanComparisonEntry` from candidate.py.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PlanComparisonEntry {
    pub candidate_id: String,
    pub total_score: f64,
    #[serde(default)]
    pub score_delta_from_recommended: Option<f64>,
    pub hard_constraints_satisfied: bool,
    #[serde(default)]
    pub hard_constraint_checked_count: Option<u32>,
    #[serde(default)]
    pub hard_constraint_violation_count: Option<u32>,
    #[serde(default)]
    pub dimension_scores: HashMap<String, Option<f64>>,
    #[serde(default)]
    pub explanations: Vec<PlanComparisonExplanation>,
    #[serde(default)]
    pub advantages: Vec<String>,
    #[serde(default)]
    pub costs: Vec<String>,
    #[serde(default)]
    pub history_comparison: HashMap<String, String>,
}

/// `PlanComparisonReport` from candidate.py (schema_version 0.2.2, kind
/// `plan_comparison_report`).
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PlanComparisonReportArtifact {
    pub schema_version: String,
    pub kind: String,
    #[serde(default)]
    pub created_at: String,
    pub candidate_count: u32,
    pub recommended_candidate_id: String,
    pub candidates: Vec<PlanComparisonEntry>,
    #[serde(default)]
    pub warnings: Vec<String>,
    #[serde(default)]
    pub metadata: serde_json::Value,
}

impl PlanComparisonReportArtifact {
    /// Cross-field invariants from the Python model validators:
    /// non-empty candidates, unique ids, `candidate_count` match, and the
    /// recommended id referencing a candidate.
    pub fn validate_references(&self) -> Result<(), String> {
        if self.candidates.is_empty() {
            return Err("plan comparison report must contain at least one candidate".to_string());
        }
        let mut seen = std::collections::HashSet::new();
        for candidate in &self.candidates {
            if !seen.insert(candidate.candidate_id.as_str()) {
                return Err(format!(
                    "plan comparison report candidate_id values must be unique: {}",
                    candidate.candidate_id
                ));
            }
        }
        if self.candidate_count as usize != self.candidates.len() {
            return Err(format!(
                "candidate_count ({}) must match the number of report candidates ({})",
                self.candidate_count,
                self.candidates.len()
            ));
        }
        if !seen.contains(self.recommended_candidate_id.as_str()) {
            return Err(format!(
                "recommended_candidate_id ({}) must reference a report candidate",
                self.recommended_candidate_id
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_document() -> &'static str {
        r#"{
            "schema_version": "0.2.2",
            "kind": "plan_comparison_report",
            "created_at": "2026-03-17T10:25:00Z",
            "candidate_count": 2,
            "recommended_candidate_id": "cand-A",
            "candidates": [
                {
                    "candidate_id": "cand-A",
                    "total_score": 84.4,
                    "score_delta_from_recommended": 0.0,
                    "hard_constraints_satisfied": true,
                    "hard_constraint_checked_count": 1,
                    "hard_constraint_violation_count": 0,
                    "dimension_scores": {"fair_rotation": 82.4, "stability": 85.6},
                    "explanations": [
                        {"kind": "advantage", "dimension": "fair_rotation", "score": 82.4, "rating": "high"}
                    ],
                    "advantages": ["轮换更公平"],
                    "costs": ["成绩搭配略差"],
                    "history_comparison": {"period": "第 3 期"}
                },
                {
                    "candidate_id": "cand-B",
                    "total_score": 82.9,
                    "score_delta_from_recommended": -1.5,
                    "hard_constraints_satisfied": true,
                    "hard_constraint_checked_count": 1,
                    "hard_constraint_violation_count": 0,
                    "dimension_scores": {"score_balance": 81.2},
                    "explanations": [],
                    "advantages": [],
                    "costs": [],
                    "history_comparison": {}
                }
            ],
            "warnings": [],
            "metadata": {"seed": 168996}
        }"#
    }

    #[test]
    fn plan_comparison_round_trip_preserves_all_fields() {
        let parsed: PlanComparisonReportArtifact =
            serde_json::from_str(sample_document()).unwrap();
        assert!(parsed.validate_references().is_ok());
        let reencoded = serde_json::to_string(&parsed).unwrap();
        let reparsed: PlanComparisonReportArtifact = serde_json::from_str(&reencoded).unwrap();
        assert_eq!(reparsed.candidate_count, 2);
        assert_eq!(reparsed.recommended_candidate_id, "cand-A");
        assert_eq!(reparsed.candidates[0].advantages, vec!["轮换更公平"]);
        assert_eq!(
            reparsed.candidates[1].score_delta_from_recommended,
            Some(-1.5)
        );
        assert_eq!(
            reparsed.candidates[0].dimension_scores["fair_rotation"],
            Some(82.4)
        );
    }

    #[test]
    fn plan_comparison_rejects_unknown_fields() {
        let document = r#"{
            "schema_version": "0.2.2",
            "kind": "plan_comparison_report",
            "created_at": "2026-03-17T10:25:00Z",
            "candidate_count": 1,
            "recommended_candidate_id": "cand-A",
            "candidates": [],
            "warnings": [],
            "metadata": {},
            "unexpected": true
        }"#;
        assert!(serde_json::from_str::<PlanComparisonReportArtifact>(document).is_err());
    }

    #[test]
    fn plan_comparison_cross_field_invariants() {
        // Empty candidates.
        let empty = r#"{"schema_version":"0.2.2","kind":"plan_comparison_report","created_at":"2026-03-17T10:25:00Z","candidate_count":0,"recommended_candidate_id":"x","candidates":[],"warnings":[],"metadata":{}}"#;
        let parsed: PlanComparisonReportArtifact = serde_json::from_str(empty).unwrap();
        assert!(parsed.validate_references().is_err());

        // Duplicate candidate ids.
        let dup = r#"{"schema_version":"0.2.2","kind":"plan_comparison_report","created_at":"2026-03-17T10:25:00Z","candidate_count":2,"recommended_candidate_id":"a","candidates":[{"candidate_id":"a","total_score":1.0,"hard_constraints_satisfied":true},{"candidate_id":"a","total_score":2.0,"hard_constraints_satisfied":true}],"warnings":[],"metadata":{}}"#;
        let parsed: PlanComparisonReportArtifact = serde_json::from_str(dup).unwrap();
        assert!(parsed.validate_references().is_err());

        // Recommended id not referencing a candidate.
        let bad_ref = r#"{"schema_version":"0.2.2","kind":"plan_comparison_report","created_at":"2026-03-17T10:25:00Z","candidate_count":1,"recommended_candidate_id":"nope","candidates":[{"candidate_id":"a","total_score":1.0,"hard_constraints_satisfied":true}],"warnings":[],"metadata":{}}"#;
        let parsed: PlanComparisonReportArtifact = serde_json::from_str(bad_ref).unwrap();
        assert!(parsed.validate_references().is_err());
    }
}
