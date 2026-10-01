//! Cross-command contracts: a successful save must remain usable by the
//! next real CLI invocation, and a failed/dry-run command must not publish.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

use serde_json::{json, Value};

const BIN: &str = env!("CARGO_BIN_EXE_seattrellis");
static NEXT_FIXTURE: AtomicU64 = AtomicU64::new(0);

struct Workspace(PathBuf);

impl Workspace {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "seattrellis-cli-regressions-{}-{}",
            std::process::id(),
            NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&root).unwrap();
        write_json(
            &root.join("students.json"),
            &json!({"students": [
                {"student_id":"1","name":"Alice","gender":"F","height_cm":165,
                 "score":92,"vision":"0.7","notes":"Preserve this note",
                 "needs":["vision_front"],"attributes":{"example.priority":true}},
                {"student_id":"2","name":"Bob","score":75},
                {"student_id":"3","name":"Carla","score":85}
            ]}),
        );
        write_json(
            &root.join("layout.json"),
            &json!({"layout_id":"test","name":"Room","seats":[
                {"seat_id":"A1","row":1,"col":1},
                {"seat_id":"A2","row":1,"col":2},
                {"seat_id":"A3","row":2,"col":1},
                {"seat_id":"A4","row":2,"col":2}
            ]}),
        );
        write_json(&root.join("rules.json"), &json!({"seed":7,"soft":{}}));
        write_json(
            &root.join("seattrellis.project.json"),
            &json!({"kind":"seattrellis_project","schema_version":1,
                "name":"Regression","students":"students.json","layout":"layout.json",
                "rules":"rules.json","outputs_dir":"custom-results","history_dir":"history",
                "default_candidates":2,"default_candidate":"recommended",
                "default_export_format":"html"}),
        );
        std::fs::create_dir_all(root.join("history")).unwrap();
        Self(root)
    }

    fn path(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }

    fn project(&self) -> String {
        text_path(&self.path("seattrellis.project.json"))
    }
}

impl Drop for Workspace {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn text_path(path: &Path) -> String {
    path.to_str().unwrap().to_string()
}

fn run(arguments: &[&str]) -> Output {
    Command::new(BIN).args(arguments).output().unwrap()
}

fn successful(arguments: &[&str]) -> Output {
    let output = run(arguments);
    assert!(
        output.status.success(),
        "{arguments:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

fn read_json(path: &Path) -> Value {
    serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap()
}

fn write_json(path: &Path, value: &Value) {
    std::fs::write(path, serde_json::to_vec(value).unwrap()).unwrap();
}

#[test]
fn schema_dry_run_validates_every_version_without_writing() {
    let workspace = Workspace::new();
    let v1 = workspace.path("students.json");
    let v2 = workspace.path("roster-v2.json");
    let sentinel = workspace.path("sentinel.json");
    std::fs::write(&sentinel, b"ORIGINAL").unwrap();
    successful(&[
        "schema-migrate",
        "--input",
        &text_path(&v1),
        "--dry-run",
        "--output",
        &text_path(&sentinel),
    ]);
    assert_eq!(std::fs::read(&sentinel).unwrap(), b"ORIGINAL");
    successful(&[
        "schema-migrate",
        "--input",
        &text_path(&v1),
        "--output",
        &text_path(&v2),
    ]);
    let original = std::fs::read(&v2).unwrap();
    successful(&["schema-migrate", "--input", &text_path(&v2), "--dry-run"]);
    assert_eq!(std::fs::read(&v2).unwrap(), original);
    assert_eq!(
        run(&[
            "schema-migrate",
            "--input",
            &text_path(&v2),
            "--dry-run",
            "--in-place"
        ])
        .status
        .code(),
        Some(2)
    );
    assert_eq!(std::fs::read(&v2).unwrap(), original);
    successful(&[
        "schema-migrate",
        "--input",
        &text_path(&v2),
        "--dry-run",
        "--output",
        &text_path(&sentinel),
    ]);
    assert_eq!(std::fs::read(&sentinel).unwrap(), b"ORIGINAL");
    for invalid in [
        json!({"kind":"project","schema_version":2,"data":{"bogus":true}}),
        json!({"kind":"student_roster","schema_version":2,"data":{"students":[]},"unknown":true}),
        json!({"kind":"student_roster","schema_version":"2","data":{"students":[]}}),
        json!({"kind":"student_roster","schema_version":3,"data":{"students":[]}}),
        json!({"students":[{"student_id":"1","name":"A"}],"unknown":true}),
    ] {
        let path = workspace.path("invalid.json");
        write_json(&path, &invalid);
        let output = run(&[
            "schema-migrate",
            "--input",
            &text_path(&path),
            "--output",
            &text_path(&sentinel),
            "--dry-run",
        ]);
        assert!(!output.status.success(), "accepted {invalid}");
        assert_eq!(std::fs::read(&sentinel).unwrap(), b"ORIGINAL");
    }
    assert!(!workspace
        .path(".seattrellis-transactions")
        .read_dir()
        .unwrap()
        .any(|entry| entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .contains("journal.json")));
}

#[test]
fn migrated_roster_layout_and_project_reopen_in_real_consumers() {
    let workspace = Workspace::new();
    for file in ["students.json", "layout.json", "seattrellis.project.json"] {
        successful(&[
            "schema-migrate",
            "--input",
            &text_path(&workspace.path(file)),
            "--in-place",
        ]);
    }
    successful(&[
        "project-validate",
        "--project",
        &workspace.project(),
        "--strict",
    ]);
    successful(&[
        "project-solve",
        "--project",
        &workspace.project(),
        "--candidates",
        "1",
    ]);
    let snapshot = workspace.path("custom-results/latest.snapshot.json");
    let document = read_json(&snapshot);
    assert_eq!(document["students"][0]["notes"], "Preserve this note");
    assert_eq!(
        document["students"][0]["attributes"]["example.priority"],
        true
    );
    assert_eq!(
        document["original_request"]["seat_positions"][1],
        json!([2.0, 1.0])
    );
    successful(&[
        "project-export",
        "--project",
        &workspace.project(),
        "--snapshot",
        &text_path(&snapshot),
        "--output",
        &text_path(&workspace.path("migrated.html")),
    ]);
}

#[test]
fn standalone_edit_reopen_repair_and_export_keep_source_and_locks() {
    let workspace = Workspace::new();
    let initial = workspace.path("initial.json");
    successful(&[
        "project-solve",
        "--project",
        &workspace.project(),
        "--candidates",
        "1",
        "--output",
        &text_path(&initial),
    ]);
    let mut initial_document = read_json(&initial);
    initial_document["metadata"] = json!({"example.provenance":{"source":"manual import"}});
    write_json(&initial, &initial_document);
    let seat_index = initial_document["assignment"]
        .as_array()
        .unwrap()
        .iter()
        .find(|pair| pair[0] == 1)
        .unwrap()[1]
        .as_u64()
        .unwrap() as usize;
    let locked_seat = initial_document["layout"]["seats"][seat_index]["seat_id"]
        .as_str()
        .unwrap();
    let lock_operation = format!("lock-seat:{locked_seat}");
    let unlock_operation = format!("unlock-seat:{locked_seat}");
    let locked = workspace.path("locked.snapshot.json");
    successful(&[
        "edit",
        "--snapshot",
        &text_path(&initial),
        "--operation",
        "lock-student:1",
        "--operation",
        &lock_operation,
        "--output",
        &text_path(&locked),
    ]);
    let locked_document = read_json(&locked);
    assert_eq!(
        locked_document["metadata"]["lock_state"]["locked_students"],
        json!(["1"])
    );
    assert_eq!(
        locked_document["metadata"]["lock_state"]["locked_seats"],
        json!([locked_seat])
    );
    assert_eq!(
        locked_document["metadata"]["example.provenance"]["source"],
        "manual import"
    );
    assert_eq!(
        locked_document["students"][0]["notes"],
        "Preserve this note"
    );
    assert_eq!(locked_document["students"][0]["score"], 92);
    let forbidden = workspace.path("forbidden.json");
    let output = run(&[
        "edit",
        "--snapshot",
        &text_path(&locked),
        "--operation",
        "swap:1:2",
        "--output",
        &text_path(&forbidden),
    ]);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("locked"));
    assert!(!forbidden.exists());
    let output = run(&[
        "edit",
        "--snapshot",
        &text_path(&locked),
        "--operation",
        "swap:2:3",
        "--output",
        &text_path(&forbidden),
    ]);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("Seat is locked"));
    assert!(!forbidden.exists());
    let unlocked = workspace.path("unlocked.snapshot.json");
    successful(&[
        "edit",
        "--snapshot",
        &text_path(&locked),
        "--operation",
        "unlock-student:1",
        "--operation",
        &unlock_operation,
        "--operation",
        "swap:1:2",
        "--operation",
        "lock-student:2",
        "--output",
        &text_path(&unlocked),
        "--strict",
    ]);
    let problem = workspace.path("problem.json");
    write_json(&problem, &initial_document["original_request"]);
    let repaired = workspace.path("repaired.snapshot.json");
    successful(&[
        "repair",
        "--problem",
        &text_path(&problem),
        "--snapshot",
        &text_path(&unlocked),
        "--affected",
        "3",
        "--output",
        &text_path(&repaired),
    ]);
    let repaired_document = read_json(&repaired);
    assert_eq!(
        repaired_document["metadata"]["lock_state"]["locked_students"],
        json!(["2"])
    );
    assert_eq!(
        repaired_document["metadata"]["example.provenance"],
        locked_document["metadata"]["example.provenance"]
    );
    assert_eq!(repaired_document["students"], locked_document["students"]);
    let output = run(&[
        "edit",
        "--snapshot",
        &text_path(&repaired),
        "--operation",
        "swap:2:3",
        "--output",
        &text_path(&forbidden),
    ]);
    assert!(!output.status.success(), "repair lost saved lock");
    successful(&[
        "export",
        "--problem",
        &text_path(&problem),
        "--solution",
        &text_path(&repaired),
        "--format",
        "print-html",
        "--output",
        &text_path(&workspace.path("plan.html")),
    ]);
    assert!(std::fs::read_to_string(workspace.path("plan.html"))
        .unwrap()
        .contains("Alice"));
    let audit = successful(&[
        "audit",
        "--problem",
        &text_path(&problem),
        "--solution",
        &text_path(&repaired),
    ]);
    assert!(serde_json::from_slice::<Value>(&audit.stdout)
        .unwrap()
        .is_object());
}

#[test]
fn unnamed_core_problem_repair_can_reopen_edit_audit_and_export() {
    let workspace = Workspace::new();
    let problem = workspace.path("unnamed-problem.json");
    write_json(
        &problem,
        &json!({"api_version":2,"student_count":3,
        "seat_positions":[[0.0,0.0],[1.0,0.0],[2.0,0.0]],
        "edges":[[0,1],[1,2]],"seed":7}),
    );
    let solved = workspace.path("unnamed-solved.json");
    successful(&[
        "solve",
        "--problem",
        &text_path(&problem),
        "--output",
        &text_path(&solved),
    ]);
    let repaired = workspace.path("unnamed-repaired.json");
    successful(&[
        "repair",
        "--problem",
        &text_path(&problem),
        "--snapshot",
        &text_path(&solved),
        "--output",
        &text_path(&repaired),
    ]);
    let locked = workspace.path("unnamed-locked.json");
    successful(&[
        "edit",
        "--snapshot",
        &text_path(&repaired),
        "--operation",
        "lock-student:STU001",
        "--output",
        &text_path(&locked),
    ]);
    successful(&[
        "audit",
        "--problem",
        &text_path(&problem),
        "--solution",
        &text_path(&locked),
    ]);
    successful(&[
        "export",
        "--problem",
        &text_path(&problem),
        "--solution",
        &text_path(&locked),
        "--format",
        "html",
        "--output",
        &text_path(&workspace.path("unnamed.html")),
    ]);
}

#[test]
fn disabled_layout_extras_share_the_same_domain_across_cli_commands() {
    let workspace = Workspace::new();
    let problem = workspace.path("disabled-problem.json");
    write_json(
        &problem,
        &json!({"api_version":2,"student_count":2,"seed":7,
        "students":[{"key":"one","display_name":"Alice"},{"key":"two","display_name":"Bob"}],
        "seat_positions":[[1.0,1.0],[3.0,1.0]],"edges":[[0,1]],
        "layout":{"seats":[
            {"seat_id":"disabled","row":1,"col":0,"enabled":false},
            {"seat_id":"A1","row":1,"col":1,"enabled":true},
            {"seat_id":"middle","row":1,"col":2,"enabled":false},
            {"seat_id":"A3","row":1,"col":3,"enabled":true}
        ]}}),
    );
    let solved = workspace.path("disabled-solved.json");
    successful(&[
        "solve",
        "--problem",
        &text_path(&problem),
        "--output",
        &text_path(&solved),
    ]);
    let repaired = workspace.path("disabled-repaired.json");
    successful(&[
        "repair",
        "--problem",
        &text_path(&problem),
        "--snapshot",
        &text_path(&solved),
        "--output",
        &text_path(&repaired),
    ]);
    let locked = workspace.path("disabled-locked.json");
    successful(&[
        "edit",
        "--snapshot",
        &text_path(&repaired),
        "--operation",
        "lock-seat:A1",
        "--output",
        &text_path(&locked),
    ]);
    let assignments = read_json(&locked)["assignments"]
        .as_array()
        .unwrap()
        .clone();
    assert_eq!(assignments.len(), 2);
    assert!(assignments
        .iter()
        .all(|entry| entry["seat_id"] == "A1" || entry["seat_id"] == "A3"));
    let forbidden = workspace.path("disabled-forbidden.json");
    let output = run(&[
        "edit",
        "--snapshot",
        &text_path(&locked),
        "--operation",
        "swap:one:two",
        "--output",
        &text_path(&forbidden),
    ]);
    assert!(!output.status.success());
    assert!(!forbidden.exists());
    successful(&[
        "audit",
        "--problem",
        &text_path(&problem),
        "--solution",
        &text_path(&locked),
    ]);
    successful(&[
        "export",
        "--problem",
        &text_path(&problem),
        "--solution",
        &text_path(&locked),
        "--format",
        "svg",
        "--output",
        &text_path(&workspace.path("disabled.svg")),
    ]);
    let svg = std::fs::read_to_string(workspace.path("disabled.svg")).unwrap();
    assert!(
        svg.contains("Alice") && svg.contains("Bob"),
        "an assigned student was rendered as a disabled cell"
    );
}

#[test]
fn malformed_or_unknown_saved_locks_cannot_be_silently_dropped() {
    let workspace = Workspace::new();
    let snapshot = workspace.path("initial.json");
    successful(&[
        "project-solve",
        "--project",
        &workspace.project(),
        "--candidates",
        "1",
        "--output",
        &text_path(&snapshot),
    ]);
    let original = read_json(&snapshot);
    for locks in [
        json!({"locked_students":"1"}),
        json!({"locked_students":["missing"]}),
        json!({"locked_seats":["missing"]}),
        json!({"locked_students":[1]}),
        json!([]),
    ] {
        let mut document = original.clone();
        document["metadata"] = json!({"lock_state":locks});
        write_json(&snapshot, &document);
        let output_path = workspace.path("forbidden.json");
        let output = run(&[
            "edit",
            "--snapshot",
            &text_path(&snapshot),
            "--operation",
            "swap:1:2",
            "--output",
            &text_path(&output_path),
        ]);
        assert!(!output.status.success(), "accepted bad locks {locks}");
        assert!(!output_path.exists());
    }
    for kind in ["seattrellis_snapshot", "seating_snapshot", "candidate_set"] {
        let mut document = original.clone();
        document["kind"] = json!(kind);
        document["schema_version"] = json!(3);
        write_json(&snapshot, &document);
        let output_path = workspace.path("future.json");
        assert!(!run(&[
            "edit",
            "--snapshot",
            &text_path(&snapshot),
            "--operation",
            "swap:1:2",
            "--output",
            &text_path(&output_path)
        ])
        .status
        .success());
        assert!(!output_path.exists());
    }
}

#[test]
fn configured_outputs_support_default_edit_repair_export_and_backup() {
    let workspace = Workspace::new();
    successful(&[
        "project-solve",
        "--project",
        &workspace.project(),
        "--report",
        &text_path(&workspace.path("comparison.json")),
    ]);
    let candidates = workspace.path("custom-results/latest.candidates.json");
    assert!(candidates.is_file());
    successful(&[
        "project-edit",
        "--project",
        &workspace.project(),
        "--operation",
        "lock-student:1",
    ]);
    let edited = workspace.path("custom-results/edited-latest.candidates.json");
    assert!(edited.is_file());
    let output = run(&[
        "project-edit",
        "--project",
        &workspace.project(),
        "--operation",
        "swap:1:2",
    ]);
    assert!(!output.status.success());
    successful(&[
        "project-repair",
        "--project",
        &workspace.project(),
        "--affected",
        "3",
    ]);
    let repaired = workspace.path("custom-results/repaired-edited-latest.candidates.json");
    assert!(repaired.is_file());
    successful(&[
        "project-export",
        "--project",
        &workspace.project(),
        "--snapshot",
        &text_path(&repaired),
        "--format",
        "html",
        "--output",
        &text_path(&workspace.path("plan.html")),
    ]);
    assert!(!workspace.path("outputs").exists());
    let bundle = workspace.path("backup.seattrellis.zip");
    successful(&[
        "project-pack",
        "--project",
        &workspace.project(),
        "--output",
        &text_path(&bundle),
    ]);
    let restored = workspace.path("restored");
    successful(&[
        "project-restore",
        "--bundle",
        &text_path(&bundle),
        "--output-dir",
        &text_path(&restored),
    ]);
    assert_eq!(
        std::fs::read(restored.join("custom-results/repaired-edited-latest.candidates.json"))
            .unwrap(),
        std::fs::read(repaired).unwrap()
    );
}

#[test]
fn candidate_and_comparison_report_publish_as_one_batch() {
    let workspace = Workspace::new();
    let result = workspace.path("custom-results/result.json");
    std::fs::create_dir_all(result.parent().unwrap()).unwrap();
    std::fs::write(&result, b"ORIGINAL").unwrap();
    let report = workspace.path("reports/report.json");
    std::fs::create_dir_all(&report).unwrap();
    let output = run(&[
        "project-solve",
        "--project",
        &workspace.project(),
        "--output",
        &text_path(&result),
        "--report",
        &text_path(&report),
    ]);
    assert!(!output.status.success());
    assert_eq!(std::fs::read(&result).unwrap(), b"ORIGINAL");
    let output = run(&[
        "project-solve",
        "--project",
        &workspace.project(),
        "--output",
        &text_path(&result),
        "--report",
        &text_path(&result),
    ]);
    assert!(!output.status.success());
    assert_eq!(std::fs::read(&result).unwrap(), b"ORIGINAL");
    std::fs::remove_dir(&report).unwrap();
    successful(&[
        "project-solve",
        "--project",
        &workspace.project(),
        "--output",
        &text_path(&result),
        "--report",
        &text_path(&report),
    ]);
    assert!(read_json(&result)["candidates"].is_array());
    assert!(read_json(&report)["candidates"].is_array());
}

#[test]
fn project_history_reaches_solve_candidate_stability_and_rotation() {
    let workspace = Workspace::new();
    write_json(
        &workspace.path("history/period-1.snapshot.json"),
        &json!({"assignments":[
            {"student_key":"1","seat_id":"A1"},
            {"student_key":"2","seat_id":"A2"},
            {"student_key":"3","seat_id":"A3"}
        ]}),
    );
    write_json(
        &workspace.path("rules.json"),
        &json!({"seed":7,"soft":{
            "fair_rotation":{"enabled":true,"weight":5},
            "avoid_recent_neighbors":{"enabled":true,"weight":5}
        }}),
    );
    successful(&[
        "project-solve",
        "--project",
        &workspace.project(),
        "--report",
        &text_path(&workspace.path("comparison.json")),
    ]);
    let candidates = read_json(&workspace.path("custom-results/latest.candidates.json"));
    assert_eq!(
        candidates["original_request"]["history"]["history_count"],
        1
    );
    assert_eq!(
        candidates["original_request"]["pair_history"]["history_count"],
        1
    );
    let score = &candidates["candidates"][0]["plan_score"]["breakdown"];
    assert!(
        score["stability_score"]["score"].is_number(),
        "latest history not scored: {score}"
    );
    let comparison = read_json(&workspace.path("comparison.json"));
    for candidate in comparison["candidates"].as_array().unwrap() {
        assert!(!candidate["explanations"].as_array().unwrap().is_empty());
        assert_eq!(
            candidate["history_comparison"]["seating_history"],
            "1 prior seating periods considered"
        );
        for explanation in candidate["explanations"].as_array().unwrap() {
            let score = explanation["score"].as_f64().unwrap();
            assert!((0.0..=100.0).contains(&score));
        }
    }
    successful(&[
        "project-rotate",
        "--project",
        &workspace.project(),
        "--periods",
        "2",
    ]);
    let rotation = read_json(&workspace.path("custom-results/rotation-plan.json"));
    assert_eq!(rotation["base_history_count"], 1);
    assert_eq!(rotation["periods"].as_array().unwrap().len(), 2);
}

#[test]
fn concurrent_real_cli_writers_share_one_output_directory() {
    let workspace = Workspace::new();
    let outputs = workspace.path("concurrent");
    std::fs::create_dir(&outputs).unwrap();
    let mut children = Vec::new();
    for index in 0..16 {
        let target = outputs.join(format!("schema-{index}.json"));
        let child = Command::new(BIN)
            .args([
                "schema-export",
                "--kind",
                "rotation_plan",
                "--output",
                &text_path(&target),
            ])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        children.push((target, child));
    }
    for (target, child) in children {
        let output = child.wait_with_output().unwrap();
        assert!(
            output.status.success(),
            "concurrent writer failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(read_json(&target).is_object());
    }
}

#[test]
fn ignored_project_flags_are_errors_and_export_aliases_are_accepted() {
    let workspace = Workspace::new();
    let project = workspace.project();
    for arguments in [
        vec!["project-info", "--project", &project, "--seed", "42"],
        vec![
            "project-validate",
            "--project",
            &project,
            "--output",
            "ignored.json",
        ],
        vec!["project-solve", "--project", &project, "--format", "pdf"],
    ] {
        assert_eq!(run(&arguments).status.code(), Some(2));
    }
}

#[test]
fn default_candidate_and_saved_source_survive_project_changes() {
    let workspace = Workspace::new();
    successful(&["project-solve", "--project", &workspace.project()]);
    let candidates = read_json(&workspace.path("custom-results/latest.candidates.json"));
    let chosen = candidates["candidates"].as_array().unwrap().last().unwrap();
    let chosen_id = chosen["candidate_id"].as_str().unwrap();
    let mut project = read_json(&workspace.path("seattrellis.project.json"));
    project["default_candidate"] = json!(chosen_id);
    write_json(&workspace.path("seattrellis.project.json"), &project);
    successful(&[
        "project-edit",
        "--project",
        &workspace.project(),
        "--operation",
        "lock-student:3",
    ]);
    let edited = read_json(&workspace.path("custom-results/edited-latest.candidates.json"));
    assert_eq!(edited["metadata"]["candidate_id"], chosen_id);
    for pair in chosen["assignment"].as_array().unwrap() {
        let student =
            &candidates["original_request"]["students"][pair[0].as_u64().unwrap() as usize]["key"];
        let seat = &candidates["original_request"]["layout"]["seats"]
            [pair[1].as_u64().unwrap() as usize]["seat_id"];
        assert!(edited["assignments"]
            .as_array()
            .unwrap()
            .iter()
            .any(|entry| entry["student_key"] == *student && entry["seat_id"] == *seat));
    }
    let mut roster = read_json(&workspace.path("students.json"));
    roster["students"][0]["name"] = json!("Changed current roster");
    roster["students"][0]["score"] = json!(12);
    write_json(&workspace.path("students.json"), &roster);
    successful(&[
        "project-export",
        "--project",
        &workspace.project(),
        "--output",
        &text_path(&workspace.path("saved.html")),
    ]);
    let html = std::fs::read_to_string(workspace.path("saved.html")).unwrap();
    assert!(html.contains("Alice"));
    assert!(!html.contains("Changed current roster"));
    for format in [
        "svg",
        "html",
        "print-html",
        "png",
        "pdf",
        "xlsx",
        "excel",
        "docx",
        "pptx",
    ] {
        project["default_export_format"] = json!(format);
        write_json(&workspace.path("seattrellis.project.json"), &project);
        successful(&[
            "project-validate",
            "--project",
            &workspace.project(),
            "--strict",
        ]);
        successful(&[
            "schema-migrate",
            "--input",
            &workspace.project(),
            "--dry-run",
        ]);
    }
    successful(&[
        "project-export",
        "--project",
        &workspace.project(),
        "--format",
        "excel",
        "--output",
        &text_path(&workspace.path("saved.xlsx")),
    ]);
    assert!(std::fs::read(workspace.path("saved.xlsx"))
        .unwrap()
        .starts_with(b"PK"));
    // Canonical restore stores the compiled source in metadata so the strict
    // snapshot DTO can preserve its complete solver and history context.
    let mut metadata = edited["metadata"].clone();
    metadata["original_request"] = edited["original_request"].clone();
    let canonical = workspace.path("custom-results/restored.snapshot.json");
    write_json(
        &canonical,
        &json!({
            "kind":"seating_snapshot","schema_version":2,
            "data":{
                "schema_version":"0.2.2","metadata":metadata,
                "students":edited["students"],"layout":edited["layout"],
                "rules":edited["rules"],"assignments":edited["assignments"],
                "solver_status":edited["solver_status"]
            }
        }),
    );
    successful(&[
        "schema-migrate",
        "--input",
        &text_path(&canonical),
        "--dry-run",
    ]);
    let canonical_edit = workspace.path("canonical-edit.json");
    successful(&[
        "edit",
        "--snapshot",
        &text_path(&canonical),
        "--operation",
        "lock-student:2",
        "--output",
        &text_path(&canonical_edit),
    ]);
    assert_eq!(
        read_json(&canonical_edit)["original_request"],
        edited["original_request"]
    );
    for source in ["students.json", "layout.json", "rules.json"] {
        std::fs::remove_file(workspace.path(source)).unwrap();
    }
    successful(&[
        "project-export",
        "--project",
        &workspace.project(),
        "--format",
        "html",
        "--output",
        &text_path(&workspace.path("after-source-removed.html")),
    ]);
    assert!(
        std::fs::read_to_string(workspace.path("after-source-removed.html"))
            .unwrap()
            .contains("Alice")
    );
}
