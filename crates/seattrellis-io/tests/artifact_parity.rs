use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use seattrellis_io::projects::{compare_artifacts_json, restore_artifact_json};
use serde_json::{json, Value};

const EXPECTED: &str = include_str!("../../../fixtures/artifact-parity/expected.json");
const PROJECT: &str = include_str!("../../../fixtures/artifact-parity/project.json");
const LEFT: &str = include_str!("../../../fixtures/artifact-parity/history/snapshot-left.json");
const RIGHT: &str = include_str!("../../../fixtures/artifact-parity/outputs/snapshot-right.json");
const CANDIDATES: &str = include_str!("../../../fixtures/artifact-parity/outputs/candidates.json");

static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);

struct FixtureWorkspace(PathBuf);

impl FixtureWorkspace {
    fn new() -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(0);
        let sequence = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "seattrellis-artifact-parity-{}-{nonce}-{sequence}",
            std::process::id()
        ));
        fs::create_dir_all(root.join("history")).unwrap();
        fs::create_dir_all(root.join("outputs")).unwrap();
        fs::write(root.join("project.json"), PROJECT).unwrap();
        fs::write(root.join("history/snapshot-left.json"), LEFT).unwrap();
        fs::write(root.join("outputs/snapshot-right.json"), RIGHT).unwrap();
        fs::write(root.join("outputs/candidates.json"), CANDIDATES).unwrap();
        Self(root)
    }
}

impl Drop for FixtureWorkspace {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn is_rfc3339(value: &str) -> bool {
    value.contains('T') && (value.ends_with('Z') || value.ends_with("+00:00"))
}

fn restore_summary(path: &Path) -> Value {
    let document: Value = serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap();
    let restored_at = document["metadata"]["restored_at"].as_str().unwrap();
    assert!(is_rfc3339(restored_at), "{restored_at}");
    json!({
        "schema_version": document.get("schema_version").cloned().unwrap_or(Value::Null),
        "kind": document.get("kind").cloned().unwrap_or(Value::Null),
        "restored_from": document["metadata"].get("restored_from").cloned().unwrap_or(Value::Null),
        "restored_at": "<RFC3339>",
        "student_keys": document["students"].as_array().unwrap().iter().map(|student| {
            student.get("student_id").filter(|value| !value.is_null())
                .or_else(|| student.get("name"))
                .cloned().unwrap_or(Value::Null)
        }).collect::<Vec<_>>(),
        "assignments": document["assignments"].as_array().unwrap().iter().map(|assignment| json!({
            "student_key": assignment.get("student_key").cloned().unwrap_or(Value::Null),
            "seat_id": assignment.get("seat_id").cloned().unwrap_or(Value::Null),
        })).collect::<Vec<_>>(),
        "has_candidate_envelope": document.get("candidates").is_some(),
    })
}

#[test]
fn artifact_compare_and_restore_match_python_oracle_golden() {
    let fixture = FixtureWorkspace::new();
    let project = fixture.0.join("project.json");
    let left = fixture.0.join("history/snapshot-left.json");
    let right = fixture.0.join("outputs/snapshot-right.json");
    let candidates = fixture.0.join("outputs/candidates.json");
    let expected: Value = serde_json::from_str(EXPECTED).unwrap();

    let mut compare: Value = serde_json::from_str(
        &compare_artifacts_json(
            project.to_str().unwrap(),
            left.to_str().unwrap(),
            right.to_str().unwrap(),
        )
        .unwrap(),
    )
    .unwrap();
    compare["left"]["path"] = json!("snapshot-left.json");
    compare["right"]["path"] = json!("snapshot-right.json");
    assert_eq!(compare, expected["compare"]);

    let restored: Value = serde_json::from_str(
        &restore_artifact_json(project.to_str().unwrap(), left.to_str().unwrap()).unwrap(),
    )
    .unwrap();
    let restored_path = Path::new(restored["restored_artifact"].as_str().unwrap());
    assert_eq!(restore_summary(restored_path), expected["restore_snapshot"]);

    let restored: Value = serde_json::from_str(
        &restore_artifact_json(project.to_str().unwrap(), candidates.to_str().unwrap()).unwrap(),
    )
    .unwrap();
    let restored_path = Path::new(restored["restored_artifact"].as_str().unwrap());
    assert_eq!(
        restore_summary(restored_path),
        expected["restore_candidate_set"]
    );
}
