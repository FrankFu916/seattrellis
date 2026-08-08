//! `ClassroomLayout` v2 payload: seats, adjacency and metadata.
//!
//! Mirrors schemas/classroom-layout.schema.json (SeatNode + AdjacencyConfig).

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ClassroomLayout {
    #[serde(default)]
    pub layout_id: String,
    #[serde(default)]
    pub name: String,
    pub seats: Vec<SeatNode>,
    #[serde(default)]
    pub adjacency: AdjacencyConfig,
    #[serde(default)]
    pub metadata: serde_json::Value,
}

/// A seat node (v1 `SeatNode`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SeatNode {
    pub seat_id: String,
    pub row: i32,
    pub col: i32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub x: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub y: Option<f64>,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub zone: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group_id: Option<String>,
    #[serde(default)]
    pub near_window: bool,
    #[serde(default)]
    pub near_door: bool,
    #[serde(default)]
    pub near_platform: bool,
    #[serde(default)]
    pub near_ac: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    #[serde(default, skip_serializing_if = "serde_json::Value::is_null")]
    pub attributes: serde_json::Value,
}

/// Adjacency derivation rules (v1 `AdjacencyConfig`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AdjacencyConfig {
    #[serde(default = "default_true")]
    pub include_horizontal: bool,
    #[serde(default)]
    pub include_vertical: bool,
    #[serde(default)]
    pub include_diagonal: bool,
    #[serde(default = "default_one")]
    pub max_row_delta: i32,
    #[serde(default = "default_one")]
    pub max_col_delta: i32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_distance: Option<f64>,
    #[serde(default = "default_true")]
    pub use_xy_distance: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub custom_edges: Vec<(String, String)>,
}

impl Default for AdjacencyConfig {
    /// v1 defaults: horizontal adjacency only, delta 1, xy distances.
    fn default() -> Self {
        AdjacencyConfig {
            include_horizontal: true,
            include_vertical: false,
            include_diagonal: false,
            max_row_delta: 1,
            max_col_delta: 1,
            max_distance: None,
            use_xy_distance: true,
            custom_edges: Vec::new(),
        }
    }
}

fn default_true() -> bool {
    true
}

fn default_one() -> i32 {
    1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_with_zone_and_custom_edges() {
        let layout = ClassroomLayout {
            layout_id: "room-1".into(),
            name: "教室 A".into(),
            seats: vec![SeatNode {
                seat_id: "R1C1".into(),
                row: 1,
                col: 1,
                x: Some(1.0),
                y: Some(1.0),
                enabled: true,
                zone: Some("front".into()),
                group_id: None,
                near_window: true,
                near_door: false,
                near_platform: true,
                near_ac: false,
                tags: vec![],
                attributes: serde_json::Value::Null,
            }],
            adjacency: AdjacencyConfig {
                include_horizontal: true,
                include_vertical: true,
                include_diagonal: false,
                max_row_delta: 1,
                max_col_delta: 1,
                max_distance: None,
                use_xy_distance: true,
                custom_edges: vec![("R1C1".into(), "R2C2".into())],
            },
            metadata: serde_json::json!({ "platform": "front" }),
        };
        let json = serde_json::to_string(&layout).unwrap();
        let parsed: ClassroomLayout = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, layout);
    }

    #[test]
    fn unknown_seat_fields_are_rejected() {
        let json = r#"{"seats":[{"seat_id":"S1","row":1,"col":1,"mystery":1}]}"#;
        assert!(serde_json::from_str::<ClassroomLayout>(json).is_err());
    }

    #[test]
    fn defaults_apply_for_missing_optional_fields() {
        let json = r#"{"seats":[{"seat_id":"S1","row":1,"col":1}]}"#;
        let parsed: ClassroomLayout = serde_json::from_str(json).unwrap();
        assert!(parsed.seats[0].enabled);
        assert!(parsed.adjacency.include_horizontal);
        assert!(!parsed.adjacency.include_vertical);
    }
}
