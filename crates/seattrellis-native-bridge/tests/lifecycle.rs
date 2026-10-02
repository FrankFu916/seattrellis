//! Exercise the exported ABI, including ownership and independent sessions.
use base64::Engine;
use seattrellis_bridge::*;
use serde_json::{json, Value};
use std::sync::Mutex;

static TEST_LOCK: Mutex<()> = Mutex::new(());

struct Session(u64);
impl Session {
    fn new() -> Self {
        let id = seattrellis_session_create();
        assert_ne!(id, 0);
        Self(id)
    }
    fn call(&self, operation: &str, payload: Value) -> Value {
        call(
            self.0,
            json!({"protocol_version":1,"operation":operation,"payload":payload}),
        )
    }
}
impl Drop for Session {
    fn drop(&mut self) {
        seattrellis_session_destroy(self.0);
    }
}

struct Owned(SeattrellisBuffer);
impl Drop for Owned {
    fn drop(&mut self) {
        seattrellis_buffer_free(self.0);
    }
}

fn decode(buffer: SeattrellisBuffer) -> Value {
    let owned = Owned(buffer);
    assert!(!owned.0.data.is_null());
    assert!(owned.0.len <= MAX_OUTPUT_BYTES);
    // SAFETY: the ABI returned this live allocation, and Owned frees it only
    // after parsing; there are no concurrent readers or frees.
    let bytes = unsafe { std::slice::from_raw_parts(owned.0.data, owned.0.len) };
    serde_json::from_slice(bytes).unwrap()
}

fn call(id: u64, input: Value) -> Value {
    let bytes = serde_json::to_vec(&input).unwrap();
    // SAFETY: immutable Vec allocation remains live for the whole call.
    decode(unsafe { seattrellis_session_dispatch(id, bytes.as_ptr(), bytes.len()) })
}

fn source() -> Value {
    json!({
        "api_version":2,"student_count":2,
        "seat_positions":[[0.0,0.0],[1.0,0.0],[2.0,0.0]],
        "fixed_seats":[[0,0]],"seed":42,"time_budget_ms":1000,
        "students":[{"key":"S1","display_name":"张同学"},{"key":"S2","display_name":"李同学"}],
        "layout":{"name":"原生教室","seats":[
            {"seat_id":"A1","row":1,"col":1,"enabled":true},
            {"seat_id":"A2","row":1,"col":2,"enabled":true},
            {"seat_id":"A3","row":1,"col":3,"enabled":true}
        ]}
    })
}

fn command(id: &str, revision: u64, command_id: &str, operations: Value) -> Value {
    json!({"kind":"seattrellis_editor_command","protocol_version":"1.0", "command_id":command_id,
        "draft_id":id,"base_revision":revision,"action":"apply","operations":operations})
}

#[test]
fn edited_document_reopens_repairs_audits_and_exports_through_abi() {
    let _serial = TEST_LOCK.lock().unwrap();
    assert_eq!(seattrellis_abi_version(), 1);
    let session = Session::new();
    let isolated = Session::new();
    let generated = session.call("generate", source());
    assert_eq!(generated["ok"], true, "{generated}");
    assert_eq!(generated["result"]["status"], "Solved");
    let id = generated["result"]["editor"]["draft_id"].as_str().unwrap();
    assert_eq!(
        isolated.call("state", json!({"draft_id":id}))["error"]["status"],
        404
    );
    let locked = session.call(
        "command",
        command(
            id,
            0,
            "lock-1",
            json!([
                {"kind":"lock_student","payload":{"student_key":"S2"}}
            ]),
        ),
    );
    assert_eq!(locked["result"]["revision"], 1, "{locked}");
    let moved = session.call(
        "command",
        command(
            id,
            1,
            "move-1",
            json!([
                {"kind":"move_student","payload":{"student_key":"S1","seat_id":"A3"}}
            ]),
        ),
    );
    assert_eq!(moved["result"]["revision"], 2, "{moved}");
    assert_eq!(moved["result"]["validation"]["valid"], false);
    assert_eq!(
        session.call(
            "command",
            command(
                id,
                1,
                "stale",
                json!([
                    {"kind":"unlock_student","payload":{"student_key":"S2"}}
                ])
            )
        )["error"]["status"],
        409
    );
    let saved = session.call(
        "serialize",
        json!({
            "class_source":{"name":"完整班级","private_notes":"只存于本地","solve":source()},
            "draft_refs":[{"draft_id":id,"revision":2}]
        }),
    );
    assert_eq!(saved["ok"], true, "{saved}");
    assert_eq!(
        saved["result"]["drafts"][0]["lock_state"]["locked_students"],
        json!(["S2"])
    );
    let document = saved["result"].clone();
    let old_handle = session.0;
    drop(session);
    assert_eq!(
        call(
            old_handle,
            json!({"protocol_version":1,"operation":"state","payload":{"draft_id":id}})
        )["error"]["code"],
        "invalid_session"
    );
    let reopened = Session::new();
    assert_ne!(reopened.0, old_handle);
    let opened = reopened.call("open", document);
    assert_eq!(opened["ok"], true, "{opened}");
    assert_eq!(
        opened["result"]["class_source"]["private_notes"],
        "只存于本地"
    );
    let state = &opened["result"]["editor"];
    let new_id = state["draft_id"].as_str().unwrap();
    assert_ne!(new_id, id);
    let s2_before = state["students"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["student_key"] == "S2")
        .unwrap();
    assert_eq!(s2_before["locked"], true);
    assert_eq!(
        reopened.call("audit", json!({"draft_id":new_id}))["result"]["feasible"],
        false
    );
    assert_eq!(
        reopened.call(
            "export",
            json!({"draft_id":new_id,"format":"svg","options":{}})
        )["error"]["status"],
        422
    );
    let repaired = reopened.call(
        "repair",
        json!({"draft_id":new_id,"base_revision":0,"affected_students":["S1"]}),
    );
    assert_eq!(repaired["ok"], true, "{repaired}");
    assert_eq!(repaired["result"]["revision"], 1);
    let repaired_students = repaired["result"]["students"].as_array().unwrap();
    let s1 = repaired_students
        .iter()
        .find(|s| s["student_key"] == "S1")
        .unwrap();
    let s2 = repaired_students
        .iter()
        .find(|s| s["student_key"] == "S2")
        .unwrap();
    assert_eq!(s1["seat_id"], "A1");
    assert_eq!(s2["seat_id"], s2_before["seat_id"]);
    assert_eq!(s2["locked"], true);
    assert_eq!(
        reopened.call("audit", json!({"draft_id":new_id}))["result"]["feasible"],
        true
    );
    let exported = reopened.call(
        "export",
        json!({"draft_id":new_id,"format":"svg","options":{
            "expected_revision":1,"privacy":{"anonymize":true},"template":"public"
        }}),
    );
    assert_eq!(exported["ok"], true, "{exported}");
    assert_eq!(exported["result"]["filename"], "seat-plan.svg");
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(exported["result"]["base64"].as_str().unwrap())
        .unwrap();
    let svg = String::from_utf8(bytes).unwrap();
    assert!(svg.contains("<svg"));
    assert!(!svg.contains("张同学"));
    assert!(!svg.contains("李同学"));
    assert_eq!(
        reopened.call("delete", json!({"draft_id":new_id}))["result"]["deleted"],
        true
    );
    assert_eq!(
        reopened.call("state", json!({"draft_id":new_id}))["error"]["status"],
        404
    );
}

#[test]
fn invalid_envelopes_and_oversized_foreign_inputs_are_rejected() {
    let _serial = TEST_LOCK.lock().unwrap();
    let session = Session::new();
    for input in [b"not JSON".as_slice(), b"\xff", b"{}"] {
        // SAFETY: each static initialized byte slice is live and immutable.
        let result =
            decode(unsafe { seattrellis_session_dispatch(session.0, input.as_ptr(), input.len()) });
        assert_eq!(result["error"]["code"], "invalid_json");
    }
    // Null/oversize are rejected before constructing a foreign-memory slice.
    let empty = decode(unsafe { seattrellis_session_dispatch(session.0, std::ptr::null(), 0) });
    assert_eq!(empty["error"]["code"], "invalid_buffer");
    let huge = decode(unsafe {
        seattrellis_session_dispatch(session.0, std::ptr::null(), MAX_INPUT_BYTES + 1)
    });
    assert_eq!(huge["error"]["code"], "input_too_large");
    assert_eq!(
        call(
            session.0,
            json!({"protocol_version":2,"operation":"state","payload":{}})
        )["error"]["code"],
        "protocol_mismatch"
    );
    assert_eq!(
        session.call("unknown", json!({}))["error"]["code"],
        "unknown_operation"
    );
    assert_eq!(seattrellis_session_cancel(0), -1);
    assert_eq!(seattrellis_session_cancel(session.0), 0);
    assert_eq!(
        session.call("generate", source())["result"]["status"],
        "Solved",
        "idle cancellation must not poison a later call"
    );
}

#[test]
fn session_and_owned_response_quotas_are_enforced_and_recover_after_release() {
    let _serial = TEST_LOCK.lock().unwrap();
    let sessions: Vec<_> = (0..MAX_SESSIONS).map(|_| Session::new()).collect();
    assert_eq!(seattrellis_session_create(), 0);
    let stale = sessions[0].0;
    let mut sessions = sessions;
    sessions.remove(0);
    let replacement = Session::new();
    assert_ne!(replacement.0, stale);
    seattrellis_session_destroy(stale);
    assert_eq!(
        seattrellis_session_cancel(replacement.0),
        0,
        "old handle cannot affect replacement"
    );
    let generated = replacement.call("generate", source());
    let draft_id = generated["result"]["editor"]["draft_id"].as_str().unwrap();
    let input = br#"{"protocol_version":1,"operation":"unknown","payload":{}}"#;
    let owned: Vec<_> = (0..MAX_OUTSTANDING_BUFFERS)
        .map(|_| {
            // SAFETY: static allocation is initialized, readable and immutable.
            let buffer =
                unsafe { seattrellis_session_dispatch(replacement.0, input.as_ptr(), input.len()) };
            assert!(!buffer.data.is_null());
            Owned(buffer)
        })
        .collect();
    let full = unsafe { seattrellis_session_dispatch(replacement.0, input.as_ptr(), input.len()) };
    assert!(full.data.is_null());
    assert_eq!(full.len, 0);
    // An incorrect length must not free the legitimate still-live allocation.
    seattrellis_buffer_free(SeattrellisBuffer {
        data: owned[0].0.data,
        len: owned[0].0.len + 1,
    });
    let full_again =
        unsafe { seattrellis_session_dispatch(replacement.0, input.as_ptr(), input.len()) };
    assert!(full_again.data.is_null());
    let blocked_command = serde_json::to_vec(&json!({"protocol_version":1,"operation":"command","payload":command(
        draft_id, 0, "quota-blocked", json!([{"kind":"lock_student","payload":{"student_key":"S2"}}])
    )})).unwrap();
    let blocked = unsafe {
        seattrellis_session_dispatch(
            replacement.0,
            blocked_command.as_ptr(),
            blocked_command.len(),
        )
    };
    assert!(
        blocked.data.is_null(),
        "quota rejection must happen before command mutation"
    );
    drop(owned);
    let state = replacement.call("state", json!({"draft_id":draft_id}));
    assert_eq!(state["result"]["revision"], 0);
    assert!(state["result"]["students"]
        .as_array()
        .unwrap()
        .iter()
        .all(|student| student["locked"] == false));
    assert_eq!(
        replacement.call("unknown", json!({}))["error"]["code"],
        "unknown_operation"
    );
    seattrellis_buffer_free(SeattrellisBuffer {
        data: std::ptr::null_mut(),
        len: 0,
    });
}
