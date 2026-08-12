//! Exhaustive-ish argument-combination sweep for the hand-written CLI parser
//! (alpha.2 gap: "CLI 参数组合全量枚举"). Spawns the real binary via
//! `CARGO_BIN_EXE_seattrellis_cli` for every command and asserts:
//!
//! * usage/argument errors are graceful: frozen exit code 2 (M1-03 table
//!   0/2/3/4/5/70/130; usage errors are InvalidInput), an `error:` message
//!   on stderr, and never a panic;
//! * valid minimal invocations exit 0 with empty stderr;
//! * garbage input files fail fast with a clean error and a frozen exit code.
//!
//! Runtime is bounded by ~270 short-lived subprocess spawns (a few seconds
//! in a debug build). One small solve request (3 students / 3 seats) is
//! reused across solve/candidates/score/audit/repair/edit; the project
//! commands reuse one tiny 4-student workspace.

use std::process::{Command, Output};
use std::sync::OnceLock;

const BIN: &str = env!("CARGO_BIN_EXE_seattrellis_cli");

/// Frozen v2 CLI exit codes (M1-03): 0 success, 2 InvalidInput (and usage
/// errors), 3 ProvenInfeasible, 4 Timeout, 5 Unknown, 70 InternalError,
/// 130 user cancel.
const FROZEN_EXIT_CODES: [i32; 7] = [0, 2, 3, 4, 5, 70, 130];

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Kind {
    /// Graceful usage/argument error: exit 2 + `error:` on stderr.
    Usage,
    /// Success: exit 0, empty stderr.
    Valid,
    /// Any non-panicking frozen exit code with an `error:` on stderr.
    Clean,
}

struct Case {
    name: String,
    args: Vec<String>,
    kind: Kind,
}

impl Case {
    fn new(name: &str, args: Vec<String>, kind: Kind) -> Self {
        Case {
            name: name.to_string(),
            args,
            kind,
        }
    }
}

/// Build a `Vec<String>` argv from mixed `&str` / `String` pieces.
macro_rules! args {
    ($($x:expr),* $(,)?) => {
        vec![$(String::from($x)),*]
    };
}

/// Shared synthetic fixtures, built once per test process. All paths are
/// UTF-8 temp-dir paths, stored as `String` so they drop straight into argv.
struct Fixtures {
    root: String,
    /// Dedicated output directory for the standalone sweep. Every write the
    /// standalone cases make lands here so that concurrent CLI processes from
    /// the other sweeps never share a journal directory (the transaction
    /// layer's recovery pass in crates/seattrellis-io/src/transaction.rs
    /// would otherwise roll back a live sibling transaction).
    std_out: String,
    problem: String,
    solution: String,
    bad_json: String,
    garbage: String,
    snap: String,
    edit_snapshot: String,
    v1_roster: String,
    v1_inplace: String,
    hist_dir: String,
    empty_dir: String,
    project_file: String,
    snapshot_json: String,
    edited_json: String,
    init_workspace_a: String,
    init_workspace_b: String,
    empty_workspace: String,
    bundle: String,
}

fn fixtures() -> &'static Fixtures {
    static FIXTURES: OnceLock<Fixtures> = OnceLock::new();
    FIXTURES.get_or_init(Fixtures::build)
}

impl Fixtures {
    fn build() -> Fixtures {
        let root = std::env::temp_dir().join(format!(
            "seattrellis-cli-arg-sweep-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();

        let problem = root.join("problem.json");
        std::fs::write(
            &problem,
            r#"{"api_version":2,"student_count":3,"seat_positions":[[0.0,0.0],[1.0,0.0],[2.0,0.0]],"edges":[[0,1],[1,2]],"seed":7,"rules":{"seed":7,"soft":{}}}"#,
        )
        .unwrap();

        // Not valid UTF-8: the JSON-read path must fail cleanly, not panic.
        let garbage = root.join("garbage.bin");
        std::fs::write(&garbage, [0x00u8, 0x01, 0xff, 0xfe, 0xde, 0xad, 0xbe, 0xef]).unwrap();

        let bad_json = root.join("bad.json");
        std::fs::write(&bad_json, "this is not json at all {{{").unwrap();

        // Assignments-style snapshot with the synthesized keys the repair /
        // history paths expect for a request without a students array
        // (STU001.., seats seat-1..).
        let snap = root.join("snap.json");
        std::fs::write(
            &snap,
            r#"{"kind":"seattrellis_snapshot","schema_version":2,"assignments":[{"student_key":"STU001","seat_id":"seat-1"},{"student_key":"STU002","seat_id":"seat-2"},{"student_key":"STU003","seat_id":"seat-3"}]}"#,
        )
        .unwrap();

        // Self-contained editor artifact (embedded students/layout/rules).
        let edit_snapshot = root.join("edit-snapshot.json");
        std::fs::write(
            &edit_snapshot,
            r#"{"kind":"seattrellis_snapshot","schema_version":2,
                "students":[{"student_id":"1","name":"A"},{"student_id":"2","name":"B"},{"student_id":"3","name":"C"}],
                "layout":{"layout_id":"l","name":"Room","seats":[
                    {"seat_id":"R1C1","row":1,"col":1,"x":0.0,"y":0.0,"enabled":true},
                    {"seat_id":"R1C2","row":1,"col":2,"x":1.0,"y":0.0,"enabled":true},
                    {"seat_id":"R1C3","row":1,"col":3,"x":2.0,"y":0.0,"enabled":true}],
                    "adjacency":{"edges":[["R1C1","R1C2"],["R1C2","R1C3"]]}},
                "rules":{"seed":1,"soft":{}},
                "assignments":[{"student_key":"1","seat_id":"R1C1"},{"student_key":"2","seat_id":"R1C2"},{"student_key":"3","seat_id":"R1C3"}]}"#,
        )
        .unwrap();

        // v1 roster for schema-migrate, plus a private copy for --in-place.
        let std_out = root.join("std-out");
        std::fs::create_dir_all(&std_out).unwrap();
        let v1_roster = root.join("v1-roster.json");
        std::fs::write(
            &v1_roster,
            r#"{"kind":"student_roster","schema_version":1,"data":{"students":[
                {"student_id":"1","name":"A","height_cm":160,"score":80,"tags":[],"needs":[]},
                {"student_id":"2","name":"B","height_cm":170,"score":70,"tags":[],"needs":[]}]}}"#,
        )
        .unwrap();
        let v1_inplace = std_out.join("v1-inplace.json");
        std::fs::copy(&v1_roster, &v1_inplace).unwrap();

        // History dir scanned by --history-dir (glob *.snapshot.json).
        let hist_dir = root.join("hist");
        std::fs::create_dir_all(&hist_dir).unwrap();
        std::fs::copy(&snap, hist_dir.join("plan.snapshot.json")).unwrap();
        // A directory without snapshot files.
        let empty_dir = root.join("empty");
        std::fs::create_dir_all(&empty_dir).unwrap();

        // 4-student project workspace (initialized by the real binary below).
        let proj = root.join("proj");
        write_project_workspace(&proj);
        let project_file = proj.join("seattrellis.project.json");
        // Named without "snapshot" so the latest-snapshot auto-discovery in
        // project-edit/project-repair deterministically finds the editor-style
        // artifact: project-solve's CoreSolveResponse output cannot be parsed
        // by the repair path, which requires `assignments` entries.
        let snapshot_json = proj.join("outputs").join("solve-response.json");
        let edited_json = proj.join("outputs").join("edited.snapshot.json");
        let bundle_dir = root.join("bundle");
        std::fs::create_dir_all(&bundle_dir).unwrap();

        // Extra workspaces for project-init valid / duplicate runs.
        let init_a = root.join("init-a");
        let init_b = root.join("init-b");
        write_project_workspace(&init_a);
        write_project_workspace(&init_b);
        let empty_workspace = root.join("init-empty");
        std::fs::create_dir_all(&empty_workspace).unwrap();

        // Drive the real binary to produce the derived fixtures: the solve
        // response (guaranteed valid for the export/audit boundaries) and the
        // initialized project (solve + edit outputs).
        let run = |args: Vec<String>| -> Output {
            Command::new(BIN)
                .args(args)
                .output()
                .unwrap_or_else(|error| panic!("could not spawn {BIN}: {error}"))
        };
        let exit_ok = |out: &Output, what: &str| {
            assert!(
                out.status.code() == Some(0),
                "{what} during fixture setup failed: exit {:?} stderr: {}",
                out.status.code(),
                String::from_utf8_lossy(&out.stderr)
            );
        };
        exit_ok(
            &run(args![
                "solve",
                "--problem",
                problem.to_str().unwrap(),
                "--output",
                root.join("solution.json").to_str().unwrap(),
            ]),
            "solve",
        );
        exit_ok(
            &run(args!["project-init", "--dir", proj.to_str().unwrap()]),
            "project-init",
        );
        exit_ok(
            &run(args![
                "project-solve",
                "--project",
                project_file.to_str().unwrap(),
                "--output",
                snapshot_json.to_str().unwrap(),
            ]),
            "project-solve",
        );
        exit_ok(
            &run(args![
                "project-edit",
                "--project",
                project_file.to_str().unwrap(),
                "--snapshot",
                snapshot_json.to_str().unwrap(),
                "--operation",
                "swap:1:2",
                "--output",
                edited_json.to_str().unwrap(),
            ]),
            "project-edit",
        );

        let f = |path: &std::path::Path| path.to_str().unwrap().to_string();
        Fixtures {
            root: f(&root),
            std_out: f(&std_out),
            problem: f(&problem),
            solution: f(&root.join("solution.json")),
            bad_json: f(&bad_json),
            garbage: f(&garbage),
            snap: f(&snap),
            edit_snapshot: f(&edit_snapshot),
            v1_roster: f(&v1_roster),
            v1_inplace: f(&v1_inplace),
            hist_dir: f(&hist_dir),
            empty_dir: f(&empty_dir),
            project_file: f(&project_file),
            snapshot_json: f(&snapshot_json),
            edited_json: f(&edited_json),
            init_workspace_a: f(&init_a),
            init_workspace_b: f(&init_b),
            empty_workspace: f(&empty_workspace),
            bundle: f(&bundle_dir.join("pack1.zip")),
        }
    }
}

/// students.csv / layout.json / rules.json for a 4-student workspace
/// (the shape `project-init` requires).
fn write_project_workspace(dir: &std::path::Path) {
    std::fs::create_dir_all(dir).unwrap();
    std::fs::write(dir.join("students.csv"), "id,name\n1,A\n2,B\n3,C\n4,D\n").unwrap();
    std::fs::write(
        dir.join("layout.json"),
        r#"{"layout_id":"l","name":"Room","seats":[
            {"seat_id":"R1C1","row":1,"col":1,"x":0.0,"y":0.0,"enabled":true},
            {"seat_id":"R1C2","row":1,"col":2,"x":1.0,"y":0.0,"enabled":true},
            {"seat_id":"R2C1","row":2,"col":1,"x":0.0,"y":1.0,"enabled":true},
            {"seat_id":"R2C2","row":2,"col":2,"x":1.0,"y":1.0,"enabled":true}
        ],"adjacency":{"edges":[["R1C1","R1C2"],["R2C1","R2C2"],["R1C1","R2C1"],["R1C2","R2C2"]]}}"#,
    )
    .unwrap();
    std::fs::write(dir.join("rules.json"), r#"{"seed":7,"soft":{}}"#).unwrap();
}

fn run(args: &[String]) -> Output {
    Command::new(BIN)
        .args(args)
        .output()
        .unwrap_or_else(|error| panic!("could not spawn {BIN}: {error}"))
}

fn check(failures: &mut Vec<String>, case: &str, output: &Output, kind: Kind) {
    let stderr = String::from_utf8_lossy(&output.stderr);
    let code = output.status.code();
    let mut issues: Vec<String> = Vec::new();
    if stderr.contains("panicked") {
        issues.push(format!("panic message on stderr: {stderr:?}"));
    }
    match kind {
        Kind::Valid => {
            if code != Some(0) {
                issues.push(format!("expected exit 0, got {code:?}"));
            }
            if !stderr.is_empty() {
                issues.push(format!("expected empty stderr, got: {stderr:?}"));
            }
        }
        Kind::Usage => {
            // Frozen M1-03: usage/argument errors exit 2 (InvalidInput).
            if code != Some(2) {
                issues.push(format!("usage error expected frozen exit 2, got {code:?}"));
            }
            if !stderr.contains("error:") {
                issues.push(format!(
                    "usage error expected 'error:' on stderr, got: {stderr:?}"
                ));
            }
        }
        Kind::Clean => {
            if !matches!(code, Some(0 | 2 | 3 | 4 | 5 | 70 | 130)) {
                issues.push(format!(
                    "expected a frozen exit code {FROZEN_EXIT_CODES:?}, got {code:?}"
                ));
            }
            if !stderr.contains("error:") {
                issues.push(format!("expected 'error:' on stderr, got: {stderr:?}"));
            }
        }
    }
    if !issues.is_empty() {
        failures.push(format!("{case}: {}", issues.join("; ")));
    }
}

fn sweep(cases: Vec<Case>) -> Vec<String> {
    let mut failures = Vec::new();
    for case in &cases {
        let output = run(&case.args);
        check(&mut failures, &case.name, &output, case.kind);
    }
    failures
}

fn assert_no_failures(context: &str, failures: Vec<String>) {
    assert!(
        failures.is_empty(),
        "{context}: {} case(s) failed:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

/// The five generic cases every command shares, plus the valid invocation
/// and per-command extras.
///
/// * `no_args`: bare command (missing required flags) -> Usage unless the
///   command is valid with no arguments;
/// * `--help`: always exit 0;
/// * unknown flag -> Usage;
/// * value flag with a missing value -> Usage;
/// * `input_flag` fed a garbage binary file -> Clean, no panic;
/// * the `valid` invocation -> Valid.
fn command_cases(
    cmd: &str,
    valid: Vec<String>,
    input_flag: &str,
    no_args: Kind,
    fx: &Fixtures,
    extras: Vec<Case>,
) -> Vec<Case> {
    let mut cases = Vec::new();
    cases.push(Case::new(&format!("{cmd}:no-args"), args![cmd], no_args));
    cases.push(Case::new(
        &format!("{cmd}:--help"),
        args![cmd, "--help"],
        Kind::Valid,
    ));
    cases.push(Case::new(
        &format!("{cmd}:unknown-flag"),
        args![cmd, "--bogus"],
        Kind::Usage,
    ));
    cases.push(Case::new(
        &format!("{cmd}:flag-missing-value"),
        args![cmd, input_flag],
        Kind::Usage,
    ));
    // Garbage binary in place of the JSON input: must fail fast, cleanly.
    // Commands without a file-input flag (doctor, schema-list) skip this.
    let mut garbage_args = valid.clone();
    if let Some(position) = garbage_args.iter().position(|arg| arg == input_flag) {
        garbage_args[position + 1] = fx.garbage.clone();
        cases.push(Case::new(
            &format!("{cmd}:garbage-input"),
            garbage_args,
            Kind::Clean,
        ));
    }
    cases.push(Case::new(&format!("{cmd}:valid"), valid, Kind::Valid));
    cases.extend(extras);
    cases
}

// ---------------------------------------------------------------------------
// Top-level: no command, help/version aliases, unknown commands.
// ---------------------------------------------------------------------------

#[test]
fn top_level_arguments_sweep() {
    let cases = vec![
        Case::new("top:no-args", vec![], Kind::Valid),
        Case::new("top:--help", args!["--help"], Kind::Valid),
        Case::new("top:-h", args!["-h"], Kind::Valid),
        Case::new("top:help-command", args!["help"], Kind::Valid),
        Case::new("top:--version", args!["--version"], Kind::Valid),
        Case::new("top:-V", args!["-V"], Kind::Valid),
        Case::new("top:version-command", args!["version"], Kind::Valid),
        Case::new("top:unknown-command", args!["frobnicate"], Kind::Usage),
        Case::new("top:--bogus", args!["--bogus"], Kind::Usage),
    ];
    // `failures` is only mutated by the cfg(unix) non-UTF-8 block below;
    // on Windows the binding stays immutable (clippy -D warnings).
    #[cfg_attr(not(unix), allow(unused_mut))]
    let mut failures = sweep(cases);
    // Non-UTF-8 argv must be a graceful usage error, not a panic (unix).
    #[cfg(unix)]
    {
        use std::ffi::OsStr;
        use std::os::unix::ffi::OsStrExt;
        let output = Command::new(BIN)
            .arg("solve")
            .arg(OsStr::from_bytes(&[0xff]))
            .output()
            .unwrap();
        check(&mut failures, "top:non-utf8-arg", &output, Kind::Usage);
    }
    assert_no_failures("top-level sweep", failures);
}

// ---------------------------------------------------------------------------
// Standalone commands: validate precheck audit score candidates
// history-report pair-report repair edit schema-list schema-export
// schema-migrate solve export doctor.
// ---------------------------------------------------------------------------

#[test]
fn standalone_commands_sweep() {
    let fx = fixtures();
    let assign = "[[0,0],[1,1],[2,2]]";
    let mut cases = Vec::new();

    // doctor / schema-list take no arguments.
    cases.extend(command_cases(
        "doctor",
        args!["doctor"],
        "--dir",
        Kind::Valid,
        fx,
        vec![],
    ));
    cases.push(Case::new(
        "doctor:extra-arg",
        args!["doctor", "extra"],
        Kind::Usage,
    ));
    cases.extend(command_cases(
        "schema-list",
        args!["schema-list"],
        "--kind",
        Kind::Valid,
        fx,
        vec![],
    ));
    cases.push(Case::new(
        "schema-list:extra-arg",
        args!["schema-list", "extra"],
        Kind::Usage,
    ));

    cases.extend(command_cases(
        "validate",
        args!["validate", "--problem", &fx.problem],
        "--problem",
        Kind::Usage,
        fx,
        vec![
            Case::new(
                "validate:preset-unknown",
                args![
                    "validate",
                    "--problem",
                    &fx.problem,
                    "--preset",
                    "no-such-preset"
                ],
                Kind::Clean,
            ),
            Case::new(
                "validate:history-missing-file",
                args![
                    "validate",
                    "--problem",
                    &fx.problem,
                    "--history",
                    "/nonexistent.json"
                ],
                Kind::Clean,
            ),
            Case::new(
                "validate:dup-preset-last-wins",
                args![
                    "validate",
                    "--problem",
                    &fx.problem,
                    "--preset",
                    "daily",
                    "--preset",
                    "random",
                ],
                Kind::Valid,
            ),
            Case::new(
                "validate:equals-syntax",
                args!["validate", format!("--problem={}", fx.problem)],
                Kind::Valid,
            ),
        ],
    ));

    cases.extend(command_cases(
        "precheck",
        args!["precheck", "--problem", &fx.problem],
        "--problem",
        Kind::Usage,
        fx,
        vec![Case::new(
            "precheck:dup-problem",
            args![
                "precheck",
                "--problem",
                &fx.problem,
                "--problem",
                &fx.problem
            ],
            Kind::Valid,
        )],
    ));

    cases.extend(command_cases(
        "audit",
        args![
            "audit",
            "--problem",
            &fx.problem,
            "--solution",
            &fx.solution
        ],
        "--problem",
        Kind::Usage,
        fx,
        vec![
            Case::new(
                "audit:missing-solution",
                args!["audit", "--problem", &fx.problem],
                Kind::Usage,
            ),
            Case::new(
                "audit:garbage-solution",
                args!["audit", "--problem", &fx.problem, "--solution", &fx.garbage],
                Kind::Clean,
            ),
            Case::new(
                "audit:dup-problem",
                args![
                    "audit",
                    "--problem",
                    &fx.problem,
                    "--problem",
                    &fx.problem,
                    "--solution",
                    &fx.solution,
                ],
                Kind::Valid,
            ),
        ],
    ));

    cases.extend(command_cases(
        "score",
        args!["score", "--problem", &fx.problem, "--assignment", assign],
        "--problem",
        Kind::Usage,
        fx,
        vec![
            Case::new(
                "score:bad-assignment",
                args!["score", "--problem", &fx.problem, "--assignment", "notjson"],
                Kind::Clean,
            ),
            Case::new(
                "score:bad-diversity",
                args![
                    "score",
                    "--problem",
                    &fx.problem,
                    "--assignment",
                    assign,
                    "--diversity",
                    "abc",
                ],
                Kind::Usage,
            ),
            Case::new(
                "score:dup-assignment",
                args![
                    "score",
                    "--problem",
                    &fx.problem,
                    "--assignment",
                    assign,
                    "--assignment",
                    assign,
                ],
                Kind::Valid,
            ),
            Case::new(
                "score:latest-snapshot-missing",
                args![
                    "score",
                    "--problem",
                    &fx.problem,
                    "--assignment",
                    assign,
                    "--latest-snapshot",
                    "/nonexistent.json",
                ],
                Kind::Clean,
            ),
        ],
    ));

    cases.extend(command_cases(
        "candidates",
        args!["candidates", "--problem", &fx.problem, "--count", "1"],
        "--problem",
        Kind::Usage,
        fx,
        vec![
            Case::new(
                "candidates:count-zero",
                args!["candidates", "--problem", &fx.problem, "--count", "0"],
                Kind::Clean,
            ),
            Case::new(
                "candidates:count-bad",
                args!["candidates", "--problem", &fx.problem, "--count", "abc"],
                Kind::Usage,
            ),
            Case::new(
                "candidates:dup-count",
                args![
                    "candidates",
                    "--problem",
                    &fx.problem,
                    "--count",
                    "1",
                    "--count",
                    "1",
                ],
                Kind::Valid,
            ),
        ],
    ));

    cases.extend(command_cases(
        "history-report",
        args![
            "history-report",
            "--problem",
            &fx.problem,
            "--history",
            &fx.snap
        ],
        "--problem",
        Kind::Usage,
        fx,
        vec![
            Case::new(
                "history-report:history-dir",
                args![
                    "history-report",
                    "--problem",
                    &fx.problem,
                    "--history-dir",
                    &fx.hist_dir,
                ],
                Kind::Valid,
            ),
            Case::new(
                "history-report:history-dir-empty",
                args![
                    "history-report",
                    "--problem",
                    &fx.problem,
                    "--history-dir",
                    &fx.empty_dir,
                ],
                Kind::Clean,
            ),
            Case::new(
                "history-report:no-history",
                args!["history-report", "--problem", &fx.problem],
                Kind::Usage,
            ),
            Case::new(
                "history-report:dup-history",
                args![
                    "history-report",
                    "--problem",
                    &fx.problem,
                    "--history",
                    &fx.snap,
                    "--history",
                    &fx.snap,
                ],
                Kind::Valid,
            ),
            Case::new(
                "history-report:output",
                args![
                    "history-report",
                    "--problem",
                    &fx.problem,
                    "--history",
                    &fx.snap,
                    "--output",
                    format!("{}/hr-report.json", fx.std_out),
                ],
                Kind::Valid,
            ),
        ],
    ));

    cases.extend(command_cases(
        "pair-report",
        args![
            "pair-report",
            "--problem",
            &fx.problem,
            "--history",
            &fx.snap
        ],
        "--problem",
        Kind::Usage,
        fx,
        vec![
            Case::new(
                "pair-report:history-dir-and-top",
                args![
                    "pair-report",
                    "--problem",
                    &fx.problem,
                    "--history-dir",
                    &fx.hist_dir,
                    "--top",
                    "3",
                    "--within-distance",
                    "1",
                ],
                Kind::Valid,
            ),
            Case::new(
                "pair-report:top-bad",
                args![
                    "pair-report",
                    "--problem",
                    &fx.problem,
                    "--history",
                    &fx.snap,
                    "--top",
                    "abc",
                ],
                Kind::Usage,
            ),
            Case::new(
                "pair-report:within-distance-bad",
                args![
                    "pair-report",
                    "--problem",
                    &fx.problem,
                    "--history",
                    &fx.snap,
                    "--within-distance",
                    "abc",
                ],
                Kind::Usage,
            ),
            Case::new(
                "pair-report:no-history",
                args!["pair-report", "--problem", &fx.problem],
                Kind::Usage,
            ),
            Case::new(
                "pair-report:dup-history",
                args![
                    "pair-report",
                    "--problem",
                    &fx.problem,
                    "--history",
                    &fx.snap,
                    "--history",
                    &fx.snap,
                ],
                Kind::Valid,
            ),
        ],
    ));

    cases.extend(command_cases(
        "repair",
        args![
            "repair",
            "--problem",
            &fx.problem,
            "--snapshot",
            &fx.snap,
            "--output",
            format!("{}/repair-out.json", fx.std_out),
        ],
        "--problem",
        Kind::Usage,
        fx,
        vec![
            Case::new(
                "repair:missing-snapshot",
                args!["repair", "--problem", &fx.problem],
                Kind::Usage,
            ),
            Case::new(
                "repair:locks-and-ignore-saved",
                args![
                    "repair",
                    "--problem",
                    &fx.problem,
                    "--snapshot",
                    &fx.snap,
                    "--lock-student",
                    "STU001",
                    "--lock-seat",
                    "seat-1",
                    "--affected",
                    "STU002",
                    "--ignore-saved-locks",
                    "--output",
                    format!("{}/repair-locks.json", fx.std_out),
                ],
                Kind::Valid,
            ),
            Case::new(
                "repair:dup-lock-student",
                args![
                    "repair",
                    "--problem",
                    &fx.problem,
                    "--snapshot",
                    &fx.snap,
                    "--lock-student",
                    "STU001",
                    "--lock-student",
                    "STU002",
                    "--output",
                    format!("{}/repair-dup.json", fx.std_out),
                ],
                Kind::Valid,
            ),
            Case::new(
                "repair:garbage-snapshot",
                args![
                    "repair",
                    "--problem",
                    &fx.problem,
                    "--snapshot",
                    &fx.garbage,
                ],
                Kind::Clean,
            ),
        ],
    ));

    cases.extend(command_cases(
        "edit",
        args![
            "edit",
            "--snapshot",
            &fx.edit_snapshot,
            "--operation",
            "swap:1:2",
            "--output",
            format!("{}/edit-out.json", fx.std_out),
        ],
        "--snapshot",
        Kind::Usage,
        fx,
        vec![
            Case::new(
                "edit:missing-operation",
                args!["edit", "--snapshot", &fx.edit_snapshot],
                Kind::Usage,
            ),
            Case::new(
                "edit:bad-operation",
                args![
                    "edit",
                    "--snapshot",
                    &fx.edit_snapshot,
                    "--operation",
                    "bogus:1:2",
                ],
                Kind::Clean,
            ),
            Case::new(
                "edit:candidate-on-plain-snapshot",
                args![
                    "edit",
                    "--snapshot",
                    &fx.edit_snapshot,
                    "--operation",
                    "swap:1:2",
                    "--candidate",
                    "recommended",
                ],
                Kind::Clean,
            ),
            Case::new(
                "edit:operations-file-garbage",
                args![
                    "edit",
                    "--snapshot",
                    &fx.edit_snapshot,
                    "--operations-file",
                    &fx.garbage,
                ],
                Kind::Clean,
            ),
            Case::new(
                "edit:dup-operation",
                args![
                    "edit",
                    "--snapshot",
                    &fx.edit_snapshot,
                    "--operation",
                    "swap:1:2",
                    "--operation",
                    "swap:1:3",
                    "--output",
                    format!("{}/edit-dup.json", fx.std_out),
                ],
                Kind::Valid,
            ),
            Case::new(
                "edit:strict",
                args![
                    "edit",
                    "--snapshot",
                    &fx.edit_snapshot,
                    "--operation",
                    "swap:1:2",
                    "--strict",
                    "--output",
                    format!("{}/edit-strict.json", fx.std_out),
                ],
                Kind::Valid,
            ),
        ],
    ));

    cases.extend(command_cases(
        "schema-export",
        args![
            "schema-export",
            "--kind",
            "student_roster",
            "--output",
            format!("{}/roster.schema.json", fx.std_out),
        ],
        "--kind",
        Kind::Usage,
        fx,
        vec![
            Case::new(
                "schema-export:missing-output",
                args!["schema-export", "--kind", "student_roster"],
                Kind::Clean,
            ),
            Case::new(
                "schema-export:kind-bogus",
                args![
                    "schema-export",
                    "--kind",
                    "bogus",
                    "--output",
                    format!("{}/bogus.schema.json", fx.std_out),
                ],
                Kind::Clean,
            ),
            Case::new(
                "schema-export:equals-syntax",
                args![
                    "schema-export",
                    format!("--kind=layout"),
                    "--output",
                    format!("{}/layout.schema.json", fx.std_out),
                ],
                Kind::Valid,
            ),
            Case::new(
                "schema-export:dup-kind",
                args![
                    "schema-export",
                    "--kind",
                    "student_roster",
                    "--kind",
                    "layout",
                    "--output",
                    format!("{}/dup.schema.json", fx.std_out),
                ],
                Kind::Valid,
            ),
        ],
    ));

    cases.extend(command_cases(
        "schema-migrate",
        args![
            "schema-migrate",
            "--input",
            &fx.v1_roster,
            "--output",
            format!("{}/migrated.json", fx.std_out),
        ],
        "--input",
        Kind::Usage,
        fx,
        vec![
            Case::new(
                "schema-migrate:inplace-and-dryrun",
                args![
                    "schema-migrate",
                    "--input",
                    &fx.v1_roster,
                    "--in-place",
                    "--dry-run",
                ],
                Kind::Usage,
            ),
            Case::new(
                "schema-migrate:dry-run",
                args![
                    "schema-migrate",
                    "--input",
                    &fx.v1_roster,
                    "--dry-run",
                    "--output",
                    format!("{}/dry.json", fx.std_out),
                ],
                Kind::Valid,
            ),
            Case::new(
                "schema-migrate:in-place",
                args!["schema-migrate", "--input", &fx.v1_inplace, "--in-place"],
                Kind::Valid,
            ),
            Case::new(
                "schema-migrate:missing-output",
                args!["schema-migrate", "--input", &fx.v1_roster],
                Kind::Clean,
            ),
            Case::new(
                "schema-migrate:dup-input",
                args![
                    "schema-migrate",
                    "--input",
                    &fx.v1_roster,
                    "--input",
                    &fx.v1_roster,
                    "--output",
                    format!("{}/dup.json", fx.std_out),
                ],
                Kind::Valid,
            ),
            Case::new(
                "schema-migrate:garbage-input",
                args![
                    "schema-migrate",
                    "--input",
                    &fx.garbage,
                    "--output",
                    format!("{}/garbage.json", fx.std_out),
                ],
                Kind::Clean,
            ),
        ],
    ));

    cases.extend(command_cases(
        "solve",
        args![
            "solve",
            "--problem",
            &fx.problem,
            "--output",
            format!("{}/solve-out.json", fx.std_out)
        ],
        "--problem",
        Kind::Usage,
        fx,
        vec![
            Case::new(
                "solve:bad-json",
                args!["solve", "--problem", &fx.bad_json],
                Kind::Clean,
            ),
            Case::new(
                "solve:bad-time-limit",
                args!["solve", "--problem", &fx.problem, "--time-limit", "abc"],
                Kind::Usage,
            ),
            Case::new(
                "solve:non-positive-time-limit",
                args!["solve", "--problem", &fx.problem, "--time-limit", "-1"],
                Kind::Usage,
            ),
            Case::new(
                "solve:small-time-limit",
                args!["solve", "--problem", &fx.problem, "--time-limit", "0.05"],
                Kind::Valid,
            ),
            Case::new(
                "solve:bad-seed",
                args!["solve", "--problem", &fx.problem, "--seed", "abc"],
                Kind::Usage,
            ),
            Case::new(
                "solve:equals-syntax",
                args!["solve", format!("--problem={}", fx.problem)],
                Kind::Valid,
            ),
            Case::new(
                "solve:dup-problem",
                args!["solve", "--problem", &fx.problem, "--problem", &fx.problem,],
                Kind::Valid,
            ),
            Case::new(
                "solve:positional-arg",
                args!["solve", &fx.problem],
                Kind::Usage,
            ),
            Case::new(
                "solve:unwritable-output",
                // A parent that is a regular file fails identically on
                // every platform (Windows would happily auto-create
                // `/nonexistent-dir` at the drive root, so an absolute
                // missing dir is NOT a portable "unwritable" path).
                args![
                    "solve",
                    "--problem",
                    &fx.problem,
                    "--output",
                    format!("{}/nested/solve.json", fx.garbage),
                ],
                Kind::Clean,
            ),
            Case::new(
                "solve:help-wins",
                args!["solve", "--problem", &fx.problem, "--help"],
                Kind::Valid,
            ),
        ],
    ));

    cases.extend(command_cases(
        "export",
        args![
            "export",
            "--problem",
            &fx.problem,
            "--solution",
            &fx.solution,
            "--format",
            "svg",
            "--output",
            format!("{}/plan.svg", fx.std_out),
        ],
        "--problem",
        Kind::Usage,
        fx,
        vec![
            Case::new(
                "export:missing-format",
                args![
                    "export",
                    "--problem",
                    &fx.problem,
                    "--solution",
                    &fx.solution,
                    "--output",
                    format!("{}/x1.svg", fx.std_out),
                ],
                Kind::Usage,
            ),
            Case::new(
                "export:missing-output",
                args![
                    "export",
                    "--problem",
                    &fx.problem,
                    "--solution",
                    &fx.solution,
                    "--format",
                    "svg",
                ],
                Kind::Usage,
            ),
            Case::new(
                "export:bad-format",
                args![
                    "export",
                    "--problem",
                    &fx.problem,
                    "--solution",
                    &fx.solution,
                    "--format",
                    "bmp",
                    "--output",
                    format!("{}/x2.svg", fx.std_out),
                ],
                Kind::Usage,
            ),
            Case::new(
                "export:bad-template",
                args![
                    "export",
                    "--problem",
                    &fx.problem,
                    "--solution",
                    &fx.solution,
                    "--format",
                    "svg",
                    "--template",
                    "bogus",
                    "--output",
                    format!("{}/x3.svg", fx.std_out),
                ],
                Kind::Usage,
            ),
            Case::new(
                "export:public-template",
                args![
                    "export",
                    "--problem",
                    &fx.problem,
                    "--solution",
                    &fx.solution,
                    "--format",
                    "svg",
                    "--template",
                    "public",
                    "--output",
                    format!("{}/x4.svg", fx.std_out),
                ],
                Kind::Valid,
            ),
            Case::new(
                "export:garbage-solution",
                args![
                    "export",
                    "--problem",
                    &fx.problem,
                    "--solution",
                    &fx.garbage,
                    "--format",
                    "svg",
                    "--output",
                    format!("{}/x5.svg", fx.std_out),
                ],
                Kind::Clean,
            ),
            Case::new(
                "export:dup-format",
                args![
                    "export",
                    "--problem",
                    &fx.problem,
                    "--solution",
                    &fx.solution,
                    "--format",
                    "svg",
                    "--format",
                    "svg",
                    "--output",
                    format!("{}/x6.svg", fx.std_out),
                ],
                Kind::Valid,
            ),
        ],
    ));

    assert_no_failures("standalone command sweep", sweep(cases));
}

// ---------------------------------------------------------------------------
// Project lifecycle commands: project-init -list -info -validate -solve
// -export -rotate -edit -repair -privacy -pack -restore.
// ---------------------------------------------------------------------------

#[test]
fn project_commands_sweep() {
    let fx = fixtures();
    let mut cases = Vec::new();

    // project-init needs a pre-built workspace; the duplicate case targets a
    // second workspace so the two cases never step on each other.
    cases.extend(command_cases(
        "project-init",
        args!["project-init", "--dir", &fx.init_workspace_a],
        "--dir",
        Kind::Usage,
        fx,
        vec![
            Case::new(
                "project-init:dup-dir-last-wins",
                args![
                    "project-init",
                    "--dir",
                    &fx.init_workspace_a,
                    "--dir",
                    &fx.init_workspace_b,
                ],
                Kind::Valid,
            ),
            Case::new(
                "project-init:dir-missing",
                args!["project-init", "--dir", "/nonexistent-dir"],
                Kind::Clean,
            ),
            Case::new(
                "project-init:dir-without-workspace-files",
                args!["project-init", "--dir", &fx.empty_workspace],
                Kind::Clean,
            ),
        ],
    ));

    // project-list defaults --root "." and --limit 20: bare invocation is a
    // valid run, not a usage error.
    cases.extend(command_cases(
        "project-list",
        args!["project-list", "--root", &fx.root, "--limit", "10"],
        "--root",
        Kind::Valid,
        fx,
        vec![
            Case::new(
                "project-list:root-missing",
                args!["project-list", "--root", "/nonexistent-root"],
                Kind::Clean,
            ),
            Case::new(
                "project-list:limit-bad",
                args!["project-list", "--limit", "abc"],
                Kind::Usage,
            ),
            Case::new(
                "project-list:dup-limit",
                args![
                    "project-list",
                    "--root",
                    &fx.root,
                    "--limit",
                    "10",
                    "--limit",
                    "5",
                ],
                Kind::Valid,
            ),
        ],
    ));

    for cmd in ["project-info", "project-validate"] {
        cases.extend(command_cases(
            cmd,
            args![cmd, "--project", &fx.project_file],
            "--project",
            Kind::Usage,
            fx,
            vec![
                Case::new(
                    &format!("{cmd}:project-missing"),
                    args![cmd, "--project", "/nonexistent.json"],
                    Kind::Clean,
                ),
                Case::new(
                    &format!("{cmd}:dup-project"),
                    args![
                        cmd,
                        "--project",
                        &fx.project_file,
                        "--project",
                        &fx.project_file
                    ],
                    Kind::Valid,
                ),
            ],
        ));
    }

    cases.extend(command_cases(
        "project-solve",
        args![
            "project-solve",
            "--project",
            &fx.project_file,
            "--output",
            format!("{}/proj/ps-sol.json", fx.root),
        ],
        "--project",
        Kind::Usage,
        fx,
        vec![
            Case::new(
                "project-solve:project-missing",
                args!["project-solve", "--project", "/nonexistent.json"],
                Kind::Clean,
            ),
            Case::new(
                "project-solve:seed",
                args![
                    "project-solve",
                    "--project",
                    &fx.project_file,
                    "--seed",
                    "42",
                    "--output",
                    format!("{}/proj/ps-sol2.json", fx.root),
                ],
                Kind::Valid,
            ),
            Case::new(
                "project-solve:dup-project",
                args![
                    "project-solve",
                    "--project",
                    &fx.project_file,
                    "--project",
                    &fx.project_file,
                    "--output",
                    format!("{}/proj/ps-sol3.json", fx.root),
                ],
                Kind::Valid,
            ),
        ],
    ));

    cases.extend(command_cases(
        "project-export",
        args![
            "project-export",
            "--project",
            &fx.project_file,
            "--snapshot",
            &fx.snapshot_json,
            "--format",
            "svg",
            "--output",
            format!("{}/proj/pe.svg", fx.root),
        ],
        "--project",
        Kind::Usage,
        fx,
        vec![
            Case::new(
                "project-export:missing-snapshot",
                args![
                    "project-export",
                    "--project",
                    &fx.project_file,
                    "--format",
                    "svg",
                    "--output",
                    format!("{}/proj/pe1.svg", fx.root),
                ],
                Kind::Clean,
            ),
            Case::new(
                "project-export:missing-format",
                args![
                    "project-export",
                    "--project",
                    &fx.project_file,
                    "--snapshot",
                    &fx.snapshot_json,
                    "--output",
                    format!("{}/proj/pe2.svg", fx.root),
                ],
                Kind::Clean,
            ),
            Case::new(
                "project-export:missing-output",
                args![
                    "project-export",
                    "--project",
                    &fx.project_file,
                    "--snapshot",
                    &fx.snapshot_json,
                    "--format",
                    "svg",
                ],
                Kind::Clean,
            ),
            Case::new(
                "project-export:garbage-snapshot",
                args![
                    "project-export",
                    "--project",
                    &fx.project_file,
                    "--snapshot",
                    &fx.garbage,
                    "--format",
                    "svg",
                    "--output",
                    format!("{}/proj/pe3.svg", fx.root),
                ],
                Kind::Clean,
            ),
            Case::new(
                "project-export:dup-project",
                args![
                    "project-export",
                    "--project",
                    &fx.project_file,
                    "--project",
                    &fx.project_file,
                    "--snapshot",
                    &fx.snapshot_json,
                    "--format",
                    "svg",
                    "--output",
                    format!("{}/proj/pe4.svg", fx.root),
                ],
                Kind::Valid,
            ),
        ],
    ));

    cases.extend(command_cases(
        "project-rotate",
        args![
            "project-rotate",
            "--project",
            &fx.project_file,
            "--periods",
            "1",
            "--output",
            format!("{}/proj/rot.json", fx.root),
        ],
        "--project",
        Kind::Usage,
        fx,
        vec![
            Case::new(
                "project-rotate:periods-zero",
                args![
                    "project-rotate",
                    "--project",
                    &fx.project_file,
                    "--periods",
                    "0"
                ],
                Kind::Usage,
            ),
            Case::new(
                "project-rotate:periods-too-big",
                args![
                    "project-rotate",
                    "--project",
                    &fx.project_file,
                    "--periods",
                    "21"
                ],
                Kind::Usage,
            ),
            Case::new(
                "project-rotate:periods-bad",
                args![
                    "project-rotate",
                    "--project",
                    &fx.project_file,
                    "--periods",
                    "abc"
                ],
                Kind::Usage,
            ),
            Case::new(
                "project-rotate:dup-project",
                args![
                    "project-rotate",
                    "--project",
                    &fx.project_file,
                    "--project",
                    &fx.project_file,
                    "--periods",
                    "1",
                    "--output",
                    format!("{}/proj/rot2.json", fx.root),
                ],
                Kind::Valid,
            ),
        ],
    ));

    cases.extend(command_cases(
        "project-edit",
        args![
            "project-edit",
            "--project",
            &fx.project_file,
            "--snapshot",
            &fx.snapshot_json,
            "--operation",
            "swap:1:2",
            "--output",
            format!("{}/proj/pe-edit.json", fx.root),
        ],
        "--project",
        Kind::Usage,
        fx,
        vec![
            Case::new(
                "project-edit:no-operation",
                args![
                    "project-edit",
                    "--project",
                    &fx.project_file,
                    "--snapshot",
                    &fx.snapshot_json,
                    "--output",
                    format!("{}/proj/pe-noop.json", fx.root),
                ],
                Kind::Valid,
            ),
            Case::new(
                "project-edit:bad-operation",
                args![
                    "project-edit",
                    "--project",
                    &fx.project_file,
                    "--snapshot",
                    &fx.snapshot_json,
                    "--operation",
                    "bogus:1:2",
                    "--output",
                    format!("{}/proj/pe-bad.json", fx.root),
                ],
                Kind::Clean,
            ),
            Case::new(
                "project-edit:strict",
                args![
                    "project-edit",
                    "--project",
                    &fx.project_file,
                    "--snapshot",
                    &fx.snapshot_json,
                    "--operation",
                    "swap:1:2",
                    "--strict",
                    "--output",
                    format!("{}/proj/pe-strict.json", fx.root),
                ],
                Kind::Valid,
            ),
            Case::new(
                "project-edit:dup-operation",
                args![
                    "project-edit",
                    "--project",
                    &fx.project_file,
                    "--snapshot",
                    &fx.snapshot_json,
                    "--operation",
                    "swap:1:2",
                    "--operation",
                    "swap:1:3",
                    "--output",
                    format!("{}/proj/pe-dup.json", fx.root),
                ],
                Kind::Valid,
            ),
        ],
    ));

    cases.extend(command_cases(
        "project-repair",
        args![
            "project-repair",
            "--project",
            &fx.project_file,
            "--snapshot",
            &fx.edited_json,
            "--output",
            format!("{}/proj/pr.json", fx.root),
        ],
        "--project",
        Kind::Usage,
        fx,
        vec![
            // Without --snapshot the outputs dir auto-discovers the latest
            // *.snapshot.json artifact (the editor-style edited.snapshot.json).
            Case::new(
                "project-repair:auto-snapshot",
                args![
                    "project-repair",
                    "--project",
                    &fx.project_file,
                    "--output",
                    format!("{}/proj/pr-auto.json", fx.root),
                ],
                Kind::Valid,
            ),
            // Project-repair must accept project-solve's own output
            // (CoreSolveResponse with index-pair `assignment`) as the
            // snapshot; ledger §19.33 pins this dual-shape boundary.
            Case::new(
                "project-repair:solve-output-snapshot",
                args![
                    "project-repair",
                    "--project",
                    &fx.project_file,
                    "--snapshot",
                    &fx.snapshot_json,
                    "--output",
                    format!("{}/proj/pr-solve-output.json", fx.root),
                ],
                Kind::Valid,
            ),
            Case::new(
                "project-repair:ignore-saved-locks",
                args![
                    "project-repair",
                    "--project",
                    &fx.project_file,
                    "--snapshot",
                    &fx.edited_json,
                    "--ignore-saved-locks",
                    "--output",
                    format!("{}/proj/pr-locks.json", fx.root),
                ],
                Kind::Valid,
            ),
            Case::new(
                "project-repair:dup-locked-students",
                args![
                    "project-repair",
                    "--project",
                    &fx.project_file,
                    "--snapshot",
                    &fx.edited_json,
                    "--locked-students",
                    "1",
                    "--locked-students",
                    "2",
                    "--output",
                    format!("{}/proj/pr-dup.json", fx.root),
                ],
                Kind::Valid,
            ),
            Case::new(
                "project-repair:garbage-snapshot",
                args![
                    "project-repair",
                    "--project",
                    &fx.project_file,
                    "--snapshot",
                    &fx.garbage,
                    "--output",
                    format!("{}/proj/pr-garbage.json", fx.root),
                ],
                Kind::Clean,
            ),
        ],
    ));

    cases.extend(command_cases(
        "project-privacy",
        args!["project-privacy", "--project", &fx.project_file],
        "--project",
        Kind::Usage,
        fx,
        vec![
            Case::new(
                "project-privacy:no-include-outputs",
                args![
                    "project-privacy",
                    "--project",
                    &fx.project_file,
                    "--no-include-outputs"
                ],
                Kind::Valid,
            ),
            Case::new(
                "project-privacy:project-missing",
                args!["project-privacy", "--project", "/nonexistent.json"],
                Kind::Clean,
            ),
            Case::new(
                "project-privacy:dup-project",
                args![
                    "project-privacy",
                    "--project",
                    &fx.project_file,
                    "--project",
                    &fx.project_file,
                ],
                Kind::Valid,
            ),
        ],
    ));

    // Order matters for project-pack / project-restore: the valid pack case
    // creates the bundle the "existing bundle without --force" and the
    // restore cases consume.
    cases.extend(command_cases(
        "project-pack",
        args![
            "project-pack",
            "--project",
            &fx.project_file,
            "--output",
            &fx.bundle,
            "--force",
        ],
        "--project",
        Kind::Usage,
        fx,
        vec![
            Case::new(
                "project-pack:missing-output",
                args!["project-pack", "--project", &fx.project_file],
                Kind::Usage,
            ),
            Case::new(
                "project-pack:existing-bundle-no-force",
                args![
                    "project-pack",
                    "--project",
                    &fx.project_file,
                    "--output",
                    &fx.bundle
                ],
                Kind::Clean,
            ),
            Case::new(
                "project-pack:dup-project",
                args![
                    "project-pack",
                    "--project",
                    &fx.project_file,
                    "--project",
                    &fx.project_file,
                    "--output",
                    format!("{}/pack2.zip", fx.root),
                    "--force",
                ],
                Kind::Valid,
            ),
        ],
    ));

    let restore_a = format!("{}/restore-a", fx.root);
    cases.extend(command_cases(
        "project-restore",
        args![
            "project-restore",
            "--bundle",
            &fx.bundle,
            "--output-dir",
            &restore_a,
        ],
        "--bundle",
        Kind::Usage,
        fx,
        vec![
            Case::new(
                "project-restore:missing-output-dir",
                args!["project-restore", "--bundle", &fx.bundle],
                Kind::Usage,
            ),
            Case::new(
                "project-restore:non-empty-no-force",
                args![
                    "project-restore",
                    "--bundle",
                    &fx.bundle,
                    "--output-dir",
                    &restore_a,
                ],
                Kind::Clean,
            ),
            Case::new(
                "project-restore:force",
                args![
                    "project-restore",
                    "--bundle",
                    &fx.bundle,
                    "--output-dir",
                    &restore_a,
                    "--force",
                ],
                Kind::Valid,
            ),
            Case::new(
                "project-restore:garbage-bundle",
                args![
                    "project-restore",
                    "--bundle",
                    &fx.garbage,
                    "--output-dir",
                    format!("{}/restore-b", fx.root),
                ],
                Kind::Clean,
            ),
            Case::new(
                "project-restore:dup-bundle",
                args![
                    "project-restore",
                    "--bundle",
                    &fx.bundle,
                    "--bundle",
                    &fx.bundle,
                    "--output-dir",
                    format!("{}/restore-c", fx.root),
                ],
                Kind::Valid,
            ),
        ],
    ));

    assert_no_failures("project command sweep", sweep(cases));
}
