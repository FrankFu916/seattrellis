//! Fuzz-style editor-command bombardment (plan §11.4 editor command sequence
//! target): arbitrary bytes parsed as `EditorCommandEnvelope` must never
//! panic (the parser is strict and returns errors for malformed input).

use proptest::prelude::*;
use seattrellis_domain::editing::{
    apply_command, EditorCommandEnvelope, EditorDraft, EditorSeatSpec,
};

fn draft() -> EditorDraft {
    let keys = ["s0", "s1", "s2"];
    let seat_ids: Vec<String> = (0..3).map(|i| format!("A{}", i + 1)).collect();
    let seats: Vec<EditorSeatSpec> = (0..3)
        .map(|i| EditorSeatSpec {
            seat_id: seat_ids[i].clone(),
            row: 1,
            col: i as i32 + 1,
            enabled: true,
        })
        .collect();
    let assignment: Vec<(&str, &str)> = keys
        .iter()
        .cloned()
        .zip(seat_ids.iter().map(String::as_str))
        .collect();
    EditorDraft::new("draft-f", None, &keys, seats, &assignment, None).expect("draft builds")
}

fn random_document(bytes: Vec<u8>) -> String {
    String::from_utf8_lossy(&bytes).into_owned()
}

proptest! {
    #![proptest_config(proptest::prelude::ProptestConfig::with_cases(256))]

    #[test]
    fn editor_envelope_parser_never_panics(bytes in prop::collection::vec(any::<u8>(), 0..2048)) {
        // Malformed envelopes must yield Err, never panic.
        let parsed = serde_json::from_str::<EditorCommandEnvelope>(&random_document(bytes));
        if let Ok(command) = parsed {
            // Even valid envelopes must not panic when applied to a draft
            // (unknown draft ids, stale revisions, bad actions are errors).
            let mut draft = draft();
            let _ = apply_command(&mut draft, &command);
        }
    }

    #[test]
    fn editor_operation_payload_never_panics(bytes in prop::collection::vec(any::<u8>(), 0..1024)) {
        let document = random_document(bytes);
        // Wrap random bytes as an apply command with one operation whose
        // payload is the random text; applying must not panic.
        let mut draft = draft();
        let command: EditorCommandEnvelope = serde_json::from_value(serde_json::json!({
            "kind": "seattrellis_editor_command",
            "protocol_version": "1.0",
            "command_id": "fuzz-cmd",
            "draft_id": "draft-f",
            "base_revision": draft.revision(),
            "action": "apply",
            "operations": [{"kind": "seat_student", "payload": {"student_key": document, "seat_id": "A1"}}]
        })).expect("envelope builds");
        let _ = apply_command(&mut draft, &command);
    }
}
