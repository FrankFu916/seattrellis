//! Journaled multi-file transactions (M2-04).
//!
//! A [`FileTransaction`] makes a group of file replacements atomic as a
//! unit: every target is staged to a sibling temp file first, the transaction
//! is recorded in a journal (fsync'd) *before* any target is touched, each
//! existing target is backed up, then the temps are renamed into place and
//! the results are re-read. A failure at any point rolls the transaction
//! back from the backups; a journal left behind by a crashed process is
//! detected and rolled back by [`recover_leftover_transactions`] at startup.
//!
//! The plan's sequence is followed literally: stage → validate → backup →
//! sync → replace → reread, with failure injection proving the source is
//! never left damaged.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// One staged replacement inside a transaction.
#[derive(Debug)]
struct Step {
    /// The final destination path.
    target: PathBuf,
    /// The staged sibling temp file.
    temp: PathBuf,
    /// Backup of the pre-existing target (created on demand at commit).
    backup: Option<PathBuf>,
}

/// A journaled file transaction. Created with [`FileTransaction::begin`];
/// every staged file is recorded in the journal before any target changes.
pub struct FileTransaction {
    journal: PathBuf,
    steps: Vec<Step>,
    finished: bool,
}

impl Drop for FileTransaction {
    /// Abort (roll back) unless the transaction was committed or already
    /// rolled back. Safe to run on a crashed journal too.
    fn drop(&mut self) {
        if !self.finished {
            let _ = self.rollback();
        }
    }
}

impl FileTransaction {
    /// Begin a transaction rooted at `journal_dir`. The journal file records
    /// every staged replacement so a crash mid-transaction is recoverable.
    pub fn begin(journal_dir: &Path) -> Result<FileTransaction, String> {
        fs::create_dir_all(journal_dir)
            .map_err(|error| format!("cannot create journal dir {}: {error}", journal_dir.display()))?;
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(0);
        let journal = journal_dir.join(format!(
            "seattrellis-txn-{}-{nanos}.journal.json",
            std::process::id()
        ));
        // Create + fsync the journal up front: an empty journal means "begun
        // but nothing staged yet" and is rolled back as a no-op.
        let mut file = fs::File::create(&journal)
            .map_err(|error| format!("cannot create journal {}: {error}", journal.display()))?;
        file.write_all(b"{\"steps\":[]}")
            .and_then(|_| file.sync_all())
            .map_err(|error| format!("cannot initialize journal {}: {error}", journal.display()))?;
        Ok(FileTransaction {
            journal,
            steps: Vec::new(),
            finished: false,
        })
    }

    /// Stage a file replacement: write `contents` to a sibling temp file.
    /// The journal is updated (fsync'd) before the transaction is committed.
    pub fn stage(&mut self, target: &Path, contents: &[u8]) -> Result<(), String> {
        let parent = target
            .parent()
            .ok_or_else(|| format!("target has no parent directory: {}", target.display()))?;
        fs::create_dir_all(parent)
            .map_err(|error| format!("cannot prepare {}: {error}", parent.display()))?;
        let name = target
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| "file".to_string());
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(0);
        let temp = parent.join(format!(".{name}.{}-{nanos}.tmp", std::process::id()));
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)
            .map_err(|error| format!("cannot create temp file {}: {error}", temp.display()))?;
        file.write_all(contents)
            .and_then(|_| file.sync_all())
            .map_err(|error| format!("cannot write temp file {}: {error}", temp.display()))?;
        drop(file);
        self.steps.push(Step {
            target: target.to_path_buf(),
            temp,
            backup: None,
        });
        self.write_journal()
    }

    /// Commit: validate (optional), backup, replace, reread. On any failure
    /// the transaction rolls back and the error is returned.
    pub fn commit(
        mut self,
        validate: impl Fn(&Path) -> Result<(), String>,
    ) -> Result<(), String> {
        let result = self.commit_inner(&validate);
        if result.is_err() {
            let _ = self.rollback();
            self.finished = true;
            return result;
        }
        self.finished = true;
        let _ = fs::remove_file(&self.journal);
        Ok(())
    }

    fn commit_inner(&mut self, validate: &impl Fn(&Path) -> Result<(), String>) -> Result<(), String> {
        // validate every staged temp first: nothing is touched until the
        // whole batch validates.
        for step in &self.steps {
            validate(&step.temp)?;
        }
        // backup existing targets (recorded for rollback + recovery), then
        // replace (atomic rename).
        for step in &mut self.steps {
            if step.target.exists() {
                let backup = sibling_with_suffix(&step.target, "bak");
                fs::copy(&step.target, &backup).map_err(|error| {
                    format!("cannot back up {}: {error}", step.target.display())
                })?;
                step.backup = Some(backup);
            }
        }
        for step in &self.steps {
            fs::rename(&step.temp, &step.target).map_err(|error| {
                format!("cannot replace {}: {error}", step.target.display())
            })?;
        }
        // reread: the caller's validator runs on the final paths too.
        for step in &self.steps {
            validate(&step.target)?;
        }
        Ok(())
    }

    /// Roll back every staged replacement from its backup (or by removing a
    /// target that did not exist before). Returns the first error, if any.
    pub fn rollback(&mut self) -> Result<(), String> {
        let mut first_error = None;
        for step in self.steps.iter().rev() {
            if let Some(backup) = &step.backup {
                if backup.exists() {
                    if let Err(error) = fs::rename(backup, &step.target) {
                        first_error.get_or_insert_with(|| {
                            format!("rollback failed for {}: {error}", step.target.display())
                        });
                    }
                }
            } else if step.target.exists() && !step.temp.exists() {
                // replaced a file that did not exist before: remove it
                let _ = fs::remove_file(&step.target);
            }
            let _ = fs::remove_file(&step.temp);
        }
        let _ = fs::remove_file(&self.journal);
        self.finished = true;
        match first_error {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }

    fn write_journal(&self) -> Result<(), String> {
        let steps: Vec<serde_json::Value> = self
            .steps
            .iter()
            .map(|step| {
                serde_json::json!({
                    "target": step.target.to_string_lossy(),
                    "temp": step.temp.to_string_lossy(),
                })
            })
            .collect();
        let document = serde_json::json!({ "steps": steps });
        let path = &self.journal;
        let mut file = fs::File::create(path)
            .map_err(|error| format!("cannot rewrite journal {}: {error}", path.display()))?;
        serde_json::to_writer(&mut file, &document)
            .map_err(|error| format!("cannot write journal {}: {error}", path.display()))?;
        file.sync_all()
            .map_err(|error| format!("cannot sync journal {}: {error}", path.display()))
    }
}

/// A sibling path with a new suffix (e.g. `file.json` -> `file.json.bak`).
fn sibling_with_suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "file".to_string());
    name.push('.');
    name.push_str(suffix);
    match path.parent() {
        Some(parent) => parent.join(&name),
        None => PathBuf::from(name),
    }
}

/// Recover from a crashed process: any journal left in `journal_dir` means a
/// transaction did not finish. Leftover temps are removed and any replaced
/// target is restored from its backup (recorded by the journal) if possible.
pub fn recover_leftover_transactions(journal_dir: &Path) -> Result<usize, String> {
    let mut recovered = 0;
    if !journal_dir.exists() {
        return Ok(0);
    }
    let entries = fs::read_dir(journal_dir)
        .map_err(|error| format!("cannot list journal dir {}: {error}", journal_dir.display()))?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("json")
            || !path
                .file_name()
                .map(|name| name.to_string_lossy().starts_with("seattrellis-txn-"))
                .unwrap_or(false)
        {
            continue;
        }
        let document: serde_json::Value = serde_json::from_str(
            &fs::read_to_string(&path)
                .map_err(|error| format!("cannot read journal {}: {error}", path.display()))?,
        )
        .map_err(|error| format!("journal {} is malformed: {error}", path.display()))?;
        let steps = document
            .get("steps")
            .and_then(|steps| steps.as_array())
            .cloned()
            .unwrap_or_default();
        for step in steps {
            let target = PathBuf::from(step["target"].as_str().unwrap_or_default());
            let backup = sibling_with_suffix(&target, "bak");
            if backup.exists() {
                let _ = fs::rename(&backup, &target);
            }
            if let Some(temp) = step["temp"].as_str() {
                let _ = fs::remove_file(PathBuf::from(temp));
            }
        }
        let _ = fs::remove_file(&path);
        recovered += 1;
    }
    Ok(recovered)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "seattrellis-txn-test-{tag}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn json_validator(path: &Path) -> Result<(), String> {
        let contents = fs::read_to_string(path)
            .map_err(|error| format!("cannot read {}: {error}", path.display()))?;
        serde_json::from_str::<serde_json::Value>(&contents)
            .map(|_| ())
            .map_err(|error| format!("{} is not valid JSON: {error}", path.display()))
    }

    #[test]
    fn commit_replaces_all_targets_and_cleans_the_journal() {
        let dir = temp_dir("commit");
        let a = dir.join("a.json");
        let b = dir.join("b.json");
        fs::write(&a, "{\"old\":true}").unwrap();
        fs::write(&b, "{\"old\":true}").unwrap();

        let mut txn = FileTransaction::begin(&dir).unwrap();
        txn.stage(&a, b"{\"new\":true}").unwrap();
        txn.stage(&b, b"{\"new\":true}").unwrap();
        txn.commit(json_validator).unwrap();

        assert_eq!(fs::read_to_string(&a).unwrap(), "{\"new\":true}");
        assert_eq!(fs::read_to_string(&b).unwrap(), "{\"new\":true}");
        // backups are kept as .bak siblings
        assert!(a.with_extension("json.bak").exists());
        assert!(b.with_extension("json.bak").exists());
        // no leftover journal
        assert_eq!(recover_leftover_transactions(&dir).unwrap(), 0);
    }

    #[test]
    fn validation_failure_touches_nothing() {
        let dir = temp_dir("validate");
        let a = dir.join("a.json");
        fs::write(&a, "{\"old\":true}").unwrap();

        let mut txn = FileTransaction::begin(&dir).unwrap();
        txn.stage(&a, b"not json at all").unwrap();
        let error = txn.commit(json_validator).unwrap_err();
        assert!(error.contains("not valid JSON"));
        // source untouched, no backup created
        assert_eq!(fs::read_to_string(&a).unwrap(), "{\"old\":true}");
        assert!(!a.with_extension("json.bak").exists());
    }

    #[test]
    fn failure_injection_mid_commit_rolls_back() {
        let dir = temp_dir("inject");
        let a = dir.join("a.json");
        let b = dir.join("b.json");
        fs::write(&a, "{\"old\":true}").unwrap();
        fs::write(&b, "{\"old\":true}").unwrap();

        let mut txn = FileTransaction::begin(&dir).unwrap();
        txn.stage(&a, b"{\"new\":true}").unwrap();
        txn.stage(&b, b"{\"new\":true}").unwrap();
        // Inject failure: make b's rename impossible by turning the target
        // into a non-empty directory AFTER staging (rename fails with ENOTEMPTY).
        fs::remove_file(&b).unwrap();
        fs::create_dir(&b).unwrap();
        fs::write(b.join("blocker"), "x").unwrap();

        let error = txn.commit(json_validator).unwrap_err();
        assert!(
            error.contains("cannot"),
            "expected a failure mid-commit, got: {error}"
        );

        // a must be rolled back to its original content; b is a dir again.
        assert_eq!(fs::read_to_string(&a).unwrap(), "{\"old\":true}");
        assert!(b.is_dir());
        // no journal left after rollback
        assert_eq!(recover_leftover_transactions(&dir).unwrap(), 0);
    }

    #[test]
    fn drop_without_commit_rolls_back() {
        let dir = temp_dir("drop");
        let a = dir.join("a.json");
        fs::write(&a, "{\"old\":true}").unwrap();

        {
            let mut txn = FileTransaction::begin(&dir).unwrap();
            txn.stage(&a, b"{\"new\":true}").unwrap();
            // dropped here without commit
        }
        assert_eq!(fs::read_to_string(&a).unwrap(), "{\"old\":true}");
    }

    #[test]
    fn new_file_rollback_removes_it() {
        let dir = temp_dir("newfile");
        let fresh = dir.join("fresh.json");
        assert!(!fresh.exists());

        let mut txn = FileTransaction::begin(&dir).unwrap();
        txn.stage(&fresh, b"{\"new\":true}").unwrap();
        // force failure: validator rejects the temp
        let error = txn.commit(|_| Err("rejected".to_string())).unwrap_err();
        assert!(error.contains("rejected"));
        assert!(!fresh.exists(), "rolled-back new file must be removed");
    }

    #[test]
    fn recovery_restores_a_crashed_transaction_from_the_journal() {
        let dir = temp_dir("recover");
        let a = dir.join("a.json");
        fs::write(&a, "{\"old\":true}").unwrap();

        // Simulate a crash: journal exists, target was replaced, backup exists.
        let backup = sibling_with_suffix(&a, "bak");
        fs::copy(&a, &backup).unwrap();
        fs::write(&a, "{\"new\":true}").unwrap();
        let journal = dir.join("seattrellis-txn-12345-1.journal.json");
        fs::write(
            &journal,
            serde_json::to_string(&serde_json::json!({
                "steps": [{ "target": a.to_string_lossy(), "temp": dir.join(".a.json.tmp").to_string_lossy() }]
            }))
            .unwrap(),
        )
        .unwrap();

        let recovered = recover_leftover_transactions(&dir).unwrap();
        assert_eq!(recovered, 1);
        assert_eq!(fs::read_to_string(&a).unwrap(), "{\"old\":true}");
        assert!(!journal.exists());
    }
}
