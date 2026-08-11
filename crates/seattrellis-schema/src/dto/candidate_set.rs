//! Candidate-set artifact DTO (plan §4.2/§17.2): typed v2 payload mirroring
//! the Python `CandidateSet` model (models/candidate.py) and the oracle
//! `schemas/candidate-set.schema.json` (schema_version 0.2.2). Strict
//! `deny_unknown_fields` keeps the durable artifact contract honest; the
//! audit API's `hard_constraint_summary.all_satisfied` (v2 UI contract) is a
//! different layer — this DTO follows the oracle field names.

use std::collections::HashMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::snapshot::SeatingSnapshotArtifact;

/// `ScoreDimension` from scoring.py: per-dimension score with rating band.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ScoreDimension {
    pub status: String,
    #[serde(default)]
    pub score: Option<f64>,
    #[serde(default)]
    pub raw_value: Option<f64>,
    #[serde(default)]
    pub weight: f64,
    #[serde(default)]
    pub rating: String,
    #[serde(default)]
    pub details: Value,
}

/// `HardConstraintSummary` from scoring.py (oracle field name `satisfied`).
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct HardConstraintSummary {
    pub satisfied: bool,
    #[serde(default)]
    pub checked_rule_count: u32,
    #[serde(default)]
    pub violation_count: u32,
    #[serde(default)]
    pub violations: Vec<String>,
    #[serde(default)]
    pub details: Value,
}

/// `ScoreBreakdown` from scoring.py: seven named dimensions plus per-rule
/// scores and the hard-constraint summary.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ScoreBreakdown {
    pub fair_rotation_score: ScoreDimension,
    pub avoid_recent_neighbors_score: ScoreDimension,
    pub score_balance_score: ScoreDimension,
    pub height_preference_score: ScoreDimension,
    pub vision_preference_score: ScoreDimension,
    pub diversity_score: ScoreDimension,
    pub stability_score: ScoreDimension,
    #[serde(default)]
    pub rule_scores: HashMap<String, ScoreDimension>,
    pub hard_constraint_summary: HardConstraintSummary,
}

/// `PlanScore` from scoring.py.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PlanScore {
    pub total: f64,
    pub breakdown: ScoreBreakdown,
}

/// `CandidatePlan` from candidate.py.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CandidatePlan {
    pub candidate_id: String,
    pub snapshot: SeatingSnapshotArtifact,
    pub score: PlanScore,
    pub hard_constraints_satisfied: bool,
    #[serde(default)]
    pub warnings: Vec<String>,
    #[serde(default)]
    pub metadata: Value,
}

/// `CandidateSet` from candidate.py (schema_version 0.2.2, kind
/// `candidate_set`).
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CandidateSetArtifact {
    pub schema_version: String,
    pub kind: String,
    #[serde(default)]
    pub created_at: String,
    #[serde(default)]
    pub metadata: Value,
    pub candidates: Vec<CandidatePlan>,
    pub recommended_candidate_id: String,
    #[serde(default)]
    pub warnings: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn candidate_set_round_trip_preserves_all_fields() {
        let document = r#"{
            "schema_version": "0.2.2",
            "kind": "candidate_set",
            "created_at": "2026-03-17T10:24:00Z",
            "metadata": {"seed": 168996},
            "candidates": [{
                "candidate_id": "cand-1",
                "snapshot": {
                    "schema_version": "0.2.2",
                    "created_at": "2026-03-17T10:24:00Z",
                    "seed": 168996,
                    "metadata": {},
                    "students": [],
                    "layout": {
                        "layout_id": "class-302",
                        "name": "302",
                        "seats": [],
                        "adjacency": {"include_horizontal": true, "include_vertical": true}
                    },
                    "rules": {"schema_version": 0, "seed": 168996, "hard": {}, "soft": {}},
                    "assignments": [],
                    "solver_status": "Solved"
                },
                "score": {
                    "total": 84.4,
                    "breakdown": {
                        "fair_rotation_score": {"status": "available", "score": 82.4, "raw_value": 0.2, "weight": 10, "rating": "high", "details": {}},
                        "avoid_recent_neighbors_score": {"status": "available", "score": 91.0, "weight": 10, "rating": "high", "details": {}},
                        "score_balance_score": {"status": "available", "score": 76.5, "weight": 10, "rating": "medium", "details": {}},
                        "height_preference_score": {"status": "available", "score": 88.2, "weight": 10, "rating": "high", "details": {}},
                        "vision_preference_score": {"status": "available", "score": 96.8, "weight": 20, "rating": "high", "details": {}},
                        "diversity_score": {"status": "available", "score": 70.1, "weight": 10, "rating": "medium", "details": {}},
                        "stability_score": {"status": "available", "score": 85.6, "weight": 10, "rating": "high", "details": {}},
                        "rule_scores": {},
                        "hard_constraint_summary": {"satisfied": true, "checked_rule_count": 1, "violation_count": 0, "violations": [], "details": {}}
                    }
                },
                "hard_constraints_satisfied": true,
                "warnings": [],
                "metadata": {}
            }],
            "recommended_candidate_id": "cand-1",
            "warnings": []
        }"#;
        let parsed: CandidateSetArtifact = serde_json::from_str(document).unwrap();
        let reencoded = serde_json::to_string(&parsed).unwrap();
        let reparsed: CandidateSetArtifact = serde_json::from_str(&reencoded).unwrap();
        assert_eq!(parsed.recommended_candidate_id, reparsed.recommended_candidate_id);
        assert_eq!(parsed.candidates.len(), 1);
        assert_eq!(parsed.candidates[0].score.total, 84.4);
        assert!(parsed.candidates[0].score.breakdown.hard_constraint_summary.satisfied);
        assert_eq!(parsed.candidates[0].snapshot.solver_status, "Solved");
    }

    #[test]
    fn candidate_set_rejects_unknown_fields() {
        let document = r#"{
            "schema_version": "0.2.2",
            "kind": "candidate_set",
            "created_at": "2026-03-17T10:24:00Z",
            "metadata": {},
            "candidates": [],
            "recommended_candidate_id": "cand-1",
            "warnings": [],
            "unexpected_field": 1
        }"#;
        assert!(serde_json::from_str::<CandidateSetArtifact>(document).is_err());
    }

    #[test]
    fn hard_constraint_summary_uses_oracle_field_name_satisfied() {
        // The durable artifact contract follows the oracle schema; the v2
        // audit API's `all_satisfied` is a separate UI-facing contract.
        let summary: HardConstraintSummary = serde_json::from_str(
            r#"{"satisfied": false, "checked_rule_count": 2, "violation_count": 1, "violations": ["x"], "details": {}}"#,
        )
        .unwrap();
        assert!(!summary.satisfied);
        assert_eq!(summary.violation_count, 1);
    }
}
