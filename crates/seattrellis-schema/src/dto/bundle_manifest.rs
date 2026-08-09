//! `ProjectBundleManifest` v2 payload: per-file integrity (M2-05).
//!
//! The v1 bundle manifest (src/seattrellis/project_bundle.py) lists file
//! names only; the v2 manifest records size + SHA-256 per entry so bundle
//! integrity is verifiable without trusting the archive. Entries are
//! validated for path safety (no absolute paths, no `..` escapes — the
//! zip-slip-style defense is enforced at the manifest level too).

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sha2::Digest;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ProjectBundleManifest {
    /// The bundle-relative path of the project file itself.
    pub project_file: String,
    #[serde(default)]
    pub include_outputs: bool,
    /// Every file in the bundle with its integrity record.
    pub files: Vec<BundleFileEntry>,
}

/// One bundle entry: path + integrity + (optional) artifact identity.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BundleFileEntry {
    /// POSIX relative path inside the bundle (must be safe to join).
    pub path: String,
    /// Exact byte size of the file.
    pub size: u64,
    /// Lowercase hex SHA-256 of the file contents.
    pub sha256: String,
    /// Recognized artifact kind, when the file is a SeatTrellis artifact.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    /// Artifact schema version, when `kind` is set.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<u32>,
}

/// One integrity or safety problem found while verifying a manifest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ManifestIssue {
    pub entry: String,
    pub problem: String,
}

/// A manifest entry path is safe when it is relative, has no `..` segments,
/// no absolute prefixes and no NUL bytes.
pub fn is_safe_entry_path(path: &str) -> bool {
    if path.is_empty()
        || path.contains('\0')
        || path.starts_with('/')
        || path.starts_with('\\')
        || path.split('/').any(|segment| segment == "..")
    {
        return false;
    }
    true
}

/// Verify a manifest against the actual files under `base_dir`: every entry
/// must exist, have the recorded size and the recorded SHA-256. Returns the
/// list of problems (empty = fully verified).
pub fn verify_manifest(
    manifest: &ProjectBundleManifest,
    base_dir: &std::path::Path,
) -> Vec<ManifestIssue> {
    let mut issues = Vec::new();
    for entry in &manifest.files {
        if !is_safe_entry_path(&entry.path) {
            issues.push(ManifestIssue {
                entry: entry.path.clone(),
                problem: "unsafe entry path (absolute or escaping the bundle)".into(),
            });
            continue;
        }
        let path = base_dir.join(&entry.path);
        let metadata = match std::fs::metadata(&path) {
            Ok(metadata) => metadata,
            Err(_) => {
                issues.push(ManifestIssue {
                    entry: entry.path.clone(),
                    problem: "file missing".into(),
                });
                continue;
            }
        };
        if metadata.len() != entry.size {
            issues.push(ManifestIssue {
                entry: entry.path.clone(),
                problem: format!(
                    "size mismatch: expected {} bytes, found {}",
                    entry.size,
                    metadata.len()
                ),
            });
            continue;
        }
        match std::fs::read(&path) {
            Ok(bytes) => {
                let digest = sha2::Sha256::digest(&bytes);
                let hex: String = digest.iter().map(|byte| format!("{byte:02x}")).collect();
                if !hex.eq_ignore_ascii_case(&entry.sha256) {
                    issues.push(ManifestIssue {
                        entry: entry.path.clone(),
                        problem: "sha256 mismatch (content tampered)".into(),
                    });
                }
            }
            Err(_) => issues.push(ManifestIssue {
                entry: entry.path.clone(),
                problem: "file unreadable".into(),
            }),
        }
    }
    issues
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;

    fn sha256_hex(bytes: &[u8]) -> String {
        let digest = <sha2::Sha256 as sha2::Digest>::digest(bytes);
        digest.iter().map(|byte| format!("{byte:02x}")).collect()
    }

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "seattrellis-bundle-test-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn round_trips() {
        let manifest = ProjectBundleManifest {
            project_file: "seattrellis.project.json".into(),
            include_outputs: false,
            files: vec![BundleFileEntry {
                path: "students.json".into(),
                size: 3,
                sha256: sha256_hex(b"abc"),
                kind: Some("student_roster".into()),
                version: Some(2),
            }],
        };
        let json = serde_json::to_string(&manifest).unwrap();
        let parsed: ProjectBundleManifest = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, manifest);
    }

    #[test]
    fn verifies_intact_bundles() {
        let dir = temp_dir("intact");
        fs::write(dir.join("a.json"), b"hello").unwrap();
        fs::write(dir.join("b.json"), b"world").unwrap();
        let manifest = ProjectBundleManifest {
            project_file: "a.json".into(),
            include_outputs: false,
            files: vec![
                BundleFileEntry {
                    path: "a.json".into(),
                    size: 5,
                    sha256: sha256_hex(b"hello"),
                    kind: None,
                    version: None,
                },
                BundleFileEntry {
                    path: "b.json".into(),
                    size: 5,
                    sha256: sha256_hex(b"world"),
                    kind: None,
                    version: None,
                },
            ],
        };
        assert!(verify_manifest(&manifest, &dir).is_empty());
    }

    #[test]
    fn detects_tampering_missing_files_and_size_mismatch() {
        let dir = temp_dir("tamper");
        fs::write(dir.join("a.json"), b"HELLO").unwrap(); // tampered content
        fs::write(dir.join("b.json"), b"world").unwrap();
        let manifest = ProjectBundleManifest {
            project_file: "a.json".into(),
            include_outputs: false,
            files: vec![
                BundleFileEntry {
                    path: "a.json".into(),
                    size: 5,
                    sha256: sha256_hex(b"hello"),
                    kind: None,
                    version: None,
                },
                BundleFileEntry {
                    path: "b.json".into(),
                    size: 999,
                    sha256: sha256_hex(b"world"),
                    kind: None,
                    version: None,
                },
                BundleFileEntry {
                    path: "gone.json".into(),
                    size: 1,
                    sha256: sha256_hex(b"x"),
                    kind: None,
                    version: None,
                },
            ],
        };
        let issues = verify_manifest(&manifest, &dir);
        let problems: Vec<&str> = issues.iter().map(|issue| issue.problem.as_str()).collect();
        assert!(problems.iter().any(|p| p.contains("sha256 mismatch")));
        assert!(problems.iter().any(|p| p.contains("size mismatch")));
        assert!(problems.iter().any(|p| p.contains("missing")));
    }

    #[test]
    fn unsafe_entry_paths_are_rejected() {
        assert!(!is_safe_entry_path("/etc/passwd"));
        assert!(!is_safe_entry_path("../escape.json"));
        assert!(!is_safe_entry_path("a/../../b"));
        assert!(!is_safe_entry_path(""));
        assert!(!is_safe_entry_path("bad\0name"));
        assert!(is_safe_entry_path("project/students.json"));
        assert!(is_safe_entry_path("中文目录/名单.csv"));
    }

    #[test]
    fn unknown_manifest_fields_are_rejected() {
        let json = r#"{"project_file":"p.json","files":[],"mystery":1}"#;
        assert!(serde_json::from_str::<ProjectBundleManifest>(json).is_err());
    }
}
