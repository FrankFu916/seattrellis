//! Strict, typed DTOs for the versioned editor command/state protocol.
//!
//! These types model `schemas/editor-command.schema.json` and
//! `schemas/editor-state.schema.json`.  The domain editor intentionally has a
//! transport-oriented JSON dispatch layer; durable/protocol schema evidence
//! belongs here and therefore uses typed operation payloads throughout.

use std::collections::HashSet;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

const MAX_COMMAND_OPERATIONS: usize = 100;
const MAX_PROTOCOL_IDENTIFIER_LENGTH: usize = 128;

/// Literal `kind` value for an editor command document.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum EditorCommandKind {
    #[serde(rename = "seattrellis_editor_command")]
    SeatTrellisEditorCommand,
}

/// Literal `kind` value for an editor state document.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum EditorStateKind {
    #[serde(rename = "seattrellis_editor_state")]
    SeatTrellisEditorState,
}

/// Frozen protocol version shared by command and state documents.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum EditorProtocolVersion {
    #[serde(rename = "1.0")]
    V1,
}

/// Action applied to an editor draft.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum EditorAction {
    Apply,
    Undo,
    Redo,
}

/// Payload for `swap_students`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SwapStudentsPayload {
    #[schemars(length(min = 1))]
    pub first_student: String,
    #[schemars(length(min = 1))]
    pub second_student: String,
}

/// Payload shared by `move_student` and `seat_student`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct StudentSeatPayload {
    #[schemars(length(min = 1))]
    pub student_key: String,
    #[schemars(length(min = 1))]
    pub seat_id: String,
}

/// Payload shared by student lock/unlock/unseat operations.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct StudentPayload {
    #[schemars(length(min = 1))]
    pub student_key: String,
}

/// Payload shared by seat lock/unlock operations.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SeatPayload {
    #[schemars(length(min = 1))]
    pub seat_id: String,
}

/// One destination in a `batch_move` operation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BatchMoveItem {
    #[schemars(length(min = 1))]
    pub student_key: String,
    #[schemars(length(min = 1))]
    pub seat_id: String,
}

/// Payload for `batch_move`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BatchMovePayload {
    #[schemars(length(min = 1, max = 100))]
    pub moves: Vec<BatchMoveItem>,
}

impl BatchMovePayload {
    /// Validate command semantics which JSON Schema cannot express completely.
    pub fn validate(&self) -> Result<(), String> {
        if self.moves.is_empty() || self.moves.len() > MAX_COMMAND_OPERATIONS {
            return Err(format!(
                "batch_move must contain between 1 and {MAX_COMMAND_OPERATIONS} moves"
            ));
        }

        let mut students = HashSet::with_capacity(self.moves.len());
        let mut seats = HashSet::with_capacity(self.moves.len());
        for movement in &self.moves {
            validate_reference("batch_move student_key", &movement.student_key)?;
            validate_reference("batch_move seat_id", &movement.seat_id)?;
            if !students.insert(movement.student_key.as_str()) {
                return Err("batch_move students must be unique".to_string());
            }
            if !seats.insert(movement.seat_id.as_str()) {
                return Err("batch_move target seats must be unique".to_string());
            }
        }
        Ok(())
    }
}

/// One of the nine frozen editor operations.
///
/// The adjacent tag representation deliberately preserves the wire shape
/// `{ "kind": "...", "payload": { ... } }`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(
    tag = "kind",
    content = "payload",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum EditingOperation {
    SwapStudents(SwapStudentsPayload),
    MoveStudent(StudentSeatPayload),
    BatchMove(BatchMovePayload),
    SeatStudent(StudentSeatPayload),
    UnseatStudent(StudentPayload),
    LockStudent(StudentPayload),
    UnlockStudent(StudentPayload),
    LockSeat(SeatPayload),
    UnlockSeat(SeatPayload),
}

impl EditingOperation {
    /// Validate non-empty entity references and batch-level invariants.
    pub fn validate(&self) -> Result<(), String> {
        match self {
            Self::SwapStudents(payload) => {
                validate_reference("first_student", &payload.first_student)?;
                validate_reference("second_student", &payload.second_student)
            }
            Self::MoveStudent(payload) | Self::SeatStudent(payload) => {
                validate_reference("student_key", &payload.student_key)?;
                validate_reference("seat_id", &payload.seat_id)
            }
            Self::BatchMove(payload) => payload.validate(),
            Self::UnseatStudent(payload)
            | Self::LockStudent(payload)
            | Self::UnlockStudent(payload) => {
                validate_reference("student_key", &payload.student_key)
            }
            Self::LockSeat(payload) | Self::UnlockSeat(payload) => {
                validate_reference("seat_id", &payload.seat_id)
            }
        }
    }

    fn expanded_count(&self) -> usize {
        match self {
            Self::BatchMove(payload) => payload.moves.len(),
            _ => 1,
        }
    }
}

/// Versioned editor command DTO.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EditorCommand {
    pub kind: EditorCommandKind,
    pub protocol_version: EditorProtocolVersion,
    #[schemars(length(min = 1, max = 128))]
    pub command_id: String,
    #[schemars(length(min = 1, max = 128))]
    pub draft_id: String,
    pub base_revision: u64,
    pub action: EditorAction,
    #[serde(default)]
    #[schemars(length(max = 100))]
    pub operations: Vec<EditingOperation>,
}

impl EditorCommand {
    /// Enforce action/operation semantics and the expanded operation budget.
    pub fn validate(&self) -> Result<(), String> {
        validate_protocol_identifier("command_id", &self.command_id)?;
        validate_protocol_identifier("draft_id", &self.draft_id)?;

        if self.operations.len() > MAX_COMMAND_OPERATIONS {
            return Err(format!(
                "editor commands may contain at most {MAX_COMMAND_OPERATIONS} operations"
            ));
        }
        match self.action {
            EditorAction::Apply if self.operations.is_empty() => {
                return Err("apply commands require at least one operation".to_string());
            }
            EditorAction::Undo | EditorAction::Redo if !self.operations.is_empty() => {
                return Err(format!(
                    "{} commands must not contain operations",
                    match self.action {
                        EditorAction::Undo => "undo",
                        EditorAction::Redo => "redo",
                        EditorAction::Apply => unreachable!(),
                    }
                ));
            }
            _ => {}
        }

        let mut expanded_count = 0usize;
        for operation in &self.operations {
            operation.validate()?;
            expanded_count = expanded_count
                .checked_add(operation.expanded_count())
                .ok_or_else(|| "expanded editor operation count overflowed".to_string())?;
        }
        if expanded_count > MAX_COMMAND_OPERATIONS {
            return Err(format!(
                "editor commands may contain at most {MAX_COMMAND_OPERATIONS} expanded operations"
            ));
        }
        Ok(())
    }
}

/// Per-student state exposed to editor clients.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EditorStudentState {
    #[schemars(length(min = 1))]
    pub student_key: String,
    pub display_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(length(min = 1))]
    pub seat_id: Option<String>,
    #[serde(default)]
    pub locked: bool,
}

/// Per-seat state exposed to editor clients.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EditorSeatState {
    #[schemars(length(min = 1))]
    pub seat_id: String,
    #[schemars(range(min = 1))]
    pub row: u32,
    #[schemars(range(min = 1))]
    pub col: u32,
    pub enabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(length(min = 1))]
    pub student_key: Option<String>,
    #[serde(default)]
    pub locked: bool,
}

/// Independent validator result for hard constraints in the current state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EditorHardConstraintState {
    pub satisfied: bool,
    pub checked_rule_count: u64,
    pub violation_count: u64,
    #[serde(default)]
    pub violations: Vec<String>,
}

/// Versioned editor state DTO, including the independent hard-constraint
/// validation result required by the formal state schema.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EditorState {
    pub kind: EditorStateKind,
    pub protocol_version: EditorProtocolVersion,
    #[schemars(length(min = 1, max = 128))]
    pub draft_id: String,
    pub revision: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(length(min = 1, max = 128))]
    pub candidate_id: Option<String>,
    pub undo_depth: u64,
    pub redo_depth: u64,
    pub students: Vec<EditorStudentState>,
    pub seats: Vec<EditorSeatState>,
    pub hard_constraints: EditorHardConstraintState,
}

impl EditorState {
    /// Validate identifier and positive-coordinate constraints represented in
    /// the formal JSON Schema.
    pub fn validate(&self) -> Result<(), String> {
        validate_protocol_identifier("draft_id", &self.draft_id)?;
        if let Some(candidate_id) = &self.candidate_id {
            validate_protocol_identifier("candidate_id", candidate_id)?;
        }
        for student in &self.students {
            validate_reference("student_key", &student.student_key)?;
            if let Some(seat_id) = &student.seat_id {
                validate_reference("student seat_id", seat_id)?;
            }
        }
        for seat in &self.seats {
            validate_reference("seat_id", &seat.seat_id)?;
            if seat.row == 0 || seat.col == 0 {
                return Err("editor seat row and col must be at least 1".to_string());
            }
            if let Some(student_key) = &seat.student_key {
                validate_reference("seat student_key", student_key)?;
            }
        }
        Ok(())
    }
}

fn validate_protocol_identifier(field: &str, value: &str) -> Result<(), String> {
    validate_reference(field, value)?;
    if value.chars().count() > MAX_PROTOCOL_IDENTIFIER_LENGTH {
        return Err(format!(
            "{field} must contain at most {MAX_PROTOCOL_IDENTIFIER_LENGTH} characters"
        ));
    }
    Ok(())
}

fn validate_reference(field: &str, value: &str) -> Result<(), String> {
    if value.trim().is_empty() {
        return Err(format!("{field} must not be empty"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn student_seat(student: &str, seat: &str) -> StudentSeatPayload {
        StudentSeatPayload {
            student_key: student.to_string(),
            seat_id: seat.to_string(),
        }
    }

    fn all_operations() -> Vec<EditingOperation> {
        vec![
            EditingOperation::SwapStudents(SwapStudentsPayload {
                first_student: "s1".to_string(),
                second_student: "s2".to_string(),
            }),
            EditingOperation::MoveStudent(student_seat("s1", "A1")),
            EditingOperation::BatchMove(BatchMovePayload {
                moves: vec![
                    BatchMoveItem {
                        student_key: "s1".to_string(),
                        seat_id: "A2".to_string(),
                    },
                    BatchMoveItem {
                        student_key: "s2".to_string(),
                        seat_id: "A1".to_string(),
                    },
                ],
            }),
            EditingOperation::SeatStudent(student_seat("s3", "B1")),
            EditingOperation::UnseatStudent(StudentPayload {
                student_key: "s3".to_string(),
            }),
            EditingOperation::LockStudent(StudentPayload {
                student_key: "s1".to_string(),
            }),
            EditingOperation::UnlockStudent(StudentPayload {
                student_key: "s1".to_string(),
            }),
            EditingOperation::LockSeat(SeatPayload {
                seat_id: "A1".to_string(),
            }),
            EditingOperation::UnlockSeat(SeatPayload {
                seat_id: "A1".to_string(),
            }),
        ]
    }

    #[test]
    fn command_round_trip_covers_all_nine_operation_shapes() {
        let command = EditorCommand {
            kind: EditorCommandKind::SeatTrellisEditorCommand,
            protocol_version: EditorProtocolVersion::V1,
            command_id: "cmd-1".to_string(),
            draft_id: "draft-1".to_string(),
            base_revision: 7,
            action: EditorAction::Apply,
            operations: all_operations(),
        };
        command.validate().unwrap();

        let encoded = serde_json::to_string(&command).unwrap();
        let decoded: EditorCommand = serde_json::from_str(&encoded).unwrap();
        assert_eq!(decoded, command);
        assert!(encoded.contains(r#""kind":"batch_move","payload""#));
    }

    #[test]
    fn state_round_trip_preserves_all_formal_fields() {
        let state = EditorState {
            kind: EditorStateKind::SeatTrellisEditorState,
            protocol_version: EditorProtocolVersion::V1,
            draft_id: "draft-1".to_string(),
            revision: 9,
            candidate_id: Some("candidate-1".to_string()),
            undo_depth: 3,
            redo_depth: 1,
            students: vec![EditorStudentState {
                student_key: "s1".to_string(),
                display_name: "张三".to_string(),
                seat_id: Some("A1".to_string()),
                locked: true,
            }],
            seats: vec![EditorSeatState {
                seat_id: "A1".to_string(),
                row: 1,
                col: 1,
                enabled: true,
                student_key: Some("s1".to_string()),
                locked: true,
            }],
            hard_constraints: EditorHardConstraintState {
                satisfied: false,
                checked_rule_count: 2,
                violation_count: 1,
                violations: vec!["student s1 violates front-row rule".to_string()],
            },
        };
        state.validate().unwrap();

        let encoded = serde_json::to_string(&state).unwrap();
        let decoded: EditorState = serde_json::from_str(&encoded).unwrap();
        assert_eq!(decoded, state);
    }

    #[test]
    fn unknown_fields_are_rejected_at_every_protocol_layer() {
        let top_level = r#"{
            "kind":"seattrellis_editor_command",
            "protocol_version":"1.0",
            "command_id":"cmd-1",
            "draft_id":"draft-1",
            "base_revision":0,
            "action":"apply",
            "operations":[{"kind":"lock_seat","payload":{"seat_id":"A1"}}],
            "unexpected":true
        }"#;
        assert!(serde_json::from_str::<EditorCommand>(top_level).is_err());

        let payload = r#"{
            "kind":"seattrellis_editor_command",
            "protocol_version":"1.0",
            "command_id":"cmd-1",
            "draft_id":"draft-1",
            "base_revision":0,
            "action":"apply",
            "operations":[{"kind":"lock_seat","payload":{"seat_id":"A1","unexpected":true}}]
        }"#;
        assert!(serde_json::from_str::<EditorCommand>(payload).is_err());

        let state = r#"{
            "kind":"seattrellis_editor_state",
            "protocol_version":"1.0",
            "draft_id":"draft-1",
            "revision":0,
            "undo_depth":0,
            "redo_depth":0,
            "students":[],
            "seats":[],
            "hard_constraints":{"satisfied":true,"checked_rule_count":0,"violation_count":0,"unexpected":true}
        }"#;
        assert!(serde_json::from_str::<EditorState>(state).is_err());
    }

    #[test]
    fn action_policy_requires_apply_operations_and_empty_undo_redo() {
        let mut command = EditorCommand {
            kind: EditorCommandKind::SeatTrellisEditorCommand,
            protocol_version: EditorProtocolVersion::V1,
            command_id: "cmd-1".to_string(),
            draft_id: "draft-1".to_string(),
            base_revision: 0,
            action: EditorAction::Apply,
            operations: Vec::new(),
        };
        assert!(command.validate().unwrap_err().contains("at least one"));

        command.action = EditorAction::Undo;
        assert!(command.validate().is_ok());
        command
            .operations
            .push(EditingOperation::LockSeat(SeatPayload {
                seat_id: "A1".to_string(),
            }));
        assert!(command.validate().unwrap_err().contains("must not contain"));

        command.action = EditorAction::Redo;
        assert!(command.validate().unwrap_err().contains("must not contain"));
    }

    #[test]
    fn batch_moves_enforce_uniqueness_and_expanded_command_budget() {
        let duplicate = BatchMovePayload {
            moves: vec![
                BatchMoveItem {
                    student_key: "s1".to_string(),
                    seat_id: "A1".to_string(),
                },
                BatchMoveItem {
                    student_key: "s1".to_string(),
                    seat_id: "A2".to_string(),
                },
            ],
        };
        assert!(duplicate.validate().unwrap_err().contains("students"));

        let moves = |prefix: &str| BatchMovePayload {
            moves: (0..60)
                .map(|index| BatchMoveItem {
                    student_key: format!("{prefix}-student-{index}"),
                    seat_id: format!("{prefix}-seat-{index}"),
                })
                .collect(),
        };
        let command = EditorCommand {
            kind: EditorCommandKind::SeatTrellisEditorCommand,
            protocol_version: EditorProtocolVersion::V1,
            command_id: "cmd-expanded".to_string(),
            draft_id: "draft-1".to_string(),
            base_revision: 0,
            action: EditorAction::Apply,
            operations: vec![
                EditingOperation::BatchMove(moves("first")),
                EditingOperation::BatchMove(moves("second")),
            ],
        };
        assert!(command.validate().unwrap_err().contains("expanded"));
    }
}
