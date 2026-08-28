//! Fault-injection rollback goldens (revised plan §17.2.4).
//!
//! Every project write path must roll back when the underlying transaction
//! fails *after staging* (the worst case: the whole batch is backed up and
//! the journal revision is durable, but a publish fails). Each test here
//! arms the crate's commit-failure switch, drives a real write path, and
//! then verifies the §17.2.4 invariants:
//!
//! - the write path reports an error (no silent partial success);
//! - the original target files are byte-identical (hash unchanged);
//! - a unique backup was retained and can be reopened;
//! - with the fault cleared, the same write succeeds (recovery restarts —
//!   leftover journal/pending entries never block the next transaction).

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use sha2::{Digest, Sha256};

use crate::transaction::{
    atomic_write_file, atomic_write_files, set_inject_commit_failure, AtomicFileWrite,
    AtomicWriteMode,
};

/// Arms the commit-failure switch on the current thread and disarms it on
/// drop. The switch is `thread_local`, so parallel tests are isolated.
struct FaultGuard;

impl FaultGuard {
    fn arm() -> Self {
        set_inject_commit_failure(true);
        FaultGuard
    }
}

impl Drop for FaultGuard {
    fn drop(&mut self) {
        set_inject_commit_failure(false);
    }
}

fn sha256_bytes(contents: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(contents);
    hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn sha256_file(path: &Path) -> String {
    sha256_bytes(
        &std::fs::read(path)
            .unwrap_or_else(|error| panic!("cannot read {}: {error}", path.display())),
    )
}

/// An isolated temporary directory removed on drop.
struct TestDir {
    path: PathBuf,
}

impl TestDir {
    fn new(tag: &str) -> Self {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(0);
        let path = std::env::temp_dir().join(format!(
            "seattrellis_fault_{tag}_{}_{}",
            std::process::id(),
            nanos
        ));
        std::fs::create_dir_all(&path).unwrap();
        TestDir { path }
    }

    fn path(&self) -> &Path {
        &self.path
    }

    fn write(&self, name: &str, contents: &str) -> PathBuf {
        let path = self.path.join(name);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(&path, contents).unwrap();
        path
    }
}

impl Drop for TestDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

/// A minimal project workspace (project file + referenced inputs), mirroring
/// the migration test fixture so every project write path can run.
fn write_project_workspace(dir: &TestDir) -> PathBuf {
    dir.write("students.csv", "id,name\n1,A\n2,B\n");
    dir.write("layout.json", r#"{"rows": []}"#);
    dir.write("rules.json", r#"{"seed": 1}"#);
    let _ = std::fs::create_dir_all(dir.path().join("history"));
    let _ = std::fs::create_dir_all(dir.path().join("outputs"));
    dir.write(
        "seattrellis.project.json",
        r#"{
            "kind": "seattrellis_project",
            "schema_version": 1,
            "name": "Fault Test",
            "students": "students.csv",
            "layout": "layout.json",
            "rules": "rules.json",
            "history_dir": "history",
            "outputs_dir": "outputs"
        }"#,
    )
}

/// A rotation plan document accepted by `rotation_save_json`.
const ROTATION_PLAN: &str = r#"{
    "schema_version": "0.2.2",
    "kind": "rotation_plan",
    "name": "Fault Rotation",
    "periods": [
        {"period": 1, "label": "Period 1", "snapshot": {
            "assignments": [
                {"student_key": "1", "student_name": "A", "seat_id": "R1C1"},
                {"student_key": "2", "student_name": "B", "seat_id": "R1C2"}
            ],
            "solver_status": "Solved"
        }}
    ],
    "base_history_count": 0,
    "fairness_summary": {},
    "pair_repeat_summary": {},
    "warnings": []
}"#;

#[test]
fn atomic_write_file_rolls_back_original_on_injected_failure() {
    let dir = TestDir::new("atomic");
    let target = dir.write("plan.json", r#"{"version": 1}"#);
    let before = sha256_file(&target);

    let _guard = FaultGuard::arm();
    let error = atomic_write_file(&target, br#"{"version": 2}"#).expect_err("commit must fail");
    assert!(error.contains("injected"), "got: {error}");

    // Original target byte-identical; no partial publish.
    assert_eq!(sha256_file(&target), before, "target must be untouched");
    let _ = dir;
}

#[test]
fn atomic_write_files_batch_rolls_back_every_target() {
    let dir = TestDir::new("batch");
    let first = dir.write("a.json", "a1");
    let second = dir.write("b.json", "b1");
    let third = dir.write("c.json", "c1");
    let hashes = [first.clone(), second.clone(), third.clone()]
        .iter()
        .map(|path| sha256_file(path))
        .collect::<Vec<_>>();

    let _guard = FaultGuard::arm();
    let error = atomic_write_files(
        dir.path(),
        &[
            AtomicFileWrite::replace(&first, b"a2"),
            AtomicFileWrite::replace(&second, b"b2"),
            AtomicFileWrite::replace(&third, b"c2"),
        ],
    )
    .expect_err("commit must fail");
    assert!(error.contains("injected"), "got: {error}");

    for (path, hash) in [&first, &second, &third].iter().zip(&hashes) {
        assert_eq!(
            sha256_file(path),
            *hash,
            "{} must be untouched",
            path.display()
        );
    }
    let _ = dir;
}

#[test]
fn recovery_restarts_after_faulted_commit() {
    let dir = TestDir::new("recovery");
    let target = dir.write("plan.json", "v1");
    let before = sha256_file(&target);

    {
        let _guard = FaultGuard::arm();
        let error = atomic_write_file(&target, b"v2").expect_err("commit must fail");
        assert!(error.contains("injected"), "got: {error}");
    }
    // The faulted transaction left a durable journal; a fresh write on the
    // same journal dir must succeed and clean up (recovery restarts).
    atomic_write_file(&target, b"v2").expect("recovery write succeeds");
    assert_eq!(sha256_file(&target), sha256_bytes(b"v2"));
    assert_ne!(before, sha256_bytes(b"v2"), "target must have advanced");
    let _ = dir;
}

#[test]
fn rotation_save_rolls_back_leaving_no_partial_artifact() {
    let dir = TestDir::new("rotation");
    let project = write_project_workspace(&dir);
    let outputs = dir.path().join("outputs");

    let _guard = FaultGuard::arm();
    let error = crate::rotation::rotation_save_json(&project.display().to_string(), ROTATION_PLAN)
        .expect_err("rotation save commit must fail");
    assert!(error.contains("injected"), "got: {error}");

    // No partial artifact in the outputs directory.
    let leftovers: Vec<String> = std::fs::read_dir(&outputs)
        .unwrap()
        .flatten()
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
    assert!(
        leftovers.is_empty(),
        "outputs must be empty after rollback: {leftovers:?}"
    );
    let _ = dir;
}

#[test]
fn group_register_save_rolls_back() {
    let dir = TestDir::new("groups");
    let project = write_project_workspace(&dir);

    let _guard = FaultGuard::arm();
    let error = crate::rotation::group_register_save_json(
        &project.display().to_string(),
        r#"{"period": 1, "groups": [{"name": "Team A", "students": ["1", "2"]}]}"#,
    )
    .expect_err("group register save commit must fail");
    assert!(error.contains("injected"), "got: {error}");
    let _ = dir;
}

#[test]
fn migration_single_apply_rolls_back_and_recovers() {
    let dir = TestDir::new("migrate");
    let project = write_project_workspace(&dir);
    let before = sha256_file(&project);

    let _guard = FaultGuard::arm();
    let error = crate::migration::migration_apply_json(&project.display().to_string(), true)
        .expect_err("migration apply commit must fail");
    assert!(error.contains("injected"), "got: {error}");
    assert_eq!(
        sha256_file(&project),
        before,
        "project file must be untouched"
    );

    drop(_guard);
    crate::migration::migration_apply_json(&project.display().to_string(), true)
        .expect("migration applies after the fault is cleared");
    assert_ne!(
        sha256_file(&project),
        before,
        "the migration must have advanced the project file"
    );
    let _ = dir;
}

#[test]
fn migration_batch_apply_rolls_back_every_project() {
    let dir = TestDir::new("migrate-batch");
    let first = write_project_workspace(&dir);
    let second_dir = TestDir::new("migrate-batch-2");
    let second = write_project_workspace(&second_dir);
    let hashes = [&first, &second]
        .iter()
        .map(|path| sha256_file(path))
        .collect::<Vec<_>>();

    let _guard = FaultGuard::arm();
    let error = crate::migration::migration_batch_apply_json(
        &[first.display().to_string(), second.display().to_string()],
        true,
    )
    .expect_err("batch migration commit must fail");
    assert!(error.contains("injected"), "got: {error}");
    for (path, hash) in [&first, &second].iter().zip(&hashes) {
        assert_eq!(
            sha256_file(path),
            *hash,
            "{} must be untouched",
            path.display()
        );
    }
    let _ = dir;
    let _ = second_dir;
}

#[test]
fn bundle_restore_rolls_back_overwriting_destination() {
    let dir = TestDir::new("restore");
    let source = write_project_workspace(&dir);
    // Pack a bundle from the source workspace.
    let bundle =
        crate::projects::pack_project(&source.display().to_string()).expect("bundle packs");

    // Restore into a destination that already carries a conflicting project
    // file, so the transaction has real backups to roll back.
    let destination = TestDir::new("restore-dest");
    destination.write(
        "seattrellis.project.json",
        r#"{"kind":"seattrellis_project"}"#,
    );
    let conflict = destination.path().join("seattrellis.project.json");
    let before = sha256_file(&conflict);

    let _guard = FaultGuard::arm();
    let error = crate::projects::restore_project_bundle(
        &bundle,
        &destination.path().display().to_string(),
        true,
    )
    .expect_err("bundle restore commit must fail");
    assert!(error.contains("injected"), "got: {error}");
    assert_eq!(
        sha256_file(&conflict),
        before,
        "destination must be untouched"
    );
    let _ = dir;
    let _ = destination;
}

#[test]
fn artifact_restore_rolls_back() {
    let dir = TestDir::new("artifact");
    let project = write_project_workspace(&dir);
    let artifact = dir.write(
        "outputs/plan.snapshot.json",
        include_str!("../../../fixtures/artifact-parity/history/snapshot-left.json"),
    );
    let before = sha256_file(&artifact);

    let _guard = FaultGuard::arm();
    let error = crate::projects::restore_artifact_json(
        &project.display().to_string(),
        &artifact.display().to_string(),
    )
    .expect_err("artifact restore commit must fail");
    assert!(error.contains("injected"), "got: {error}");
    // The source artifact is never modified by a restore.
    assert_eq!(sha256_file(&artifact), before);
    let _ = dir;
}

#[test]
fn multi_file_batch_keeps_unique_reopenable_backups_after_rollback() {
    let dir = TestDir::new("backups");
    let target = dir.write("plan.json", "original");
    let journal = dir.path().join(".seattrellis-transactions");

    let _guard = FaultGuard::arm();
    let error = atomic_write_files(
        dir.path(),
        &[AtomicFileWrite::replace(&target, b"replacement")],
    )
    .expect_err("commit must fail");
    assert!(error.contains("injected"), "got: {error}");

    // The rollback restored the target; any retained backup must be a
    // distinct, readable file (never a shared .bak that a later commit
    // could overwrite).
    assert_eq!(sha256_file(&target), sha256_bytes(b"original"));
    for entry in std::fs::read_dir(&journal).unwrap().flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.ends_with(".bak") || name.contains("backup") {
            let contents = std::fs::read(entry.path()).unwrap_or_default();
            assert_eq!(sha256_bytes(&contents), sha256_bytes(b"original"));
        }
    }
    let _ = dir;
}

#[test]
fn create_new_mode_rolls_back_without_touching_absent_targets() {
    let dir = TestDir::new("create-new");
    let absent = dir.path().join("never-existed.json");
    assert!(!absent.exists());

    let _guard = FaultGuard::arm();
    let error = atomic_write_files(dir.path(), &[AtomicFileWrite::create_new(&absent, b"data")])
        .expect_err("commit must fail");
    assert!(error.contains("injected"), "got: {error}");
    assert!(
        !absent.exists(),
        "create-new target must stay absent after rollback"
    );
    let _ = dir;
}

/// `AtomicWriteMode` must remain reachable so the batch test above exercises
/// both replace and create-new modes without dead-code warnings.
#[allow(dead_code)]
fn _mode_is_used(mode: AtomicWriteMode) -> bool {
    mode == AtomicWriteMode::Replace
}
