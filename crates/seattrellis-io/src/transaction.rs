//! Crash-recoverable, journaled file-system transactions (M2-04).
//!
//! The important invariants are deliberately stricter than a plain
//! `write(temp); rename(temp, target)` helper:
//!
//! * journal paths are relative to caller-supplied trusted roots;
//! * every journal revision is written, synced, and renamed into place before
//!   the previous revision is removed;
//! * existing targets are moved to transaction-unique backups, never to a
//!   shared `.bak` path;
//! * targets are absent before publication, so Windows is never asked to
//!   rename over an existing file;
//! * files and their containing directories are synced at every durability
//!   boundary;
//! * rollback is fingerprint-aware and refuses to overwrite a concurrently
//!   modified path;
//! * an original commit error and a rollback error are both returned.
//!
//! [`atomic_write_file`] and [`atomic_write_files`] are the safe public APIs
//! intended for CLI/application adapters. [`FileTransaction`] remains public
//! for migration and restore workflows that need a custom validator.

use std::collections::{BTreeMap, HashSet};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

const JOURNAL_VERSION: u32 = 2;
const MAX_JOURNAL_BYTES: u64 = 4 * 1024 * 1024;
const JOURNAL_PREFIX: &str = "seattrellis-txn-";
const JOURNAL_SUFFIX: &str = ".journal.json";
pub(crate) const JOURNAL_DIR_NAME: &str = ".seattrellis-transactions";

static TRANSACTION_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Whether a staged write may replace an existing target.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AtomicWriteMode {
    /// Move an existing target to a unique recoverable backup, then publish.
    Replace,
    /// Refuse to publish if the target already exists.
    CreateNew,
}

/// One owned write request accepted by [`atomic_write_files`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AtomicFileWrite {
    pub target: PathBuf,
    pub contents: Vec<u8>,
    pub mode: AtomicWriteMode,
}

impl AtomicFileWrite {
    /// Replace `target`, retaining a unique backup when it already exists.
    pub fn replace(target: impl Into<PathBuf>, contents: impl Into<Vec<u8>>) -> Self {
        Self {
            target: target.into(),
            contents: contents.into(),
            mode: AtomicWriteMode::Replace,
        }
    }

    /// Create `target` without ever overwriting an existing path.
    pub fn create_new(target: impl Into<PathBuf>, contents: impl Into<Vec<u8>>) -> Self {
        Self {
            target: target.into(),
            contents: contents.into(),
            mode: AtomicWriteMode::CreateNew,
        }
    }
}

/// The durable backups retained after a successful transaction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransactionReceipt {
    pub backups: Vec<(PathBuf, PathBuf)>,
    /// Non-fatal post-commit notice, e.g. a journal file that could not be
    /// cleaned up. The commit itself is durable; the leftover journal is
    /// removed by the next recovery pass.
    pub cleanup_warning: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum EntryKind {
    File,
    Directory,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum OriginalState {
    Unknown,
    Absent,
    Present,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum StepState {
    Staged,
    Prepared,
    BackedUp,
    Publishing,
    Published,
    Verified,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum JournalPhase {
    Active,
    RollingBack,
    RolledBack,
    Committed,
}

#[derive(Debug)]
struct Step {
    kind: EntryKind,
    mode: AtomicWriteMode,
    root_index: usize,
    target: PathBuf,
    temp: PathBuf,
    backup: PathBuf,
    fingerprint: String,
    original: OriginalState,
    state: StepState,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct JournalStep {
    kind: EntryKind,
    mode: AtomicWriteMode,
    root_index: usize,
    target: String,
    temp: String,
    backup: String,
    fingerprint: String,
    original: OriginalState,
    state: StepState,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct JournalDocument {
    version: u32,
    transaction_id: String,
    revision: u64,
    roots: Vec<String>,
    phase: JournalPhase,
    steps: Vec<JournalStep>,
}

/// A journaled file-system transaction.
pub struct FileTransaction {
    journal_dir: PathBuf,
    roots: Vec<PathBuf>,
    transaction_id: String,
    revision: u64,
    current_journal: PathBuf,
    phase: JournalPhase,
    steps: Vec<Step>,
    finished: bool,
}

impl Drop for FileTransaction {
    fn drop(&mut self) {
        if !self.finished && self.phase != JournalPhase::Committed {
            // Drop cannot return an error. A failed best-effort rollback keeps
            // the synced journal in place for explicit startup recovery.
            let _rollback_error = self.rollback_inner();
        }
    }
}

impl FileTransaction {
    /// Backwards-compatible constructor. Its trusted root is the parent of
    /// `journal_dir`; callers with targets elsewhere must use
    /// [`FileTransaction::begin_with_root`] explicitly.
    pub fn begin(journal_dir: &Path) -> Result<Self, String> {
        let default_root = journal_dir
            .parent()
            .filter(|path| !path.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        Self::begin_with_root(journal_dir, default_root)
    }

    /// Begin a transaction authorized to modify paths beneath `allowed_root`.
    pub fn begin_with_root(journal_dir: &Path, allowed_root: &Path) -> Result<Self, String> {
        Self::begin_with_roots(journal_dir, &[allowed_root.to_path_buf()])
    }

    /// Begin a transaction with explicit trusted roots. The canonical roots
    /// are supplied again during recovery; absolute target paths from a
    /// journal are never trusted.
    pub fn begin_with_roots(journal_dir: &Path, allowed_roots: &[PathBuf]) -> Result<Self, String> {
        if allowed_roots.is_empty() {
            return Err("a transaction requires at least one trusted root".to_string());
        }
        prepare_journal_dir(journal_dir)?;
        let journal_dir = canonical_directory(journal_dir, "journal directory")?;
        let roots = canonical_roots(allowed_roots)?;
        let transaction_id = next_transaction_id();
        let current_journal = journal_dir.join(journal_file_name(&transaction_id, 0));
        let mut transaction = Self {
            journal_dir,
            roots,
            transaction_id,
            revision: 0,
            current_journal,
            phase: JournalPhase::Active,
            steps: Vec::new(),
            finished: false,
        };
        transaction.write_initial_journal()?;
        Ok(transaction)
    }

    /// Stage a file replacement. Existing targets receive a unique backup at
    /// commit time.
    pub fn stage(&mut self, target: &Path, contents: &[u8]) -> Result<(), String> {
        self.stage_file(target, contents, AtomicWriteMode::Replace)
    }

    /// Stage a file that must not already exist when committed.
    pub fn stage_new(&mut self, target: &Path, contents: &[u8]) -> Result<(), String> {
        self.stage_file(target, contents, AtomicWriteMode::CreateNew)
    }

    fn stage_file(
        &mut self,
        target: &Path,
        contents: &[u8],
        mode: AtomicWriteMode,
    ) -> Result<(), String> {
        self.ensure_active()?;
        let (root_index, target) = resolve_target(&self.roots, target)?;
        self.reject_duplicate_target(&target)?;
        reject_existing_kind_mismatch(&target, EntryKind::File)?;

        let step_index = self.steps.len();
        let temp =
            sibling_transaction_path(&target, "tmp", &self.transaction_id, step_index, "tmp")?;
        let backup =
            sibling_transaction_path(&target, "backup", &self.transaction_id, step_index, "bak")?;
        ensure_path_absent(&temp, "transaction temp")?;
        ensure_path_absent(&backup, "transaction backup")?;

        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)
            .map_err(|error| format!("cannot create temp file {}: {error}", temp.display()))?;
        if let Err(error) = file.write_all(contents).and_then(|_| file.sync_all()) {
            let cleanup = fs::remove_file(&temp);
            return match cleanup {
                Ok(()) => Err(format!(
                    "cannot write temp file {}: {error}",
                    temp.display()
                )),
                Err(cleanup_error) => Err(format!(
                    "cannot write temp file {}: {error}; cleanup also failed: {cleanup_error}",
                    temp.display()
                )),
            };
        }
        drop(file);
        sync_parent(&temp)?;
        let fingerprint = fingerprint_path(&temp, EntryKind::File)?;
        self.steps.push(Step {
            kind: EntryKind::File,
            mode,
            root_index,
            target,
            temp,
            backup,
            fingerprint,
            original: OriginalState::Unknown,
            state: StepState::Staged,
        });
        self.write_journal_revision()
    }

    /// Adopt a fully staged directory for atomic publication. `staging` and
    /// `target` must be siblings so the final rename stays on one filesystem.
    /// Existing targets are retained as unique backups.
    pub fn stage_directory(&mut self, target: &Path, staging: &Path) -> Result<(), String> {
        self.stage_directory_with_mode(target, staging, AtomicWriteMode::Replace)
    }

    /// Adopt a staged directory and require a new destination.
    pub fn stage_new_directory(&mut self, target: &Path, staging: &Path) -> Result<(), String> {
        self.stage_directory_with_mode(target, staging, AtomicWriteMode::CreateNew)
    }

    fn stage_directory_with_mode(
        &mut self,
        target: &Path,
        staging: &Path,
        mode: AtomicWriteMode,
    ) -> Result<(), String> {
        self.ensure_active()?;
        let (root_index, target) = resolve_target(&self.roots, target)?;
        let (staging_root, staging) =
            resolve_existing_entry(&self.roots, staging, EntryKind::Directory)?;
        if root_index != staging_root || target.parent() != staging.parent() {
            return Err(
                "a staged directory and its target must be siblings under one trusted root"
                    .to_string(),
            );
        }
        self.reject_duplicate_target(&target)?;
        reject_existing_kind_mismatch(&target, EntryKind::Directory)?;
        sync_tree(&staging)?;

        let step_index = self.steps.len();
        let temp =
            sibling_transaction_path(&target, "tmp", &self.transaction_id, step_index, "dir")?;
        let backup =
            sibling_transaction_path(&target, "backup", &self.transaction_id, step_index, "bak")?;
        ensure_path_absent(&temp, "transaction temp")?;
        ensure_path_absent(&backup, "transaction backup")?;
        fs::rename(&staging, &temp).map_err(|error| {
            format!(
                "cannot adopt staged directory {} as {}: {error}",
                staging.display(),
                temp.display()
            )
        })?;
        sync_parent(&temp)?;
        let fingerprint = fingerprint_path(&temp, EntryKind::Directory)?;
        self.steps.push(Step {
            kind: EntryKind::Directory,
            mode,
            root_index,
            target,
            temp,
            backup,
            fingerprint,
            original: OriginalState::Unknown,
            state: StepState::Staged,
        });
        self.write_journal_revision()
    }

    /// Commit with validation before any target is touched and again after
    /// publication.
    pub fn commit(self, validate: impl Fn(&Path) -> Result<(), String>) -> Result<(), String> {
        self.commit_with_receipt(validate).map(|_| ())
    }

    /// Commit and return every backup retained by the successful transaction.
    pub fn commit_with_receipt(
        mut self,
        validate: impl Fn(&Path) -> Result<(), String>,
    ) -> Result<TransactionReceipt, String> {
        match self.commit_inner(&validate) {
            Ok(receipt) => Ok(receipt),
            Err(commit_error) => match self.rollback_inner() {
                Ok(()) => Err(commit_error),
                Err(rollback_error) => Err(format!(
                    "{commit_error}; rollback also failed: {rollback_error}"
                )),
            },
        }
    }

    fn commit_inner(
        &mut self,
        validate: &impl Fn(&Path) -> Result<(), String>,
    ) -> Result<TransactionReceipt, String> {
        self.commit_inner_with_publisher(validate, &|_, step| publish_step(step))
    }

    fn commit_inner_with_publisher(
        &mut self,
        validate: &impl Fn(&Path) -> Result<(), String>,
        publish: &impl Fn(usize, &Step) -> Result<(), String>,
    ) -> Result<TransactionReceipt, String> {
        self.ensure_active()?;
        if self.steps.is_empty() {
            return Err("cannot commit an empty transaction".to_string());
        }

        // Validate and fingerprint every staged entry before touching targets.
        for step in &self.steps {
            validate_runtime_step(&self.roots, &self.transaction_id, step)?;
            ensure_fingerprint(step, &step.temp)?;
            sync_entry(&step.temp, step.kind)?;
            validate(&step.temp)?;
        }

        // Freeze the observed original state and backup paths durably before
        // the first target is moved.
        for step in &mut self.steps {
            step.original = inspect_original(&step.target, step.kind)?;
            if step.mode == AtomicWriteMode::CreateNew && step.original == OriginalState::Present {
                return Err(format!(
                    "target already exists and create-new was requested: {}",
                    step.target.display()
                ));
            }
            ensure_path_absent(&step.backup, "transaction backup")?;
            step.state = StepState::Prepared;
        }
        self.write_journal_revision()?;

        // Back up the entire batch before publishing any staged entry.
        for index in 0..self.steps.len() {
            if self.steps[index].original == OriginalState::Present {
                let target = self.steps[index].target.clone();
                let backup = self.steps[index].backup.clone();
                sync_entry(&target, self.steps[index].kind)?;
                fs::rename(&target, &backup).map_err(|error| {
                    format!(
                        "cannot move {} to unique backup {}: {error}",
                        target.display(),
                        backup.display()
                    )
                })?;
                sync_parent(&target)?;
            }
            self.steps[index].state = StepState::BackedUp;
            self.write_journal_revision()?;
        }

        for index in 0..self.steps.len() {
            self.steps[index].state = StepState::Publishing;
            self.write_journal_revision()?;
            publish(index, &self.steps[index])?;
            self.steps[index].state = StepState::Published;
            self.write_journal_revision()?;
        }

        for index in 0..self.steps.len() {
            ensure_fingerprint(&self.steps[index], &self.steps[index].target)?;
            validate(&self.steps[index].target)?;
            self.steps[index].state = StepState::Verified;
            self.write_journal_revision()?;
        }

        self.phase = JournalPhase::Committed;
        self.write_journal_revision()?;
        let backups = self
            .steps
            .iter()
            .filter(|step| step.original == OriginalState::Present)
            .map(|step| (step.target.clone(), step.backup.clone()))
            .collect();
        self.finished = true;
        // A durable commit is a success even when journal cleanup fails: the
        // leftover journal is removed by the next recovery pass, and the
        // caller must not report "no changes were written" for a committed
        // batch (that would tempt a double apply).
        let cleanup_warning = match self.cleanup_journal_files() {
            Ok(()) => None,
            Err(error) => Some(format!(
                "transaction committed durably, but journal cleanup failed: {error}"
            )),
        };
        Ok(TransactionReceipt {
            backups,
            cleanup_warning,
        })
    }

    /// Explicitly roll back this transaction. All rollback and cleanup errors
    /// are aggregated; the journal is retained whenever recovery is incomplete.
    pub fn rollback(&mut self) -> Result<(), String> {
        self.rollback_inner()
    }

    fn rollback_inner(&mut self) -> Result<(), String> {
        if self.phase == JournalPhase::Committed {
            self.finished = true;
            return Ok(());
        }
        let mut errors = Vec::new();
        self.phase = JournalPhase::RollingBack;
        if let Err(error) = self.write_journal_revision() {
            errors.push(format!("cannot record rollback state: {error}"));
        }

        for step in self.steps.iter().rev() {
            if let Err(error) = rollback_step(step) {
                errors.push(error);
            }
        }

        if !errors.is_empty() {
            return Err(errors.join("; "));
        }

        self.phase = JournalPhase::RolledBack;
        self.write_journal_revision()?;
        self.finished = true;
        self.cleanup_journal_files()
    }

    fn ensure_active(&self) -> Result<(), String> {
        if self.finished || self.phase != JournalPhase::Active {
            Err("transaction is no longer active".to_string())
        } else {
            Ok(())
        }
    }

    fn reject_duplicate_target(&self, target: &Path) -> Result<(), String> {
        if self.steps.iter().any(|step| step.target == target) {
            Err(format!(
                "a transaction cannot stage the same target twice: {}",
                target.display()
            ))
        } else {
            Ok(())
        }
    }

    fn write_initial_journal(&mut self) -> Result<(), String> {
        let document = self.journal_document()?;
        write_new_journal(&self.journal_dir, &self.current_journal, &document)
    }

    fn write_journal_revision(&mut self) -> Result<(), String> {
        let next_revision = self
            .revision
            .checked_add(1)
            .ok_or_else(|| "transaction journal revision overflow".to_string())?;
        let next = self
            .journal_dir
            .join(journal_file_name(&self.transaction_id, next_revision));
        let old = self.current_journal.clone();
        self.revision = next_revision;
        self.current_journal = next.clone();
        let document = self.journal_document()?;
        if let Err(error) = write_new_journal(&self.journal_dir, &next, &document) {
            self.revision -= 1;
            self.current_journal = old;
            return Err(error);
        }
        match fs::remove_file(&old) {
            Ok(()) => sync_directory(&self.journal_dir),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(format!(
                "cannot remove superseded journal {}: {error}",
                old.display()
            )),
        }
    }

    fn journal_document(&self) -> Result<JournalDocument, String> {
        let roots = self
            .roots
            .iter()
            .map(|root| path_as_utf8(root, "trusted root").map(str::to_string))
            .collect::<Result<Vec<_>, _>>()?;
        let steps = self
            .steps
            .iter()
            .map(|step| step_to_journal(step, &self.roots))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(JournalDocument {
            version: JOURNAL_VERSION,
            transaction_id: self.transaction_id.clone(),
            revision: self.revision,
            roots,
            phase: self.phase,
            steps,
        })
    }

    fn cleanup_journal_files(&self) -> Result<(), String> {
        cleanup_journal_files(&self.journal_dir, &self.transaction_id)
    }
}

/// Atomically replace one file, retaining a unique backup if it existed.
pub fn atomic_write_file(target: &Path, contents: &[u8]) -> Result<TransactionReceipt, String> {
    atomic_write_file_validated(target, contents, |_| Ok(()))
}

/// Atomically create one file without overwriting an existing path.
pub fn atomic_create_file(target: &Path, contents: &[u8]) -> Result<TransactionReceipt, String> {
    atomic_create_file_validated(target, contents, |_| Ok(()))
}

/// Atomically replace one file with caller-provided staged/final validation.
pub fn atomic_write_file_validated(
    target: &Path,
    contents: &[u8],
    validate: impl Fn(&Path) -> Result<(), String>,
) -> Result<TransactionReceipt, String> {
    atomic_write_one(target, contents, AtomicWriteMode::Replace, validate)
}

/// Atomically create one file with caller-provided staged/final validation.
pub fn atomic_create_file_validated(
    target: &Path,
    contents: &[u8],
    validate: impl Fn(&Path) -> Result<(), String>,
) -> Result<TransactionReceipt, String> {
    atomic_write_one(target, contents, AtomicWriteMode::CreateNew, validate)
}

fn atomic_write_one(
    target: &Path,
    contents: &[u8],
    mode: AtomicWriteMode,
    validate: impl Fn(&Path) -> Result<(), String>,
) -> Result<TransactionReceipt, String> {
    let parent = target
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent).map_err(|error| {
        format!(
            "cannot create output directory {}: {error}",
            parent.display()
        )
    })?;
    let root = canonical_directory(parent, "output directory")?;
    let journal_dir = root.join(JOURNAL_DIR_NAME);
    recover_leftover_transactions_with_root(&journal_dir, &root)?;
    let mut transaction = FileTransaction::begin_with_root(&journal_dir, &root)?;
    match mode {
        AtomicWriteMode::Replace => transaction.stage(target, contents)?,
        AtomicWriteMode::CreateNew => transaction.stage_new(target, contents)?,
    }
    transaction.commit_with_receipt(validate)
}

/// Atomically publish several files beneath one trusted root.
pub fn atomic_write_files(
    transaction_root: &Path,
    writes: &[AtomicFileWrite],
) -> Result<TransactionReceipt, String> {
    atomic_write_files_validated(transaction_root, writes, |_| Ok(()))
}

/// Atomically publish several files with validation before and after publish.
pub fn atomic_write_files_validated(
    transaction_root: &Path,
    writes: &[AtomicFileWrite],
    validate: impl Fn(&Path) -> Result<(), String>,
) -> Result<TransactionReceipt, String> {
    if writes.is_empty() {
        return Err("atomic_write_files requires at least one write".to_string());
    }
    fs::create_dir_all(transaction_root).map_err(|error| {
        format!(
            "cannot create transaction root {}: {error}",
            transaction_root.display()
        )
    })?;
    let root = canonical_directory(transaction_root, "transaction root")?;
    let journal_dir = root.join(JOURNAL_DIR_NAME);
    recover_leftover_transactions_with_root(&journal_dir, &root)?;
    let mut transaction = FileTransaction::begin_with_root(&journal_dir, &root)?;
    for write in writes {
        match write.mode {
            AtomicWriteMode::Replace => transaction.stage(&write.target, &write.contents)?,
            AtomicWriteMode::CreateNew => transaction.stage_new(&write.target, &write.contents)?,
        }
    }
    transaction.commit_with_receipt(validate)
}

/// Recover journals using the backwards-compatible default trusted root (the
/// parent of `journal_dir`). Malformed or unauthorized journals are rejected
/// before any target is touched.
pub fn recover_leftover_transactions(journal_dir: &Path) -> Result<usize, String> {
    let default_root = journal_dir
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    recover_leftover_transactions_with_root(journal_dir, default_root)
}

/// Recover journals authorized for one explicit trusted root.
pub fn recover_leftover_transactions_with_root(
    journal_dir: &Path,
    allowed_root: &Path,
) -> Result<usize, String> {
    recover_leftover_transactions_with_roots(journal_dir, &[allowed_root.to_path_buf()])
}

/// Recover all incomplete transactions after validating every journal against
/// caller-provided canonical roots. No absolute target from disk is trusted.
pub fn recover_leftover_transactions_with_roots(
    journal_dir: &Path,
    allowed_roots: &[PathBuf],
) -> Result<usize, String> {
    if !journal_dir.exists() {
        return Ok(0);
    }
    let journal_dir = canonical_directory(journal_dir, "journal directory")?;
    let roots = canonical_roots(allowed_roots)?;
    let expected_roots = roots
        .iter()
        .map(|root| path_as_utf8(root, "trusted root").map(str::to_string))
        .collect::<Result<Vec<_>, _>>()?;

    // Parse and authorize the complete journal set before applying recovery.
    let mut groups: BTreeMap<String, Vec<(PathBuf, JournalDocument)>> = BTreeMap::new();
    let entries = fs::read_dir(&journal_dir)
        .map_err(|error| format!("cannot list journal dir {}: {error}", journal_dir.display()))?;
    for entry in entries {
        let entry = entry.map_err(|error| {
            format!(
                "cannot inspect journal dir {}: {error}",
                journal_dir.display()
            )
        })?;
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        if !name.starts_with(JOURNAL_PREFIX) || !name.ends_with(JOURNAL_SUFFIX) {
            continue;
        }
        let metadata = fs::symlink_metadata(&path)
            .map_err(|error| format!("cannot inspect journal {}: {error}", path.display()))?;
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err(format!("journal is not a regular file: {}", path.display()));
        }
        if metadata.len() > MAX_JOURNAL_BYTES {
            return Err(format!("journal is too large: {}", path.display()));
        }
        let bytes = fs::read(&path)
            .map_err(|error| format!("cannot read journal {}: {error}", path.display()))?;
        let document: JournalDocument = serde_json::from_slice(&bytes)
            .map_err(|error| format!("journal {} is malformed: {error}", path.display()))?;
        validate_journal_document(&path, &document, &expected_roots, &roots)?;
        groups
            .entry(document.transaction_id.clone())
            .or_default()
            .push((path, document));
    }

    let mut recovered = 0;
    for (transaction_id, mut revisions) in groups {
        revisions.sort_by_key(|(_, document)| document.revision);
        let (_, document) = revisions
            .last()
            .cloned()
            .ok_or_else(|| "internal error: empty journal revision group".to_string())?;
        let steps = document
            .steps
            .iter()
            .enumerate()
            .map(|(index, step)| journal_to_step(step, index, &roots, &transaction_id))
            .collect::<Result<Vec<_>, _>>()?;
        let current_journal =
            journal_dir.join(journal_file_name(&transaction_id, document.revision));
        let mut transaction = FileTransaction {
            journal_dir: journal_dir.clone(),
            roots: roots.clone(),
            transaction_id,
            revision: document.revision,
            current_journal,
            phase: document.phase,
            steps,
            finished: false,
        };
        match transaction.phase {
            JournalPhase::Committed => {
                transaction.finished = true;
                transaction.cleanup_journal_files()?;
            }
            JournalPhase::RolledBack => {
                transaction.finished = true;
                transaction.cleanup_journal_files()?;
            }
            JournalPhase::Active | JournalPhase::RollingBack => transaction.rollback_inner()?,
        }
        recovered += 1;
    }

    // Orphan pending journals: a crash between writing a revision's
    // `*.json.pending` and renaming it into place leaves a file no group
    // references. Every group above has been cleaned; anything still matching
    // the journal prefix cannot belong to a live transaction.
    let entries = match fs::read_dir(&journal_dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(recovered),
        Err(error) => {
            return Err(format!(
                "cannot list journal dir {}: {error}",
                journal_dir.display()
            ))
        }
    };
    for entry in entries {
        let entry = entry.map_err(|error| {
            format!(
                "cannot inspect journal dir {}: {error}",
                journal_dir.display()
            )
        })?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with(JOURNAL_PREFIX) && name.ends_with(".json.pending") {
            if let Err(error) = fs::remove_file(entry.path()) {
                if error.kind() != std::io::ErrorKind::NotFound {
                    return Err(format!(
                        "cannot remove orphan journal {}: {error}",
                        entry.path().display()
                    ));
                }
            }
        }
    }
    sync_directory(&journal_dir)?;
    Ok(recovered)
}

fn publish_step(step: &Step) -> Result<(), String> {
    ensure_path_absent(&step.target, "publish target")?;
    match step.kind {
        EntryKind::File => {
            // A hard link is create-new on both Unix and Windows. It never
            // relies on rename-over-existing semantics and leaves a complete,
            // synced inode at the target name before the temp name is removed.
            // Filesystems without hard-link support (FAT32/exFAT, some
            // network mounts) fall back to a create-new rename: the target is
            // guaranteed absent here (it was moved to a backup or never
            // existed), so rename is just as atomic.
            match fs::hard_link(&step.temp, &step.target) {
                Ok(()) => {
                    sync_entry(&step.target, EntryKind::File)?;
                    fs::remove_file(&step.temp).map_err(|error| {
                        format!(
                            "cannot remove published temp {}: {error}",
                            step.temp.display()
                        )
                    })?;
                }
                Err(error)
                    if matches!(
                        error.kind(),
                        std::io::ErrorKind::Unsupported | std::io::ErrorKind::PermissionDenied
                    ) =>
                {
                    // Filesystems without hard-link support (FAT32/exFAT,
                    // some network mounts) report Unsupported or a
                    // permission-style error. Re-check that the target is
                    // still absent, then publish with a create-new rename,
                    // which is just as atomic on those filesystems.
                    ensure_path_absent(&step.target, "publish target")?;
                    fs::rename(&step.temp, &step.target).map_err(|rename_error| {
                        format!(
                            "cannot publish temp file {} as {} without overwrite \
                             (hard link: {error}, rename: {rename_error})",
                            step.temp.display(),
                            step.target.display()
                        )
                    })?;
                    sync_entry(&step.target, EntryKind::File)?;
                }
                Err(error) => {
                    return Err(format!(
                        "cannot publish temp file {} as {} without overwrite: {error}",
                        step.temp.display(),
                        step.target.display()
                    ))
                }
            }
            sync_parent(&step.target)
        }
        EntryKind::Directory => {
            fs::rename(&step.temp, &step.target).map_err(|error| {
                format!(
                    "cannot publish staged directory {} as {}: {error}",
                    step.temp.display(),
                    step.target.display()
                )
            })?;
            sync_parent(&step.target)
        }
    }
}

fn rollback_step(step: &Step) -> Result<(), String> {
    let mut errors = Vec::new();
    let target_exists = path_exists_no_follow(&step.target)?;
    let backup_exists = path_exists_no_follow(&step.backup)?;

    match step.original {
        OriginalState::Present if backup_exists => {
            if target_exists {
                match fingerprint_path(&step.target, step.kind) {
                    Ok(fingerprint) if fingerprint == step.fingerprint => {
                        if let Err(error) = remove_entry(&step.target, step.kind) {
                            errors.push(error);
                        }
                    }
                    Ok(_) => errors.push(format!(
                        "rollback refused to overwrite a concurrently modified target: {}",
                        step.target.display()
                    )),
                    Err(error) => errors.push(error),
                }
            }
            if !path_exists_no_follow(&step.target)? {
                if let Err(error) = fs::rename(&step.backup, &step.target) {
                    errors.push(format!(
                        "rollback failed to restore {} from {}: {error}",
                        step.target.display(),
                        step.backup.display()
                    ));
                } else if let Err(error) = sync_parent(&step.target) {
                    errors.push(error);
                }
            }
        }
        OriginalState::Present => {
            if !target_exists && step.state >= StepState::BackedUp {
                errors.push(format!(
                    "rollback cannot restore {} because its recorded backup is missing: {}",
                    step.target.display(),
                    step.backup.display()
                ));
            } else if target_exists
                && step.state >= StepState::Publishing
                && fingerprint_path(&step.target, step.kind).ok().as_deref()
                    == Some(step.fingerprint.as_str())
            {
                errors.push(format!(
                    "rollback cannot restore {} because its recorded backup is missing",
                    step.target.display()
                ));
            }
        }
        OriginalState::Absent => {
            if target_exists && step.state >= StepState::Publishing {
                match fingerprint_path(&step.target, step.kind) {
                    Ok(fingerprint) if fingerprint == step.fingerprint => {
                        if let Err(error) = remove_entry(&step.target, step.kind) {
                            errors.push(error);
                        }
                    }
                    Ok(_) => errors.push(format!(
                        "rollback refused to remove a concurrently created target: {}",
                        step.target.display()
                    )),
                    Err(error) => errors.push(error),
                }
            }
        }
        OriginalState::Unknown => {}
    }

    if path_exists_no_follow(&step.temp)? {
        match fingerprint_path(&step.temp, step.kind) {
            Ok(fingerprint) if fingerprint == step.fingerprint => {
                if let Err(error) = remove_entry(&step.temp, step.kind) {
                    errors.push(error);
                }
            }
            Ok(_) => errors.push(format!(
                "rollback refused to remove a modified temp entry: {}",
                step.temp.display()
            )),
            Err(error) => errors.push(error),
        }
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors.join("; "))
    }
}

fn inspect_original(path: &Path, kind: EntryKind) -> Result<OriginalState, String> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => {
            if metadata.file_type().is_symlink() {
                return Err(format!(
                    "transaction target is a symlink: {}",
                    path.display()
                ));
            }
            let matches = match kind {
                EntryKind::File => metadata.is_file(),
                EntryKind::Directory => metadata.is_dir(),
            };
            if !matches {
                return Err(format!(
                    "transaction target has the wrong entry type: {}",
                    path.display()
                ));
            }
            Ok(OriginalState::Present)
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(OriginalState::Absent),
        Err(error) => Err(format!("cannot inspect target {}: {error}", path.display())),
    }
}

fn reject_existing_kind_mismatch(path: &Path, kind: EntryKind) -> Result<(), String> {
    match inspect_original(path, kind) {
        Ok(_) => Ok(()),
        Err(error) => Err(error),
    }
}

fn validate_runtime_step(
    roots: &[PathBuf],
    transaction_id: &str,
    step: &Step,
) -> Result<(), String> {
    let target = relative_path(roots, step.root_index, &step.target)?;
    let temp = relative_path(roots, step.root_index, &step.temp)?;
    let backup = relative_path(roots, step.root_index, &step.backup)?;
    validate_step_path_names(
        &target,
        &temp,
        &backup,
        transaction_id,
        roots,
        step.root_index,
    )
}

fn ensure_fingerprint(step: &Step, path: &Path) -> Result<(), String> {
    let actual = fingerprint_path(path, step.kind)?;
    if actual == step.fingerprint {
        Ok(())
    } else {
        Err(format!(
            "staged entry changed during transaction: {}",
            path.display()
        ))
    }
}

fn step_to_journal(step: &Step, roots: &[PathBuf]) -> Result<JournalStep, String> {
    Ok(JournalStep {
        kind: step.kind,
        mode: step.mode,
        root_index: step.root_index,
        target: relative_path(roots, step.root_index, &step.target)?,
        temp: relative_path(roots, step.root_index, &step.temp)?,
        backup: relative_path(roots, step.root_index, &step.backup)?,
        fingerprint: step.fingerprint.clone(),
        original: step.original,
        state: step.state,
    })
}

fn journal_to_step(
    step: &JournalStep,
    index: usize,
    roots: &[PathBuf],
    transaction_id: &str,
) -> Result<Step, String> {
    let root = roots
        .get(step.root_index)
        .ok_or_else(|| format!("journal step {index} has an invalid trusted-root index"))?;
    let target = root.join(validate_relative_path(&step.target, "target")?);
    let temp = root.join(validate_relative_path(&step.temp, "temp")?);
    let backup = root.join(validate_relative_path(&step.backup, "backup")?);
    validate_step_path_names(
        &step.target,
        &step.temp,
        &step.backup,
        transaction_id,
        roots,
        step.root_index,
    )?;
    Ok(Step {
        kind: step.kind,
        mode: step.mode,
        root_index: step.root_index,
        target,
        temp,
        backup,
        fingerprint: step.fingerprint.clone(),
        original: step.original,
        state: step.state,
    })
}

fn validate_journal_document(
    path: &Path,
    document: &JournalDocument,
    expected_roots: &[String],
    roots: &[PathBuf],
) -> Result<(), String> {
    if document.version != JOURNAL_VERSION {
        return Err(format!(
            "journal {} uses unsupported version {}",
            path.display(),
            document.version
        ));
    }
    validate_transaction_id(&document.transaction_id)?;
    let expected_name = journal_file_name(&document.transaction_id, document.revision);
    if path.file_name().and_then(|name| name.to_str()) != Some(expected_name.as_str()) {
        return Err(format!(
            "journal filename does not match its identity: {}",
            path.display()
        ));
    }
    if document.roots != expected_roots {
        return Err(format!(
            "journal {} is outside the caller's trusted transaction roots",
            path.display()
        ));
    }
    let mut targets = HashSet::new();
    for (index, step) in document.steps.iter().enumerate() {
        if step.fingerprint.len() != 64
            || !step
                .fingerprint
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit())
        {
            return Err(format!("journal step {index} has an invalid fingerprint"));
        }
        let rebuilt = journal_to_step(step, index, roots, &document.transaction_id)?;
        if !targets.insert(rebuilt.target) {
            return Err(format!("journal contains duplicate target at step {index}"));
        }
    }
    Ok(())
}

fn validate_step_path_names(
    target: &str,
    temp: &str,
    backup: &str,
    transaction_id: &str,
    roots: &[PathBuf],
    root_index: usize,
) -> Result<(), String> {
    let root = roots
        .get(root_index)
        .ok_or_else(|| "journal references an unknown trusted root".to_string())?;
    let target_rel = validate_relative_path(target, "target")?;
    let temp_rel = validate_relative_path(temp, "temp")?;
    let backup_rel = validate_relative_path(backup, "backup")?;
    if target_rel.parent() != temp_rel.parent() || target_rel.parent() != backup_rel.parent() {
        return Err("journal temp/backup must be siblings of their target".to_string());
    }
    let target_name = target_rel
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| "journal target name is not valid UTF-8".to_string())?;
    let temp_name = temp_rel
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| "journal temp name is not valid UTF-8".to_string())?;
    let backup_name = backup_rel
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| "journal backup name is not valid UTF-8".to_string())?;
    let temp_prefix = format!(".{target_name}.seattrellis-tmp-{transaction_id}-");
    let backup_prefix = format!(".{target_name}.seattrellis-backup-{transaction_id}-");
    if !temp_name.starts_with(&temp_prefix) || !backup_name.starts_with(&backup_prefix) {
        return Err("journal temp/backup names do not match the transaction identity".to_string());
    }
    let parent = target_rel.parent().unwrap_or_else(|| Path::new(""));
    let parent_abs = root.join(parent);
    let canonical_parent = canonical_directory(&parent_abs, "journal target parent")?;
    if !canonical_parent.starts_with(root) {
        return Err("journal path escapes its trusted root through a symlink".to_string());
    }
    Ok(())
}

fn relative_path(roots: &[PathBuf], root_index: usize, path: &Path) -> Result<String, String> {
    let root = roots
        .get(root_index)
        .ok_or_else(|| "transaction references an unknown trusted root".to_string())?;
    let relative = path.strip_prefix(root).map_err(|_| {
        format!(
            "transaction path {} escapes trusted root {}",
            path.display(),
            root.display()
        )
    })?;
    let text = path_as_utf8(relative, "transaction-relative path")?;
    validate_relative_path(text, "transaction-relative path")?;
    Ok(text.to_string())
}

fn validate_relative_path<'a>(value: &'a str, label: &str) -> Result<&'a Path, String> {
    if value.is_empty() || value.contains('\0') || value.contains('\\') {
        return Err(format!("journal {label} is not a safe relative path"));
    }
    if value.len() >= 2 && value.as_bytes()[1] == b':' {
        return Err(format!(
            "journal {label} looks like an absolute Windows path"
        ));
    }
    let path = Path::new(value);
    if path.is_absolute() {
        return Err(format!("journal {label} must be relative"));
    }
    if path
        .components()
        .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(format!("journal {label} contains a non-normal component"));
    }
    Ok(path)
}

fn resolve_target(roots: &[PathBuf], target: &Path) -> Result<(usize, PathBuf), String> {
    let parent = target
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let canonical_parent = canonical_directory(parent, "transaction target parent")?;
    let name = target
        .file_name()
        .ok_or_else(|| format!("target has no file name: {}", target.display()))?;
    if name.to_str().is_none() {
        return Err(format!(
            "target name is not valid UTF-8: {}",
            target.display()
        ));
    }
    let target = canonical_parent.join(name);
    let root_index = most_specific_root(roots, &canonical_parent).ok_or_else(|| {
        format!(
            "transaction target is outside every trusted root: {}",
            target.display()
        )
    })?;
    Ok((root_index, target))
}

fn resolve_existing_entry(
    roots: &[PathBuf],
    path: &Path,
    kind: EntryKind,
) -> Result<(usize, PathBuf), String> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| format!("cannot inspect staged entry {}: {error}", path.display()))?;
    if metadata.file_type().is_symlink() {
        return Err(format!("staged entry is a symlink: {}", path.display()));
    }
    let matches = match kind {
        EntryKind::File => metadata.is_file(),
        EntryKind::Directory => metadata.is_dir(),
    };
    if !matches {
        return Err(format!(
            "staged entry has the wrong type: {}",
            path.display()
        ));
    }
    let canonical = fs::canonicalize(path)
        .map_err(|error| format!("cannot resolve staged entry {}: {error}", path.display()))?;
    let root_index = most_specific_root(roots, &canonical).ok_or_else(|| {
        format!(
            "staged entry is outside every trusted root: {}",
            canonical.display()
        )
    })?;
    Ok((root_index, canonical))
}

fn most_specific_root(roots: &[PathBuf], path: &Path) -> Option<usize> {
    roots
        .iter()
        .enumerate()
        .filter(|(_, root)| path.starts_with(root))
        .max_by_key(|(_, root)| root.components().count())
        .map(|(index, _)| index)
}

fn canonical_roots(roots: &[PathBuf]) -> Result<Vec<PathBuf>, String> {
    if roots.is_empty() {
        return Err("recovery requires at least one trusted root".to_string());
    }
    let mut canonical = Vec::with_capacity(roots.len());
    for root in roots {
        let root = canonical_directory(root, "trusted transaction root")?;
        if !canonical.contains(&root) {
            canonical.push(root);
        }
    }
    Ok(canonical)
}

fn canonical_directory(path: &Path, label: &str) -> Result<PathBuf, String> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| format!("cannot inspect {label} {}: {error}", path.display()))?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(format!(
            "{label} is not a real directory: {}",
            path.display()
        ));
    }
    fs::canonicalize(path)
        .map_err(|error| format!("cannot resolve {label} {}: {error}", path.display()))
}

fn prepare_journal_dir(path: &Path) -> Result<(), String> {
    #[cfg(unix)]
    let existed = path.exists();
    fs::create_dir_all(path)
        .map_err(|error| format!("cannot create journal dir {}: {error}", path.display()))?;
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| format!("cannot inspect journal dir {}: {error}", path.display()))?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(format!(
            "journal path is not a real directory: {}",
            path.display()
        ));
    }
    #[cfg(unix)]
    if !existed {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))
            .map_err(|error| format!("cannot restrict journal dir {}: {error}", path.display()))?;
    }
    sync_parent(path)?;
    sync_directory(path)
}

fn sibling_transaction_path(
    target: &Path,
    purpose: &str,
    transaction_id: &str,
    step_index: usize,
    suffix: &str,
) -> Result<PathBuf, String> {
    let name = target
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| format!("target name is not valid UTF-8: {}", target.display()))?;
    let parent = target
        .parent()
        .ok_or_else(|| format!("target has no parent: {}", target.display()))?;
    Ok(parent.join(format!(
        ".{name}.seattrellis-{purpose}-{transaction_id}-{step_index}.{suffix}"
    )))
}

fn ensure_path_absent(path: &Path, label: &str) -> Result<(), String> {
    match fs::symlink_metadata(path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Ok(_) => Err(format!(
            "{label} path already exists; refusing to overwrite it: {}",
            path.display()
        )),
        Err(error) => Err(format!(
            "cannot inspect {label} {}: {error}",
            path.display()
        )),
    }
}

fn path_exists_no_follow(path: &Path) -> Result<bool, String> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => {
            if metadata.file_type().is_symlink() {
                Err(format!(
                    "transaction path became a symlink: {}",
                    path.display()
                ))
            } else {
                Ok(true)
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(format!(
            "cannot inspect transaction path {}: {error}",
            path.display()
        )),
    }
}

fn remove_entry(path: &Path, kind: EntryKind) -> Result<(), String> {
    let result = match kind {
        EntryKind::File => fs::remove_file(path),
        EntryKind::Directory => fs::remove_dir_all(path),
    };
    result.map_err(|error| {
        format!(
            "cannot remove transaction entry {}: {error}",
            path.display()
        )
    })?;
    sync_parent(path)
}

fn fingerprint_path(path: &Path, expected_kind: EntryKind) -> Result<String, String> {
    let mut hasher = Sha256::new();
    hash_entry(path, Path::new(""), expected_kind, &mut hasher)?;
    Ok(hex_digest(hasher.finalize().as_slice()))
}

fn hash_entry(
    path: &Path,
    relative: &Path,
    expected_kind: EntryKind,
    hasher: &mut Sha256,
) -> Result<(), String> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| format!("cannot fingerprint {}: {error}", path.display()))?;
    if metadata.file_type().is_symlink() {
        return Err(format!("cannot fingerprint symlink: {}", path.display()));
    }
    hash_relative_name(relative, hasher)?;
    match expected_kind {
        EntryKind::File if metadata.is_file() => {
            hasher.update(b"file\0");
            hasher.update(metadata.len().to_le_bytes());
            let mut file = File::open(path)
                .map_err(|error| format!("cannot fingerprint {}: {error}", path.display()))?;
            let mut buffer = [0_u8; 64 * 1024];
            loop {
                let read = file
                    .read(&mut buffer)
                    .map_err(|error| format!("cannot fingerprint {}: {error}", path.display()))?;
                if read == 0 {
                    break;
                }
                hasher.update(&buffer[..read]);
            }
            Ok(())
        }
        EntryKind::Directory if metadata.is_dir() => {
            hasher.update(b"directory\0");
            let mut entries = fs::read_dir(path)
                .map_err(|error| format!("cannot fingerprint {}: {error}", path.display()))?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|error| format!("cannot fingerprint {}: {error}", path.display()))?;
            entries.sort_by_key(|entry| entry.file_name());
            for entry in entries {
                let child_name = entry.file_name();
                if child_name.to_str().is_none() {
                    return Err(format!(
                        "cannot fingerprint non-UTF-8 entry under {}",
                        path.display()
                    ));
                }
                let child_relative = relative.join(child_name);
                let child_path = entry.path();
                let child_metadata = fs::symlink_metadata(&child_path).map_err(|error| {
                    format!("cannot fingerprint {}: {error}", child_path.display())
                })?;
                let child_kind = if child_metadata.is_file() {
                    EntryKind::File
                } else if child_metadata.is_dir() {
                    EntryKind::Directory
                } else {
                    return Err(format!(
                        "cannot fingerprint special entry: {}",
                        child_path.display()
                    ));
                };
                hash_entry(&child_path, &child_relative, child_kind, hasher)?;
            }
            Ok(())
        }
        _ => Err(format!(
            "transaction entry changed type: {}",
            path.display()
        )),
    }
}

fn hash_relative_name(path: &Path, hasher: &mut Sha256) -> Result<(), String> {
    let text = path_as_utf8(path, "fingerprint path")?;
    hasher.update((text.len() as u64).to_le_bytes());
    hasher.update(text.as_bytes());
    Ok(())
}

fn hex_digest(bytes: &[u8]) -> String {
    let mut result = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        use std::fmt::Write as _;
        let _ = write!(result, "{byte:02x}");
    }
    result
}

fn sync_entry(path: &Path, kind: EntryKind) -> Result<(), String> {
    match kind {
        EntryKind::File => {
            // FlushFileBuffers on Windows requires a writable handle; a
            // read-only handle fails with ERROR_ACCESS_DENIED. Files this
            // module creates are writable, so the writable open is the
            // primary path; read-only originals are synced best-effort.
            let mut options = OpenOptions::new();
            options.read(true).write(true);
            match options.open(path).and_then(|file| file.sync_all()) {
                Ok(()) => Ok(()),
                Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => {
                    match File::open(path).and_then(|file| file.sync_all()) {
                        Ok(()) => Ok(()),
                        // Windows cannot flush read-only handles; NTFS
                        // journals the directory rename that follows anyway.
                        #[cfg(windows)]
                        Err(_) => Ok(()),
                        #[cfg(not(windows))]
                        Err(read_error) => Err(format!(
                            "cannot sync read-only file {}: {read_error} (write flush: {error})",
                            path.display()
                        )),
                    }
                }
                Err(error) => Err(format!("cannot sync file {}: {error}", path.display())),
            }
        }
        EntryKind::Directory => sync_tree(path),
    }
}

fn sync_tree(path: &Path) -> Result<(), String> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| format!("cannot sync {}: {error}", path.display()))?;
    if metadata.file_type().is_symlink() {
        return Err(format!("refusing to sync symlink: {}", path.display()));
    }
    if metadata.is_file() {
        return sync_entry(path, EntryKind::File);
    }
    if !metadata.is_dir() {
        return Err(format!(
            "refusing to sync special entry: {}",
            path.display()
        ));
    }
    let entries = fs::read_dir(path)
        .map_err(|error| format!("cannot list directory {} for sync: {error}", path.display()))?;
    for entry in entries {
        let entry = entry
            .map_err(|error| format!("cannot inspect directory {}: {error}", path.display()))?;
        sync_tree(&entry.path())?;
    }
    sync_directory(path)
}

fn sync_parent(path: &Path) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| format!("path has no parent directory: {}", path.display()))?;
    sync_directory(parent)
}

#[cfg(unix)]
fn sync_directory(path: &Path) -> Result<(), String> {
    File::open(path)
        .and_then(|directory| directory.sync_all())
        .map_err(|error| format!("cannot sync directory {}: {error}", path.display()))
}

#[cfg(windows)]
fn sync_directory(path: &Path) -> Result<(), String> {
    use std::os::windows::fs::OpenOptionsExt;
    const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;
    match OpenOptions::new()
        .read(true)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS)
        .open(path)
        .and_then(|directory| directory.sync_all())
    {
        Ok(()) => Ok(()),
        // FlushFileBuffers is not supported on directory handles on many
        // Windows versions and filesystems (ERROR_ACCESS_DENIED / not
        // supported). NTFS journals directory metadata operations itself,
        // so directory fsync is best-effort on Windows; files themselves
        // are still synced individually before publication.
        Err(_) => Ok(()),
    }
}

fn write_new_journal(
    journal_dir: &Path,
    final_path: &Path,
    document: &JournalDocument,
) -> Result<(), String> {
    ensure_path_absent(final_path, "journal revision")?;
    let pending = final_path.with_extension("json.pending");
    ensure_path_absent(&pending, "pending journal revision")?;
    let bytes = serde_json::to_vec(document)
        .map_err(|error| format!("cannot serialize transaction journal: {error}"))?;
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&pending)
        .map_err(|error| format!("cannot create journal {}: {error}", pending.display()))?;
    file.write_all(&bytes)
        .and_then(|_| file.sync_all())
        .map_err(|error| format!("cannot write journal {}: {error}", pending.display()))?;
    drop(file);
    sync_directory(journal_dir)?;
    fs::rename(&pending, final_path).map_err(|error| {
        format!(
            "cannot publish journal {} as {}: {error}",
            pending.display(),
            final_path.display()
        )
    })?;
    sync_directory(journal_dir)
}

fn cleanup_journal_files(journal_dir: &Path, transaction_id: &str) -> Result<(), String> {
    let prefix = format!("{JOURNAL_PREFIX}{transaction_id}-r");
    let entries = fs::read_dir(journal_dir)
        .map_err(|error| format!("cannot list journal dir {}: {error}", journal_dir.display()))?;
    let mut errors = Vec::new();
    for entry in entries {
        match entry {
            Ok(entry) => {
                let name = entry.file_name().to_string_lossy().into_owned();
                if name.starts_with(&prefix)
                    && (name.ends_with(JOURNAL_SUFFIX) || name.ends_with(".json.pending"))
                {
                    if let Err(error) = fs::remove_file(entry.path()) {
                        if error.kind() != std::io::ErrorKind::NotFound {
                            errors.push(format!(
                                "cannot remove journal {}: {error}",
                                entry.path().display()
                            ));
                        }
                    }
                }
            }
            Err(error) => errors.push(format!(
                "cannot inspect journal dir {}: {error}",
                journal_dir.display()
            )),
        }
    }
    if let Err(error) = sync_directory(journal_dir) {
        errors.push(error);
    }
    // The journal directory itself is intentionally kept (even when empty):
    // it is the recovery anchor for this process group, and removing it
    // races with concurrent transactions sharing the same parent.
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors.join("; "))
    }
}

fn next_transaction_id() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    let counter = TRANSACTION_COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("{}-{nanos}-{counter}", std::process::id())
}

fn validate_transaction_id(value: &str) -> Result<(), String> {
    if value.is_empty()
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || byte == b'-')
    {
        Err("journal transaction_id contains unsafe characters".to_string())
    } else {
        Ok(())
    }
}

fn journal_file_name(transaction_id: &str, revision: u64) -> String {
    format!("{JOURNAL_PREFIX}{transaction_id}-r{revision}{JOURNAL_SUFFIX}")
}

fn path_as_utf8<'a>(path: &'a Path, label: &str) -> Result<&'a str, String> {
    path.to_str()
        .ok_or_else(|| format!("{label} is not valid UTF-8: {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

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

    fn backup_files(dir: &Path) -> Vec<PathBuf> {
        fs::read_dir(dir)
            .unwrap()
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| path.to_string_lossy().contains(".seattrellis-backup-"))
            .collect()
    }

    #[test]
    fn commit_replaces_all_targets_with_unique_non_overwriting_backups() {
        let dir = temp_dir("commit");
        let journal = dir.join("journal");
        let a = dir.join("a.json");
        let b = dir.join("b.json");
        fs::write(&a, "{\"old\":true}").unwrap();
        fs::write(&b, "{\"old\":true}").unwrap();
        let legacy_backup = dir.join("a.json.bak");
        fs::write(&legacy_backup, "legacy backup").unwrap();

        let mut txn = FileTransaction::begin_with_root(&journal, &dir).unwrap();
        txn.stage(&a, b"{\"new\":true}").unwrap();
        txn.stage(&b, b"{\"new\":true}").unwrap();
        let receipt = txn.commit_with_receipt(json_validator).unwrap();

        assert_eq!(fs::read_to_string(&a).unwrap(), "{\"new\":true}");
        assert_eq!(fs::read_to_string(&b).unwrap(), "{\"new\":true}");
        assert_eq!(fs::read_to_string(&legacy_backup).unwrap(), "legacy backup");
        assert_eq!(receipt.backups.len(), 2);
        assert_ne!(receipt.backups[0].1, legacy_backup);
        assert!(receipt.backups.iter().all(|(_, backup)| backup.is_file()));
        assert_eq!(
            recover_leftover_transactions_with_root(&journal, &dir).unwrap(),
            0
        );
    }

    #[test]
    fn repeated_commits_keep_every_old_backup() {
        let dir = temp_dir("old-backups");
        let target = dir.join("value.json");
        fs::write(&target, b"{\"version\":0}").unwrap();
        let first = atomic_write_file(&target, b"{\"version\":1}").unwrap();
        let second = atomic_write_file(&target, b"{\"version\":2}").unwrap();
        assert_ne!(first.backups[0].1, second.backups[0].1);
        assert_eq!(
            fs::read_to_string(&first.backups[0].1).unwrap(),
            "{\"version\":0}"
        );
        assert_eq!(
            fs::read_to_string(&second.backups[0].1).unwrap(),
            "{\"version\":1}"
        );
    }

    #[test]
    fn validation_failure_touches_nothing() {
        let dir = temp_dir("validate");
        let target = dir.join("a.json");
        fs::write(&target, "{\"old\":true}").unwrap();
        let error = atomic_write_file_validated(&target, b"not json", json_validator).unwrap_err();
        assert!(error.contains("not valid JSON"));
        assert_eq!(fs::read_to_string(&target).unwrap(), "{\"old\":true}");
        assert!(backup_files(&dir).is_empty());
    }

    #[test]
    fn batch_failure_rolls_back_every_published_target() {
        let dir = temp_dir("batch-rollback");
        let journal = dir.join("journal");
        let a = dir.join("a.json");
        let b = dir.join("b.json");
        fs::write(&a, b"{\"old\":1}").unwrap();
        fs::write(&b, b"{\"old\":2}").unwrap();
        let mut txn = FileTransaction::begin_with_root(&journal, &dir).unwrap();
        txn.stage(&a, b"{\"new\":1}").unwrap();
        txn.stage(&b, b"{\"new\":2}").unwrap();
        // Inject a failure immediately before the second publish. The first
        // file has already become visible, so this exercises real batch
        // rollback rather than only staged validation.
        let result = txn.commit_inner_with_publisher(&json_validator, &|index, step| {
            if index == 1 {
                Err("injected failure before second publish".to_string())
            } else {
                publish_step(step)
            }
        });
        let error = match result {
            Ok(_) => panic!("failure injection unexpectedly committed"),
            Err(commit_error) => match txn.rollback_inner() {
                Ok(()) => commit_error,
                Err(rollback_error) => {
                    format!("{commit_error}; rollback also failed: {rollback_error}")
                }
            },
        };
        assert!(error.contains("injected failure"));
        assert_eq!(fs::read_to_string(&a).unwrap(), "{\"old\":1}");
        assert_eq!(fs::read_to_string(&b).unwrap(), "{\"old\":2}");
    }

    #[test]
    fn commit_reports_final_validation_and_rollback_failure_together() {
        let dir = temp_dir("combined-error");
        let journal = dir.join("journal");
        let target = dir.join("a.json");
        fs::write(&target, b"{\"old\":true}").unwrap();
        let mut txn = FileTransaction::begin_with_root(&journal, &dir).unwrap();
        txn.stage(&target, b"{\"new\":true}").unwrap();
        let calls = std::cell::Cell::new(0_u8);
        let error = txn
            .commit(|path| {
                calls.set(calls.get() + 1);
                if calls.get() == 2 {
                    fs::write(path, b"{\"concurrent\":true}").unwrap();
                    Err("final validation injected failure".to_string())
                } else {
                    Ok(())
                }
            })
            .unwrap_err();
        assert!(error.contains("final validation injected failure"));
        assert!(error.contains("rollback also failed"));
        assert!(error.contains("concurrently modified"));
    }

    #[test]
    fn create_new_never_overwrites_an_existing_target() {
        let dir = temp_dir("create-new");
        let target = dir.join("result.json");
        fs::write(&target, b"original").unwrap();
        let error = atomic_create_file(&target, b"replacement").unwrap_err();
        assert!(error.contains("create-new"));
        assert_eq!(fs::read(&target).unwrap(), b"original");
    }

    #[test]
    fn recovery_restores_a_crash_after_backup_before_publish() {
        let dir = temp_dir("crash-backup");
        let journal = dir.join("journal");
        let target = dir.join("a.json");
        fs::write(&target, b"{\"old\":true}").unwrap();
        let mut txn = FileTransaction::begin_with_root(&journal, &dir).unwrap();
        txn.stage(&target, b"{\"new\":true}").unwrap();

        // Drive the internal state to the first crash window: journal says
        // Prepared and the original has moved to the recorded backup.
        txn.steps[0].original = OriginalState::Present;
        txn.steps[0].state = StepState::Prepared;
        txn.write_journal_revision().unwrap();
        fs::rename(&txn.steps[0].target, &txn.steps[0].backup).unwrap();
        sync_parent(&txn.steps[0].target).unwrap();
        std::mem::forget(txn);

        assert!(!target.exists());
        assert_eq!(
            recover_leftover_transactions_with_root(&journal, &dir).unwrap(),
            1
        );
        assert_eq!(fs::read_to_string(&target).unwrap(), "{\"old\":true}");
    }

    #[test]
    fn recovery_removes_a_crash_published_new_file() {
        let dir = temp_dir("crash-new");
        let journal = dir.join("journal");
        let target = dir.join("new.json");
        let mut txn = FileTransaction::begin_with_root(&journal, &dir).unwrap();
        txn.stage_new(&target, b"{\"new\":true}").unwrap();
        txn.steps[0].original = OriginalState::Absent;
        txn.steps[0].state = StepState::Publishing;
        txn.write_journal_revision().unwrap();
        publish_step(&txn.steps[0]).unwrap();
        std::mem::forget(txn);

        assert!(target.exists());
        assert_eq!(
            recover_leftover_transactions_with_root(&journal, &dir).unwrap(),
            1
        );
        assert!(!target.exists());
    }

    #[test]
    fn recovery_rejects_malformed_and_escaping_journals_without_touching_victim() {
        let dir = temp_dir("malicious");
        let journal = dir.join("journal");
        fs::create_dir_all(&journal).unwrap();
        let victim = dir
            .parent()
            .unwrap()
            .join(format!("victim-{}", next_transaction_id()));
        fs::write(&victim, b"keep").unwrap();
        let transaction_id = next_transaction_id();
        let path = journal.join(journal_file_name(&transaction_id, 0));
        let document = serde_json::json!({
            "version": JOURNAL_VERSION,
            "transaction_id": transaction_id,
            "revision": 0,
            "roots": [fs::canonicalize(&dir).unwrap().to_str().unwrap()],
            "phase": "active",
            "steps": [{
                "kind": "file",
                "mode": "replace",
                "root_index": 0,
                "target": "../victim",
                "temp": "../.victim.seattrellis-tmp-x-0.tmp",
                "backup": "../.victim.seattrellis-backup-x-0.bak",
                "fingerprint": "0".repeat(64),
                "original": "present",
                "state": "published"
            }]
        });
        fs::write(&path, serde_json::to_vec(&document).unwrap()).unwrap();
        let error = recover_leftover_transactions_with_root(&journal, &dir).unwrap_err();
        assert!(error.contains("non-normal component"), "got: {error}");
        assert_eq!(fs::read(&victim).unwrap(), b"keep");
        assert!(
            path.exists(),
            "a rejected journal must remain for inspection"
        );
    }

    #[test]
    fn recovery_rejects_root_mismatch_before_any_mutation() {
        let dir = temp_dir("wrong-root");
        let other = temp_dir("wrong-root-other");
        let journal = dir.join("journal");
        let target = dir.join("a.json");
        fs::write(&target, b"old").unwrap();
        let mut txn = FileTransaction::begin_with_root(&journal, &dir).unwrap();
        txn.stage(&target, b"new").unwrap();
        std::mem::forget(txn);

        let error = recover_leftover_transactions_with_root(&journal, &other).unwrap_err();
        assert!(error.contains("trusted transaction roots"));
        assert_eq!(fs::read(&target).unwrap(), b"old");
    }

    #[test]
    fn directory_publish_is_atomic_and_keeps_old_tree_as_backup() {
        let dir = temp_dir("directory");
        let journal = dir.join("journal");
        let target = dir.join("project");
        let staging = dir.join("staging");
        fs::create_dir_all(&target).unwrap();
        fs::write(target.join("old.txt"), b"old").unwrap();
        fs::create_dir_all(&staging).unwrap();
        fs::write(staging.join("new.txt"), b"new").unwrap();
        let mut txn = FileTransaction::begin_with_root(&journal, &dir).unwrap();
        txn.stage_directory(&target, &staging).unwrap();
        let receipt = txn
            .commit_with_receipt(|path| {
                if path.join("new.txt").is_file() {
                    Ok(())
                } else {
                    Err("missing staged content".to_string())
                }
            })
            .unwrap();
        assert_eq!(fs::read(target.join("new.txt")).unwrap(), b"new");
        assert_eq!(
            fs::read(receipt.backups[0].1.join("old.txt")).unwrap(),
            b"old"
        );
    }

    /// Regression test for the migration integration: the journal directory
    /// lives under the system temp dir while the targets live in the user's
    /// project tree. `begin`'s default root (the journal's parent) must not
    /// silently reject those targets; callers stage them via explicit roots.
    #[test]
    fn explicit_roots_allow_targets_outside_the_journal_parent() {
        let journal_parent = temp_dir("roots-journal");
        let project_dir = temp_dir("roots-project");
        let journal = journal_parent.join("journal");
        let target = project_dir.join("project.json");
        fs::write(&target, b"{\"old\":true}").unwrap();

        // The default `begin` trusts only the journal's parent: staging a
        // target in another tree is refused instead of silently allowing a
        // path escape.
        let mut default_txn = FileTransaction::begin(&journal).unwrap();
        let error = default_txn.stage(&target, b"new").unwrap_err();
        assert!(
            error.contains("outside every trusted root"),
            "unexpected: {error}"
        );
        drop(default_txn);

        // Migration-style usage passes the target parents as explicit roots.
        let mut txn =
            FileTransaction::begin_with_roots(&journal, &[target.parent().unwrap().to_path_buf()])
                .unwrap();
        txn.stage(&target, b"{\"new\":true}").unwrap();
        let receipt = txn.commit_with_receipt(json_validator).unwrap();
        assert_eq!(receipt.backups.len(), 1);
        assert_eq!(fs::read_to_string(&target).unwrap(), "{\"new\":true}");
        // Recovery also needs the explicit roots to authorize the journal.
        let recovered = recover_leftover_transactions_with_roots(
            &journal,
            &[target.parent().unwrap().to_path_buf()],
        )
        .unwrap();
        assert_eq!(recovered, 0, "committed journal should be cleaned already");
    }
}
