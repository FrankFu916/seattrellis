//! Property-based editing gates (plan §11.3 Editing).
//!
//! For any sequence of randomized editor commands:
//! 1. undo(apply(x)) restores the exact pre-apply state (fingerprint equal);
//! 2. redo(undo(x)) restores the post-apply state;
//! 3. stale-revision commands are always rejected and never mutate state;
//! 4. a failed batch never leaves partial state (fingerprint unchanged);
//! 5. revision is monotonic across successful commands.

use proptest::prelude::*;
use seattrellis_domain::editing::{
    apply_command, EditorCommandEnvelope, EditorDraft, EditorOperation, EditorSeatSpec,
};

fn draft_with(n: usize) -> EditorDraft {
    let students: Vec<String> = (0..n).map(|i| format!("s{i}")).collect();
    let keys: Vec<&str> = students.iter().map(String::as_str).collect();
    let seats: Vec<EditorSeatSpec> = (0..n)
        .map(|i| EditorSeatSpec {
            seat_id: format!("A{}", i + 1),
            row: (i / 3) as i32 + 1,
            col: (i % 3) as i32 + 1,
            enabled: true,
        })
        .collect();
    let seat_ids: Vec<String> = (0..n).map(|i| format!("A{}", i + 1)).collect();
    let assignment: Vec<(&str, &str)> = keys
        .iter()
        .cloned()
        .zip(seat_ids.iter().map(String::as_str))
        .collect();
    EditorDraft::new("draft-p", None, &keys, seats, &assignment, None).expect("draft builds")
}

/// Full state fingerprint: students + seats, order-insensitive.
fn fingerprint(draft: &EditorDraft) -> String {
    let mut students: Vec<(String, Option<String>, bool)> = draft
        .students()
        .iter()
        .map(|s| (s.student_key.clone(), s.seat_id.clone(), s.locked))
        .collect();
    students.sort();
    let mut seats: Vec<(String, Option<String>, bool)> = draft
        .seats()
        .iter()
        .map(|s| (s.seat_id.clone(), s.student_key.clone(), s.locked))
        .collect();
    seats.sort();
    format!("{students:?}|{seats:?}")
}

fn command(
    draft_id: &str,
    command_id: &str,
    base_revision: u64,
    action: &str,
    operations: Vec<EditorOperation>,
) -> EditorCommandEnvelope {
    EditorCommandEnvelope {
        kind: "seattrellis_editor_command".to_string(),
        protocol_version: "1.0".to_string(),
        command_id: command_id.to_string(),
        draft_id: draft_id.to_string(),
        base_revision,
        action: action.to_string(),
        operations,
    }
}

fn op(kind: &str, payload: serde_json::Value) -> EditorOperation {
    EditorOperation {
        kind: kind.to_string(),
        payload: payload
            .as_object()
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .collect(),
    }
}

proptest! {
    #![proptest_config(proptest::prelude::ProptestConfig::with_cases(48))]

    #[test]
    fn undo_restores_and_redo_reapplies_arbitrary_operations(
        n in 3usize..=8,
        op_index in 0usize..6,
        a in 0usize..8,
        b in 0usize..8,
    ) {
        prop_assume!(a < n && b < n);
        let mut draft = draft_with(n);
        let operation = match op_index {
            0 => op("swap_students", serde_json::json!({"first_student": format!("s{a}"), "second_student": format!("s{b}")})),
            1 => op("move_student", serde_json::json!({"student_key": format!("s{a}"), "seat_id": format!("A{}", b + 1)})),
            2 => op("seat_student", serde_json::json!({"student_key": format!("s{a}"), "seat_id": format!("A{}", b + 1)})),
            3 => op("unseat_student", serde_json::json!({"student_key": format!("s{a}")})),
            4 => op("lock_student", serde_json::json!({"student_key": format!("s{a}")})),
            _ => op("lock_seat", serde_json::json!({"seat_id": format!("A{}", a + 1)})),
        };
        let before = fingerprint(&draft);
        let rev0 = draft.revision();
        // apply (may legitimately fail for invalid ops like swapping an
        // unseated student - those cases are covered by the partial-state
        // property; here we only exercise the success path)
        if apply_command(&mut draft, &command("draft-p", "cmd-apply", rev0, "apply", vec![operation])).is_ok() {
            let after = fingerprint(&draft);
            let rev1 = draft.revision();
            // undo restores
            apply_command(&mut draft, &command("draft-p", "cmd-undo", rev1, "undo", vec![])).expect("undo ok");
            prop_assert!(fingerprint(&draft) == before, "undo(apply(x)) must restore state");
            let rev2 = draft.revision();
            // redo restores
            apply_command(&mut draft, &command("draft-p", "cmd-redo", rev2, "redo", vec![])).expect("redo ok");
            prop_assert!(fingerprint(&draft) == after, "redo(undo(x)) must restore applied state");
        } else {
            // failed apply must not mutate state either
            prop_assert!(fingerprint(&draft) == before, "failed apply must not mutate");
        }
    }

    #[test]
    fn stale_revision_is_always_rejected_without_mutation(
        n in 3usize..=8,
        stale_by in 1u64..5,
    ) {
        let mut draft = draft_with(n);
        let current = draft.revision();
        let stale = current + stale_by;
        let before = fingerprint(&draft);
        let error = apply_command(
            &mut draft,
            &command(
                "draft-p",
                "cmd-stale",
                stale,
                "apply",
                vec![op("lock_student", serde_json::json!({"student_key": "s0"}))],
            ),
        )
        .expect_err("stale revision must be rejected");
        prop_assert!(error.contains("stale") || error.contains("revision"), "{error}");
        prop_assert!(fingerprint(&draft) == before, "stale command must not mutate");
    }

    #[test]
    fn failed_batch_leaves_no_partial_state(
        n in 3usize..=8,
    ) {
        // A batch whose second operation is invalid (unknown student): the
        // whole command must fail and leave the draft untouched.
        let mut draft = draft_with(n);
        let before = fingerprint(&draft);
        let current = draft.revision();
        let error = apply_command(
            &mut draft,
            &command(
                "draft-p",
                "cmd-bad-batch",
                current,
                "apply",
                vec![
                    op("lock_student", serde_json::json!({"student_key": "s0"})),
                    op("seat_student", serde_json::json!({"student_key": "ghost", "seat_id": "A1"})),
                ],
            ),
        )
        .expect_err("invalid batch must fail");
        prop_assert!(!error.is_empty());
        prop_assert!(fingerprint(&draft) == before, "failed batch must not leave partial state");
    }

    #[test]
    fn revision_is_monotonic_across_successful_commands(
        n in 3usize..=8,
        steps in 1usize..12,
    ) {
        let mut draft = draft_with(n);
        let mut last = draft.revision();
        for step in 0..steps {
            let r = draft.revision();
            prop_assert!(r >= last, "revision must be monotonic");
            last = r;
            let ok = apply_command(
                &mut draft,
                &command(
                    "draft-p",
                    &format!("cmd-{step}"),
                    r,
                    "apply",
                    vec![op("lock_student", serde_json::json!({"student_key": format!("s{}", step % n)}))],
                ),
            )
            .is_ok();
            if ok {
                prop_assert!(draft.revision() > r, "successful command must bump revision");
            }
        }
    }
}
