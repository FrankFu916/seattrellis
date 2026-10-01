//! Real process coverage: recovery must wait for a live writer, and OS locks
//! must disappear on process death so an abandoned journal remains recoverable.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use seattrellis_io::transaction::{
    atomic_create_file, recover_leftover_transactions_with_root, FileTransaction,
};

struct Worker(Child);

impl Drop for Worker {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn start_worker(root: &Path, mode: &str) -> Worker {
    Worker(
        Command::new(std::env::current_exe().unwrap())
            .args(["transaction_worker", "--exact", "--nocapture"])
            .env("SEATTRELLIS_IO_TEST_ROOT", root)
            .env("SEATTRELLIS_IO_TEST_MODE", mode)
            .stdout(Stdio::null())
            .spawn()
            .unwrap(),
    )
}

fn temp_root(tag: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!(
        "seattrellis-process-{tag}-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir_all(&root).unwrap();
    root
}

fn wait_for_file(path: &Path) {
    let deadline = Instant::now() + Duration::from_secs(15);
    while !path.exists() {
        assert!(
            Instant::now() < deadline,
            "worker did not signal {}",
            path.display()
        );
        thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn transaction_worker() {
    let Some(root) = std::env::var_os("SEATTRELLIS_IO_TEST_ROOT") else {
        return;
    };
    let root = PathBuf::from(root);
    let journal = root.join(".seattrellis-transactions");
    match std::env::var("SEATTRELLIS_IO_TEST_MODE").unwrap().as_str() {
        "hold" | "crash" => {
            let mut txn = FileTransaction::begin_with_root(&journal, &root).unwrap();
            txn.stage_new(&root.join("first.json"), b"first").unwrap();
            fs::write(root.join("staged"), b"ready").unwrap();
            if std::env::var("SEATTRELLIS_IO_TEST_MODE").unwrap() == "crash" {
                // Exit bypasses Rust Drop, matching an abruptly killed writer.
                std::process::exit(0);
            }
            wait_for_file(&root.join("release"));
            txn.commit(|_| Ok(())).unwrap();
        }
        "begin" => {
            // A different custom journal still shares the trusted-root lock.
            let mut txn =
                FileTransaction::begin_with_root(&root.join("custom-journal"), &root).unwrap();
            fs::write(root.join("recovery-count"), b"0").unwrap();
            txn.stage_new(&root.join("second.json"), b"second").unwrap();
            txn.commit(|_| Ok(())).unwrap();
        }
        "recover" => {
            let recovered = recover_leftover_transactions_with_root(&journal, &root).unwrap();
            fs::write(root.join("recovery-count"), recovered.to_string()).unwrap();
            atomic_create_file(&root.join("second.json"), b"second").unwrap();
        }
        _ => panic!("invalid test worker mode"),
    }
}

#[test]
fn public_recovery_waits_for_live_cross_process_transaction() {
    for mode in ["recover", "begin"] {
        let root = temp_root(mode);
        let mut first = start_worker(&root, "hold");
        wait_for_file(&root.join("staged"));
        let mut second = start_worker(&root, mode);
        thread::sleep(Duration::from_millis(150));
        assert!(
            second.0.try_wait().unwrap().is_none(),
            "{mode} raced a live writer"
        );
        assert!(!root.join("recovery-count").exists());
        fs::write(root.join("release"), b"go").unwrap();
        assert!(first.0.wait().unwrap().success());
        assert!(second.0.wait().unwrap().success());
        assert_eq!(fs::read(root.join("first.json")).unwrap(), b"first");
        assert_eq!(fs::read(root.join("second.json")).unwrap(), b"second");
        assert_eq!(
            fs::read_to_string(root.join("recovery-count")).unwrap(),
            "0"
        );
        fs::remove_dir_all(root).unwrap();
    }
}

#[test]
fn crashed_process_releases_root_lock_and_journal_is_recovered() {
    let root = temp_root("crash");
    let mut crashed = start_worker(&root, "crash");
    wait_for_file(&root.join("staged"));
    assert!(crashed.0.wait().unwrap().success());
    let mut recovery = start_worker(&root, "recover");
    assert!(recovery.0.wait().unwrap().success());
    assert_eq!(
        fs::read_to_string(root.join("recovery-count")).unwrap(),
        "1"
    );
    assert!(!root.join("first.json").exists());
    assert_eq!(fs::read(root.join("second.json")).unwrap(), b"second");
    fs::remove_dir_all(root).unwrap();
}
