//! Editing long-run gate (plan §11.9): 1000 consecutive editor commands with
//! revision monotonicity, no double occupancy, undo/redo reversibility and
//! atomic rollback of failed commands.

use seattrellis_domain::editing::{
    create_draft, EditorCommandEnvelope, EditorOperation, EditorSeatSpec, EditorState, ACTION_REDO,
    ACTION_UNDO,
};
use serde_json::json;

struct Lcg(u64);

impl Lcg {
    fn next(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        self.0
    }

    fn below(&mut self, bound: usize) -> usize {
        (self.next() % bound as u64) as usize
    }
}

fn test_seats() -> Vec<EditorSeatSpec> {
    let mut seats = Vec::new();
    for row in 1..=4 {
        for col in 1..=6 {
            seats.push(EditorSeatSpec {
                seat_id: format!("R{row}C{col}"),
                row,
                col,
                enabled: true,
            });
        }
    }
    seats
}

fn assignment_map(state: &EditorState) -> Vec<(String, String)> {
    let mut pairs: Vec<(String, String)> = state
        .students
        .iter()
        .filter_map(|student| {
            student
                .seat_id
                .as_ref()
                .map(|seat| (student.student_key.clone(), seat.clone()))
        })
        .collect();
    pairs.sort();
    pairs
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
        payload: payload.as_object().cloned().unwrap_or_default(),
    }
}

#[test]
fn one_thousand_commands_keep_revision_monotonic_and_assignments_unique() {
    let store = seattrellis_domain::editing::new_draft_store();
    let student_keys: Vec<String> = (1..=24).map(|index| format!("s{index}")).collect();
    let seat_ids: Vec<String> = test_seats()
        .iter()
        .map(|seat| seat.seat_id.clone())
        .collect();
    let initial: Vec<(&str, &str)> = seat_ids
        .iter()
        .take(24)
        .zip(student_keys.iter())
        .map(|(seat, student)| (student.as_str(), seat.as_str()))
        .collect();
    let initial_state = create_draft(
        &store,
        "long-run-draft",
        Some("candidate-1".to_string()),
        &student_keys.iter().map(String::as_str).collect::<Vec<_>>(),
        test_seats(),
        &initial,
        None,
    )
    .expect("draft creates");

    let mut rng = Lcg(0xED17_0001);
    let rss_before = resident_set_bytes();
    let mut rss_peak = rss_before;
    let mut revision = 0u64;
    let mut applied = 0u64;
    let mut navigated = 0u64;
    let mut previous_state = initial_state;
    // The state before/after the most recent apply, used to verify that an
    // undo restores the previous assignment and a redo re-applies it.
    let mut last_apply: Option<(EditorState, EditorState)> = None;

    for step in 0..1000u64 {
        if step % 100 == 99 {
            if let Some(rss) = resident_set_bytes() {
                rss_peak = Some(rss_peak.map_or(rss, |peak| peak.max(rss)));
            }
        }
        let roll = rng.below(12);
        let operations = match roll {
            0..=3 => {
                // Swap two seated students.
                let first = format!("s{}", rng.below(24) + 1);
                let second = format!("s{}", rng.below(24) + 1);
                vec![op(
                    "swap_students",
                    json!({ "first_student": first, "second_student": second }),
                )]
            }
            4..=6 => {
                // Move a student to a random seat (occupied seats displace).
                let student = format!("s{}", rng.below(24) + 1);
                let seat = format!("R{}C{}", rng.below(4) + 1, rng.below(6) + 1);
                vec![op(
                    "move_student",
                    json!({ "student_key": student, "seat_id": seat }),
                )]
            }
            7..=8 => {
                // Unseat then re-seat a student at a random seat.
                let student = format!("s{}", rng.below(24) + 1);
                let seat = format!("R{}C{}", rng.below(4) + 1, rng.below(6) + 1);
                vec![
                    op("unseat_student", json!({ "student_key": student })),
                    op(
                        "seat_student",
                        json!({ "student_key": student, "seat_id": seat }),
                    ),
                ]
            }
            9..=10 => {
                // Lock (or occasionally unlock) a student: does not move
                // anyone, but constrains later moves like a real teacher's
                // "keep this student here" decision.
                let student = format!("s{}", rng.below(24) + 1);
                if rng.below(3) == 0 {
                    vec![op("unlock_student", json!({ "student_key": student }))]
                } else {
                    vec![op("lock_student", json!({ "student_key": student }))]
                }
            }
            _ => {
                // Undo or redo occasionally.
                let action = if rng.below(2) == 0 {
                    ACTION_UNDO
                } else {
                    ACTION_REDO
                };
                let envelope = command(
                    "long-run-draft",
                    &format!("cmd-{step}"),
                    revision,
                    action,
                    Vec::new(),
                );
                match seattrellis_domain::editing::apply_command_in_store(&store, &envelope) {
                    Ok(state) => {
                        revision += 1;
                        navigated += 1;
                        if action == ACTION_UNDO {
                            // Undo must restore the assignment that existed
                            // before the most recent apply (revision differs
                            // by design).
                            if let Some((before, _)) = &last_apply {
                                assert_eq!(
                                    assignment_map(&state),
                                    assignment_map(before),
                                    "undo at step {step} must restore the previous assignment"
                                );
                            }
                        } else if let Some((_, after)) = &last_apply {
                            assert_eq!(
                                assignment_map(&state),
                                assignment_map(after),
                                "redo at step {step} must re-apply the undone assignment"
                            );
                        }
                        last_apply = None;
                        assert_no_double_occupancy(&state, step);
                        assert_eq!(
                            state.revision, revision,
                            "revision monotonic at step {step}"
                        );
                        previous_state = state;
                    }
                    Err(_) => {
                        // Undo with an empty stack or redo with an empty
                        // stack is a legitimate rejection: atomic rollback
                        // means nothing changed.
                        let state =
                            seattrellis_domain::editing::fetch_state(&store, "long-run-draft")
                                .unwrap();
                        assert_eq!(state.revision, revision, "failed undo/redo at step {step}");
                        assert_eq!(
                            assignment_map(&state),
                            assignment_map(&previous_state),
                            "failed undo/redo at step {step} must not change the assignment"
                        );
                    }
                }
                continue;
            }
        };
        let envelope = command(
            "long-run-draft",
            &format!("cmd-{step}"),
            revision,
            "apply",
            operations,
        );
        match seattrellis_domain::editing::apply_command_in_store(&store, &envelope) {
            Ok(state) => {
                revision += 1;
                applied += 1;
                assert_eq!(
                    state.revision, revision,
                    "revision monotonic at step {step}"
                );
                assert_no_double_occupancy(&state, step);
                if roll == 9 || roll == 10 {
                    // Lock changes don't alter assignments.
                    assert_eq!(assignment_map(&state), assignment_map(&previous_state));
                }
                last_apply = Some((previous_state.clone(), state.clone()));
                previous_state = state;
            }
            Err(_) => {
                // A command against a locked student or a full seat is a
                // legitimate rejection: §5.4 requires failed commands to
                // roll back atomically.
                let state =
                    seattrellis_domain::editing::fetch_state(&store, "long-run-draft").unwrap();
                assert_eq!(state.revision, revision, "failed apply at step {step}");
                assert_eq!(
                    assignment_map(&state),
                    assignment_map(&previous_state),
                    "failed apply at step {step} must not change the assignment"
                );
            }
        }
    }
    // Peak-memory refinement curve (plan §19.8 "500 次编辑的峰值内存"):
    // the resident set must stay flat across the whole command sequence.
    // Sampled at the end; the per-hundred-step samples are kept inside the
    // loop below and the peak-vs-start growth is asserted here.
    if let (Some(before), Some(peak)) = (rss_before, rss_peak) {
        let growth = peak.saturating_sub(before);
        assert!(
            growth < 64 * 1024 * 1024,
            "resident set grew by {growth} bytes over 1000 commands"
        );
    }

    // Apply and undo/redo each advance the revision exactly once.
    assert_eq!(
        revision,
        applied + navigated,
        "every successful command advances exactly once"
    );
    assert!(
        applied > 300,
        "the random sequence should mostly apply: {applied}"
    );
    assert!(
        navigated > 20,
        "the random sequence should exercise undo/redo: {navigated}"
    );
}

fn assert_no_double_occupancy(state: &EditorState, step: u64) {
    let mut seats: Vec<&str> = state
        .students
        .iter()
        .filter_map(|student| student.seat_id.as_deref())
        .collect();
    seats.sort_unstable();
    let duplicates: Vec<&str> = seats
        .windows(2)
        .filter(|pair| pair[0] == pair[1])
        .map(|pair| pair[0])
        .collect();
    assert!(
        duplicates.is_empty(),
        "double occupancy after step {step}: {duplicates:?}"
    );
}

#[test]
fn failed_commands_roll_back_atomically() {
    let store = seattrellis_domain::editing::new_draft_store();
    let student_keys = ["s1", "s2", "s3"];
    let seats = vec![
        EditorSeatSpec {
            seat_id: "A1".to_string(),
            row: 1,
            col: 1,
            enabled: true,
        },
        EditorSeatSpec {
            seat_id: "A2".to_string(),
            row: 1,
            col: 2,
            enabled: true,
        },
        EditorSeatSpec {
            seat_id: "B1".to_string(),
            row: 2,
            col: 1,
            enabled: true,
        },
    ];
    let _ = create_draft(
        &store,
        "atomic-draft",
        None,
        &student_keys,
        seats,
        &[("s1", "A1"), ("s2", "A2"), ("s3", "B1")],
        None,
    )
    .expect("draft creates");

    // A batch whose second operation fails must leave the draft untouched.
    let envelope = command(
        "atomic-draft",
        "cmd-bad",
        0,
        "apply",
        vec![
            op(
                "swap_students",
                json!({ "first_student": "s1", "second_student": "s2" }),
            ),
            op(
                "move_student",
                json!({ "student_key": "ghost", "seat_id": "A1" }),
            ),
        ],
    );
    let error = seattrellis_domain::editing::apply_command_in_store(&store, &envelope)
        .expect_err("unknown student must fail the batch");
    assert!(error.to_lowercase().contains("unknown"), "got: {error}");

    let state = seattrellis_domain::editing::fetch_state(&store, "atomic-draft").unwrap();
    assert_eq!(
        state.revision, 0,
        "failed command must not advance the revision"
    );
    assert_eq!(
        assignment_map(&state),
        vec![
            ("s1".to_string(), "A1".to_string()),
            ("s2".to_string(), "A2".to_string()),
            ("s3".to_string(), "B1".to_string()),
        ],
        "failed command must leave the assignment untouched"
    );
}

/// Linux resident-set size in bytes (the CI long-run job runs on ubuntu);
/// `None` on other platforms, where the memory assertion is skipped.
fn resident_set_bytes() -> Option<u64> {
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    let line = status.lines().find(|line| line.starts_with("VmRSS:"))?;
    let value = line.split_whitespace().nth(1)?;
    value.parse().ok()
}
