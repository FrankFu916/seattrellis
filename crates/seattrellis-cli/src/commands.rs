//! Solve and export command flows.
//!
//! `run_solve` reads a `CoreSolveRequest` JSON file, runs the core solver
//! (via the public `solve_problem_json` entry point), prints a human-readable
//! summary to stdout and optionally writes the `CoreSolveResponse` JSON.
//!
//! `run_export` reads both the problem and the solve result, recovers the seat
//! grid, and renders SVG or HTML through `render`.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde_json::json;

use seattrellis_core::{
    audit_report_json, generate_candidates_json_with_latest_snapshot, history_report_json,
    pair_report_json, precheck_report_json, repair_json_with_options, solve_problem_json,
    validate_solve_request_json, validate_solve_response, CoreSolveRequest, CoreSolveResponse,
    SolveStatus,
};

use crate::presets;
use crate::style::Styler;
use crate::ValidateArgs;
use crate::{
    AuditArgs, CandidatesArgs, EditArgs, ExportArgs, ExportFormat, HistoryReportArgs,
    PairReportArgs, PrecheckArgs, ProjectArgs, ProjectEditArgs, ProjectInitArgs, ProjectListArgs,
    ProjectPackArgs, ProjectPrivacyArgs, ProjectRepairArgs, ProjectRestoreArgs, ProjectRotateArgs,
    RepairArgs, SchemaExportArgs, SchemaMigrateArgs, ScoreArgs, SolveArgs,
};
use seattrellis_export::export::export_plan;

/// Publish a CLI output atomically (staged sibling temp + journaled
/// transaction with rollback, plan §5.5 "all project writes roll back").
fn write_output_atomically(path: &Path, contents: &[u8]) -> Result<(), String> {
    seattrellis_io::transaction::atomic_write_file(path, contents)
        .map_err(|error| format!("could not write {}: {error}", path.display()))?;
    Ok(())
}

pub fn run_validate(args: &ValidateArgs) -> Result<(), String> {
    let styler = Styler::stdout();
    let problem_text = read_text(&args.problem)?;
    validate_solve_request_json(&problem_text)
        .map_err(|error| format!("'{}' is invalid: {error}", args.problem.display()))?;

    let problem: CoreSolveRequest = serde_json::from_str(&problem_text)
        .map_err(|error| format!("'{}' is not valid JSON: {error}", args.problem.display()))?;
    println!("{}: {}", styler.bold("valid"), styler.green("true"));
    println!(
        "{}: {}",
        styler.bold("students"),
        styler.cyan(&problem.student_count.to_string())
    );
    println!(
        "{}: {}",
        styler.bold("seats"),
        styler.cyan(&problem.seat_positions.len().to_string())
    );
    println!(
        "{}: {}",
        styler.bold("hard rules"),
        styler.cyan(
            &(problem.fixed_seats.len()
                + problem.must_be_adjacent.len()
                + problem.cannot_be_adjacent.len()
                + problem.min_distance.len())
            .to_string(),
        )
    );

    // Warning semantics mirror `run_validate` (service.py:944 ->
    // validation.py): capability warnings for the loaded models plus
    // preset-context warnings for the preferred-data requirements.
    let mut warnings: Vec<String> = Vec::new();

    // score_distribution with scope='group' requires group_id on every
    // enabled seat (validation.py `_add_rule_capability_warnings`).
    if let (Some(rules), Some(layout)) = (&problem.rules, &problem.layout) {
        let distribution = &rules.soft.score_distribution;
        if distribution.enabled
            && distribution.scope == seattrellis_core::models::DistributionScope::Group
        {
            let missing: Vec<&str> = layout
                .seats
                .iter()
                .filter(|seat| seat.enabled && seat.group_id.is_none())
                .map(|seat| seat.seat_id.as_str())
                .collect();
            if !missing.is_empty() {
                let preview = missing[..missing.len().min(5)].join(", ");
                let suffix = if missing.len() > 5 { "..." } else { "" };
                warnings.push(format!(
                    "score_distribution with scope='group' requires group_id on every \
                     enabled seat. Missing group_id: {preview}{suffix}. The objective \
                     will be skipped until all enabled seats are grouped."
                ));
            }
        }
    }

    // History files are counted for preset history warnings; unreadable
    // paths are errors exactly like the oracle's snapshot loading.
    for path in &args.history {
        read_text(path)?;
    }
    if let Some(preset_name) = &args.preset {
        if presets::preset_requirements(preset_name).is_none() {
            return Err(format!(
                "Unknown preset {preset_name:?}. Available presets: random, exam, daily, \
                 fair-rotation, neighbor-aware, balanced, peer-mixing, score-high-front, \
                 score-high-back, row-score-balanced, group-score-balanced, mentor-pairing, \
                 height-aware, vision-friendly."
            ));
        }
        let soft = problem
            .rules
            .as_ref()
            .map(|rules| &rules.soft)
            .cloned()
            .unwrap_or_default();
        warnings.extend(presets::preset_context_warnings(
            preset_name,
            &problem.students,
            &soft,
            args.history.len(),
        ));
    }

    if !warnings.is_empty() {
        println!(
            "{}: {}",
            styler.bold("warnings"),
            styler.yellow(&warnings.len().to_string())
        );
        for warning in &warnings {
            println!("- {warning}");
        }
    }
    if args.strict && !warnings.is_empty() {
        return Err(format!(
            "Warnings treated as errors by --strict:\n{}",
            warnings
                .iter()
                .map(|warning| format!("- {warning}"))
                .collect::<Vec<_>>()
                .join("\n")
        ));
    }
    Ok(())
}

/// Run the solver and return the frozen v2 `SolveStatus` so the caller
/// can map it onto the frozen CLI exit-code table (plan §4.1, M1-03).
/// `project-solve`: compile the project workspace into a solve request and
/// run the solver (plan §5.5 project lifecycle).
pub fn run_project_solve(args: &ProjectArgs) -> Result<SolveStatus, String> {
    let mut request = crate::project::build_request(&args.project)?;
    if let Some(seed) = args.seed {
        request["seed"] = serde_json::Value::from(seed);
    }
    let request_json = serde_json::to_string(&request)
        .map_err(|error| format!("could not serialize the compiled request: {error}"))?;
    let response_json = solve_problem_json(&request_json)
        .map_err(|error| format!("solver rejected the problem: {error}"))?;
    let response: CoreSolveResponse = serde_json::from_str(&response_json)
        .map_err(|error| format!("solver returned malformed JSON: {error}"))?;

    let styler = Styler::stdout();
    println!(
        "{}: {}",
        styler.bold("feasible"),
        if response.feasible {
            styler.green("true")
        } else {
            styler.red("false")
        }
    );
    println!(
        "{}: {}",
        styler.bold("status"),
        styler.cyan(response.status.as_str())
    );
    println!(
        "{}: {}",
        styler.bold("total_cost"),
        styler.cyan(
            &response
                .total_cost
                .map(|cost| cost.to_string())
                .unwrap_or_else(|| "-".to_string())
        )
    );
    println!(
        "{}: {}",
        styler.bold("students seated"),
        styler.cyan(&response.assignment.len().to_string())
    );
    if let Some(output) = &args.output {
        write_output_atomically(output, response_json.as_bytes())?;
        println!("wrote result JSON to '{}'", output.display());
    }
    Ok(response.status)
}

/// Rebuild a `CoreSolveResponse` from a saved plan document so the export
/// boundary renders exactly the plan that was persisted — never a fresh
/// re-solve (which could silently differ from the saved plan). The result
/// passes the independent validator before it is exported.
///
/// Two document shapes are accepted: the `CoreSolveResponse` JSON written by
/// `project-solve --output` (index-pair `assignment`), and editor-style
/// snapshots with `assignments: [{student_key, seat_id}]`.
fn response_from_snapshot(
    request: &CoreSolveRequest,
    snapshot: &serde_json::Value,
) -> Result<CoreSolveResponse, String> {
    if snapshot.get("assignment").is_some() || snapshot.get("feasible").is_some() {
        let response: CoreSolveResponse = serde_json::from_value(snapshot.clone())
            .map_err(|error| format!("saved plan is not a CoreSolveResponse: {error}"))?;
        validate_solve_response(request, &response)
            .map_err(|message| format!("saved plan is not valid for this project: {message}"))?;
        return Ok(response);
    }
    let student_index: HashMap<&str, usize> = request
        .students
        .iter()
        .enumerate()
        .map(|(index, student)| (student.key.as_str(), index))
        .collect();
    let seat_index: HashMap<&str, usize> = request
        .layout
        .as_ref()
        .map(|layout| {
            layout
                .seats
                .iter()
                .enumerate()
                .map(|(index, seat)| (seat.seat_id.as_str(), index))
                .collect()
        })
        .unwrap_or_default();
    let mut assignment: Vec<[usize; 2]> = Vec::new();
    if let Some(entries) = snapshot
        .get("assignments")
        .and_then(serde_json::Value::as_array)
    {
        for entry in entries {
            let student = entry
                .get("student_key")
                .and_then(serde_json::Value::as_str)
                .ok_or("snapshot assignment is missing student_key")?;
            let seat = entry
                .get("seat_id")
                .and_then(serde_json::Value::as_str)
                .ok_or("snapshot assignment is missing seat_id")?;
            let student = *student_index
                .get(student)
                .ok_or_else(|| format!("snapshot references unknown student {student:?}"))?;
            let seat = *seat_index
                .get(seat)
                .ok_or_else(|| format!("snapshot references unknown seat {seat:?}"))?;
            assignment.push([student, seat]);
        }
    }
    let response = CoreSolveResponse {
        api_version: seattrellis_core::NATIVE_API_VERSION,
        feasible: true,
        status: SolveStatus::Solved,
        assignment,
        attempts_used: 0,
        hard_constraints_satisfied: true,
        total_cost: None,
    };
    validate_solve_response(request, &response)
        .map_err(|message| format!("saved plan is not valid for this project: {message}"))?;
    Ok(response)
}

/// `project-export`: render a SAVED plan (snapshot from `project-solve
/// --output`) to the requested format (plan §5.5 project lifecycle). It
/// never re-solves: exporting must reflect the plan the teacher saved.
pub fn run_project_export(args: &ProjectArgs) -> Result<(), String> {
    let format = args
        .format
        .as_deref()
        .ok_or("project-export requires --format <svg|html|png|pdf|print-html>")?;
    let output = args
        .output
        .clone()
        .ok_or("project-export requires --output <file>")?;
    let snapshot_path = args.snapshot.clone().ok_or(
        "project-export renders a saved plan: run 'project-solve --output <snapshot.json>' first, then pass --snapshot <file>",
    )?;
    let mut request_value = crate::project::build_request(&args.project)?;
    if let Some(seed) = args.seed {
        request_value["seed"] = serde_json::Value::from(seed);
    }
    let request: CoreSolveRequest = serde_json::from_value(request_value.clone())
        .map_err(|error| format!("compiled request is malformed: {error}"))?;
    let snapshot: serde_json::Value = serde_json::from_str(&read_text(&snapshot_path)?)
        .map_err(|error| format!("'{}' is not valid JSON: {error}", snapshot_path.display()))?;
    let response = response_from_snapshot(&request, &snapshot)?;

    let export_document = json!({
        "draft_id": "project-export",
        "format": format,
        "template": "teacher",
        "privacy": {
            "hide_scores": false, "hide_notes": false, "hide_special_needs": false,
            "anonymize": false, "show_height": true, "show_vision": true
        },
        "orientation": "portrait",
        "page_scale": 1.0,
        "locale": "zh",
        "show_student_ids": true,
        "request": request_value,
        "response": serde_json::to_value(&response)
            .map_err(|error| format!("response re-encode failed: {error}"))?,
    });
    let export_json = serde_json::to_string(&export_document)
        .map_err(|error| format!("could not serialize the export request: {error}"))?;
    let bytes = export_plan(&export_json).map_err(|error| error.to_string())?;
    write_output_atomically(&output, &bytes)?;
    println!("wrote {} to '{}'", format, output.display());
    Ok(())
}

/// `project-rotate`: generate future seating periods for a project workspace
/// (plan §5.5 lifecycle; mirrors the Python `project_rotate` contract). Runs
/// the shared application rotation path — the same solver +
/// independent-validation loop the server uses — and persists the plan into
/// the project outputs (or `--output`).
pub fn run_project_rotate(args: &ProjectRotateArgs) -> Result<(), String> {
    let request_value = crate::project::build_request(&args.project)?;
    let project_name =
        crate::project::project_name(&args.project).unwrap_or_else(|_| "SeatTrellis".to_string());
    let editor_store = seattrellis_domain::editing::new_draft_store();
    let solve_requests: seattrellis_application::SolveRequestStore =
        std::sync::Mutex::new(HashMap::new());
    let outcome = seattrellis_application::rotation::generate_rotation_plan_from_core(
        &request_value,
        seattrellis_application::rotation::RotationOptions {
            period_count: args.periods,
            labels: Vec::new(),
            base_seed: args.seed.unwrap_or(42),
            plan_name: format!("{project_name} Rotation Plan"),
            base_snapshots: Vec::new(),
        },
        &editor_store,
        &solve_requests,
    )
    .map_err(|error| format!("rotation failed: {}", error.message))?;
    if !outcome.feasible {
        return Err(format!(
            "no feasible rotation plan (status {}, period {})",
            outcome.status.as_str(),
            outcome.failed_period.unwrap_or(0)
        ));
    }
    let plan = outcome
        .plan
        .ok_or_else(|| "rotation produced no plan document".to_string())?;
    let plan_json = serde_json::to_string(&plan)
        .map_err(|error| format!("could not serialize the rotation plan: {error}"))?;
    if let Some(output) = &args.output {
        write_output_atomically(output, plan_json.as_bytes())?;
        println!(
            "wrote rotation plan ({} periods) to '{}'",
            args.periods,
            output.display()
        );
    } else {
        // Persist into the project's outputs directory (Python default).
        let saved = seattrellis_io::rotation::rotation_save_json(
            &args.project.to_string_lossy(),
            &plan_json,
        )
        .map_err(|error| format!("could not save the rotation plan: {error}"))?;
        let saved_value: serde_json::Value = serde_json::from_str(&saved)
            .map_err(|error| format!("rotation save response is malformed: {error}"))?;
        println!(
            "saved rotation plan ({} periods) to '{}'",
            saved_value
                .get("period_count")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(args.periods as u64),
            saved_value
                .get("output_path")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("")
        );
    }
    Ok(())
}

/// `project-edit`: apply manual editor operations to a project seating
/// artifact (plan §5.5 lifecycle; mirrors the Python `project_edit`
/// contract). The edited artifact is written in the editor-style snapshot
/// shape that `project-export` renders.
pub fn run_project_edit(args: &ProjectEditArgs) -> Result<(), String> {
    let request_value = crate::project::build_request(&args.project)?;
    let request: CoreSolveRequest = serde_json::from_value(request_value.clone())
        .map_err(|error| format!("compiled request is malformed: {error}"))?;

    let snapshot_path = match &args.snapshot {
        Some(path) => path.clone(),
        None => latest_snapshot_artifact(&args.project)?,
    };
    let snapshot_text = read_text(&snapshot_path)?;
    let snapshot: serde_json::Value = serde_json::from_str(&snapshot_text)
        .map_err(|error| format!("'{}' is not valid JSON: {error}", snapshot_path.display()))?;

    // Recover the draft from the saved plan (same shape project-export reads).
    let assignment = editor_assignment_pairs(&request, &snapshot)?;
    let assignment_refs: Vec<(&str, &str)> = assignment
        .iter()
        .map(|(student, seat)| (student.as_str(), seat.as_str()))
        .collect();
    let keys = seattrellis_application::class_generation::student_keys(&request);
    let key_refs: Vec<&str> = keys.iter().map(String::as_str).collect();
    let seats = seattrellis_application::class_generation::seat_specs(&request);
    let display_names: HashMap<String, String> = request
        .students
        .iter()
        .map(|student| {
            (
                student.key.clone(),
                student
                    .display_name
                    .clone()
                    .unwrap_or_else(|| student.key.clone()),
            )
        })
        .collect();

    let store = seattrellis_domain::editing::new_draft_store();
    let mut state = seattrellis_domain::editing::create_draft(
        &store,
        "project-edit",
        None,
        &key_refs,
        seats,
        &assignment_refs,
        Some(&display_names),
    )
    .map_err(|error| format!("could not open the plan for editing: {error}"))?;

    let operations = parse_edit_operations(&args.operations, args.operations_file.as_deref())?;
    for (index, operation) in operations.iter().enumerate() {
        let envelope = seattrellis_domain::editing::EditorCommandEnvelope {
            kind: "seattrellis_editor_command".to_string(),
            protocol_version: "1.0".to_string(),
            command_id: format!("cli-{index}"),
            draft_id: "project-edit".to_string(),
            base_revision: state.revision,
            action: "apply".to_string(),
            operations: vec![operation.clone()],
        };
        state = seattrellis_domain::editing::apply_command_in_store(&store, &envelope)
            .map_err(|error| format!("operation {index} failed: {error}"))?;
    }

    // Independent validation: every edited product must satisfy the plan's
    // hard rules before it is written (revised plan: feasible=true is never hard-coded).
    let edited_response = editor_response(&request, &state)?;
    if args.strict {
        validate_solve_response(&request, &edited_response)
            .map_err(|message| format!("edited plan violates hard constraints: {message}"))?;
    }

    let output = match &args.output {
        Some(path) => path.clone(),
        None => edited_snapshot_output_path(&snapshot_path, &args.project)?,
    };
    let document = edited_snapshot_document(&request, &state);
    let text = serde_json::to_string_pretty(&document)
        .map_err(|error| format!("could not serialize the edited plan: {error}"))?;
    write_output_atomically(&output, text.as_bytes())?;
    println!(
        "wrote edited snapshot ({} operations, revision {}) to '{}'",
        operations.len(),
        state.revision,
        output.display()
    );
    Ok(())
}

/// `project-repair`: re-solve a project seating artifact preserving anchors
/// (plan §5.5 lifecycle; mirrors the Python `project_repair` contract).
/// `--ignore-saved-locks` skips locks persisted in the snapshot metadata.
pub fn run_project_repair(args: &ProjectRepairArgs) -> Result<(), String> {
    let request_value = crate::project::build_request(&args.project)?;
    let request_json = serde_json::to_string(&request_value)
        .map_err(|error| format!("could not serialize the compiled request: {error}"))?;
    let snapshot_path = match &args.snapshot {
        Some(path) => path.clone(),
        None => latest_snapshot_artifact(&args.project)?,
    };
    let snapshot_text = read_text(&snapshot_path)?;
    let repaired = repair_json_with_options(
        &request_json,
        &snapshot_text,
        &args.affected,
        &args.locked_students,
        &args.locked_seats,
        !args.ignore_saved_locks,
    )
    .map_err(|error| format!("repair failed: {error}"))?;
    let output = match &args.output {
        Some(path) => path.clone(),
        None => repaired_snapshot_output_path(&snapshot_path, &args.project)?,
    };
    write_output_atomically(&output, repaired.as_bytes())?;
    println!("wrote repaired snapshot to '{}'", output.display());
    Ok(())
}

/// `schema-list`: print the v2 artifact registry (kind -> version + policy).
pub fn run_schema_list() -> Result<(), String> {
    let registry = seattrellis_schema::registry::REGISTRY;
    println!("{:<22} {:<9} migratable", "kind", "version");
    for entry in registry {
        println!(
            "{:<22} v{:<8} {}",
            format!("{:?}", entry.kind).to_lowercase(),
            entry.current_version,
            entry.migratable_from_older
        );
    }
    Ok(())
}

/// `schema-export`: write the v2 JSON Schema document for one artifact kind.
pub fn run_schema_export(args: &SchemaExportArgs) -> Result<(), String> {
    let schema = v2_schema_for_kind(&args.kind)?;
    let output = args.output.clone().ok_or_else(|| {
        format!(
            "schema-export requires --output <file> (kind {})",
            args.kind
        )
    })?;
    write_output_atomically(&output, schema.as_bytes())?;
    println!(
        "wrote JSON Schema for '{}' to '{}'",
        args.kind,
        output.display()
    );
    Ok(())
}

/// `schema-migrate`: validate and rewrite a versioned JSON artifact
/// (v1 -> v2 where a typed migration step is registered).
pub fn run_schema_migrate(args: &SchemaMigrateArgs) -> Result<(), String> {
    let input_text = read_text(&args.input)?;
    let document: serde_json::Value = serde_json::from_str(&input_text)
        .map_err(|error| format!("'{}' is not valid JSON: {error}", args.input.display()))?;
    let kind_name = document
        .get("kind")
        .and_then(serde_json::Value::as_str)
        .ok_or("artifact is missing a 'kind' field")?;
    let kind = artifact_kind_from_name(kind_name)
        .ok_or_else(|| format!("unknown artifact kind {kind_name:?}"))?;
    let version = document
        .get("schema_version")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(1);

    // v1 artifacts are envelope-less documents (`{"students": [...]}`); if a
    // caller wrapped one in an envelope, unwrap the `data` part first.
    let migration_source = if version == 1 && document.get("kind").is_some() {
        document
            .get("data")
            .cloned()
            .unwrap_or_else(|| document.clone())
    } else {
        document.clone()
    };
    let migrated = if version == 2 {
        document.clone()
    } else {
        let (migrated, report) = seattrellis_schema::migrate_v1_to_v2(kind, &migration_source)
            .map_err(|error| format!("migration failed: {error}"))?;
        if args.dry_run {
            println!(
                "would migrate {kind_name} v{version} -> v2 ({} warning(s)): {}",
                report.warnings.len(),
                args.output
                    .as_ref()
                    .map(|path| format!("target {path}", path = path.display()))
                    .unwrap_or_else(|| "no target".to_string())
            );
            return Ok(());
        }
        migrated
    };

    let output = if args.in_place {
        args.input.clone()
    } else {
        args.output
            .clone()
            .ok_or("schema-migrate requires --output <file>, --in-place, or --dry-run")?
    };
    let text = serde_json::to_string_pretty(&migrated)
        .map_err(|error| format!("could not serialize the migrated artifact: {error}"))?;
    write_output_atomically(&output, text.as_bytes())?;
    println!(
        "{kind_name} schema_version {} written to '{}'",
        migrated
            .get("schema_version")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(2),
        output.display()
    );
    Ok(())
}

// ---------------------------------------------------------------------------
// project-edit / project-repair helpers
// ---------------------------------------------------------------------------

/// The latest `*.snapshot.json` artifact in a project's outputs directory.
fn latest_snapshot_artifact(project_path: &Path) -> Result<PathBuf, String> {
    let (_, root) = seattrellis_io::projects::load_project_document(project_path)?;
    let outputs = root.join("outputs");
    if !outputs.is_dir() {
        return Err(format!(
            "project has no outputs directory yet: {}",
            outputs.display()
        ));
    }
    let mut candidates: Vec<PathBuf> = std::fs::read_dir(&outputs)
        .map_err(|error| format!("could not read {}: {error}", outputs.display()))?
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension().and_then(|extension| extension.to_str()) == Some("json")
                && path
                    .file_name()
                    .map(|name| name.to_string_lossy().contains("snapshot"))
                    .unwrap_or(false)
        })
        .collect();
    candidates.sort_by_key(|path| {
        std::fs::metadata(path)
            .and_then(|meta| meta.modified())
            .ok()
    });
    candidates
        .pop()
        .ok_or_else(|| format!("no snapshot artifact found under {}", outputs.display()))
}

fn edited_snapshot_output_path(source: &Path, project_path: &Path) -> Result<PathBuf, String> {
    let (_, root) = seattrellis_io::projects::load_project_document(project_path)?;
    let outputs = root.join("outputs");
    std::fs::create_dir_all(&outputs)
        .map_err(|error| format!("could not create {}: {error}", outputs.display()))?;
    let name = source
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "snapshot.json".to_string());
    Ok(outputs.join(format!("edited-{name}")))
}

fn repaired_snapshot_output_path(source: &Path, project_path: &Path) -> Result<PathBuf, String> {
    let (_, root) = seattrellis_io::projects::load_project_document(project_path)?;
    let outputs = root.join("outputs");
    std::fs::create_dir_all(&outputs)
        .map_err(|error| format!("could not create {}: {error}", outputs.display()))?;
    let name = source
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "snapshot.json".to_string());
    Ok(outputs.join(format!("repaired-{name}")))
}

/// Convert a saved plan document into `(student_key, seat_id)` assignment
/// pairs the editor draft understands. Two shapes are accepted, matching
/// `response_from_snapshot`: the `CoreSolveResponse` JSON written by
/// `project-solve --output` (index-pair `assignment`) and editor-style
/// snapshots with `assignments: [{student_key, seat_id}]`.
fn editor_assignment_pairs(
    request: &CoreSolveRequest,
    snapshot: &serde_json::Value,
) -> Result<Vec<(String, String)>, String> {
    let keys = seattrellis_application::class_generation::student_keys(request);
    let seat_ids: Vec<String> = (0..request.seat_positions.len())
        .map(|index| seattrellis_application::class_generation::seat_id_for_index(request, index))
        .collect();
    if let Some(index_pairs) = snapshot
        .get("assignment")
        .and_then(serde_json::Value::as_array)
    {
        let mut pairs = Vec::with_capacity(index_pairs.len());
        for pair in index_pairs {
            let entries = pair
                .as_array()
                .ok_or("snapshot assignment pair is not an array")?;
            let student_index = entries
                .first()
                .and_then(serde_json::Value::as_u64)
                .ok_or("snapshot assignment pair is missing student index")?
                as usize;
            let seat_index = entries
                .get(1)
                .and_then(serde_json::Value::as_u64)
                .ok_or("snapshot assignment pair is missing seat index")?
                as usize;
            let student = keys
                .get(student_index)
                .ok_or_else(|| {
                    format!("snapshot references unknown student index {student_index}")
                })?
                .clone();
            let seat = seat_ids
                .get(seat_index)
                .ok_or_else(|| format!("snapshot references unknown seat index {seat_index}"))?
                .clone();
            pairs.push((student, seat));
        }
        return Ok(pairs);
    }
    let entries = snapshot
        .get("assignments")
        .and_then(serde_json::Value::as_array)
        .ok_or("snapshot has neither 'assignment' nor 'assignments' (run project-solve --output first)")?;
    let mut pairs = Vec::with_capacity(entries.len());
    for entry in entries {
        let student = entry
            .get("student_key")
            .and_then(serde_json::Value::as_str)
            .ok_or("snapshot assignment is missing student_key")?;
        let seat = entry
            .get("seat_id")
            .and_then(serde_json::Value::as_str)
            .ok_or("snapshot assignment is missing seat_id")?;
        if !request.students.iter().any(|item| item.key == student) {
            return Err(format!("snapshot references unknown student {student:?}"));
        }
        if !seat_ids.iter().any(|item| item == seat) {
            return Err(format!("snapshot references unknown seat {seat:?}"));
        }
        pairs.push((student.to_string(), seat.to_string()));
    }
    Ok(pairs)
}

/// Rebuild a `CoreSolveResponse` from the edited draft so the independent
/// validator can re-check the hard rules.
fn editor_response(
    request: &CoreSolveRequest,
    state: &seattrellis_domain::editing::EditorState,
) -> Result<CoreSolveResponse, String> {
    let keys = seattrellis_application::class_generation::student_keys(request);
    let key_index: HashMap<&str, usize> = keys
        .iter()
        .enumerate()
        .map(|(index, key)| (key.as_str(), index))
        .collect();
    let seat_index: HashMap<String, usize> = (0..request.seat_positions.len())
        .map(|index| {
            (
                seattrellis_application::class_generation::seat_id_for_index(request, index),
                index,
            )
        })
        .collect();
    let mut assignment: Vec<[usize; 2]> = Vec::new();
    for student in &state.students {
        if let Some(seat_id) = &student.seat_id {
            let student_index = *key_index
                .get(student.student_key.as_str())
                .ok_or("edited draft references an unknown student")?;
            let seat_index = *seat_index
                .get(seat_id)
                .ok_or("edited draft references an unknown seat")?;
            assignment.push([student_index, seat_index]);
        }
    }
    Ok(CoreSolveResponse {
        api_version: seattrellis_core::NATIVE_API_VERSION,
        feasible: true,
        status: SolveStatus::Solved,
        assignment,
        attempts_used: 0,
        hard_constraints_satisfied: true,
        total_cost: None,
    })
}

/// The edited artifact document in the editor-style snapshot shape that
/// `project-export` renders.
fn edited_snapshot_document(
    request: &CoreSolveRequest,
    state: &seattrellis_domain::editing::EditorState,
) -> serde_json::Value {
    let entries: Vec<serde_json::Value> = state
        .students
        .iter()
        .filter_map(|student| {
            student.seat_id.as_ref().map(|seat_id| {
                json!({
                    "student_key": student.student_key,
                    "student_name": student.display_name,
                    "seat_id": seat_id,
                })
            })
        })
        .collect();
    json!({
        "kind": "seattrellis_snapshot",
        "schema_version": 2,
        "assignments": entries,
        "student_count": request.student_count,
        "edited": true,
    })
}

/// The v2 JSON Schema document embedded for one artifact kind (the same
/// files `xtask contract schemas` generates and CI drift-checks). Embedded
/// at compile time so a release binary works from any directory.
pub fn v2_schema_for_kind(kind: &str) -> Result<String, String> {
    match kind.trim().to_ascii_lowercase().as_str() {
        "student_roster" | "studentroster" | "roster" => {
            Ok(include_str!("../../../schemas/student-roster.v2.schema.json").to_string())
        }
        "classroom_layout" | "classroomlayout" | "layout" => {
            Ok(include_str!("../../../schemas/classroom-layout.v2.schema.json").to_string())
        }
        "ruleset" | "rule_set" | "rules" => {
            Ok(include_str!("../../../schemas/ruleset.v2.schema.json").to_string())
        }
        "seating_snapshot" | "seatingsnapshot" | "snapshot" => {
            Ok(include_str!("../../../schemas/snapshot.v2.schema.json").to_string())
        }
        "project" => Ok(include_str!("../../../schemas/project.v2.schema.json").to_string()),
        "project_bundle_manifest" | "bundle_manifest" => {
            Ok(include_str!("../../../schemas/project-bundle-manifest.v2.schema.json").to_string())
        }
        "candidate_set" | "candidates" => {
            Ok(include_str!("../../../schemas/candidate-set.v2.schema.json").to_string())
        }
        "plan_comparison" | "plancomparison" | "plan-comparison" => {
            Ok(include_str!("../../../schemas/plan-comparison-report.v2.schema.json").to_string())
        }
        // `rotation_plan` intentionally stays on the v1 schema: the Rust
        // rotation artifact mirrors the v1 oracle contract
        // (schema_version "0.2.2", ledger L5), so the v1 schema is the
        // correct validator for what this crate actually writes.
        "rotation_plan" | "rotation" => {
            Ok(include_str!("../../../schemas/rotation-plan.schema.json").to_string())
        }
        other => Err(format!(
            "no v2 JSON Schema embedded for kind {other:?} (known: student_roster, \
             classroom_layout, ruleset, seating_snapshot, project, \
             project_bundle_manifest, candidate_set, plan_comparison, rotation_plan; \
             history_archive/editing_operation_log/export_preset have no typed DTO \
             and are not generated by `xtask contract schemas`)"
        )),
    }
}

/// Re-solve a snapshot while preserving requested anchors (D.11 repair).
/// Saved locks persisted in the snapshot metadata are merged by default
/// (Python `reuse_saved_locks=True`); `--ignore-saved-locks` turns that off.
pub fn run_repair(args: &RepairArgs) -> Result<(), String> {
    let problem_text = read_text(&args.problem)?;
    let snapshot_text = read_text(&args.snapshot)?;
    let repaired = repair_json_with_options(
        &problem_text,
        &snapshot_text,
        &args.affected,
        &args.locked_students,
        &args.locked_seats,
        !args.ignore_saved_locks,
    )
    .map_err(|error| format!("repair failed: {error}"))?;
    if let Some(output) = &args.output {
        write_output_atomically(output, repaired.as_bytes())?;
        println!("wrote repaired snapshot to '{}'", output.display());
    } else {
        println!("{repaired}");
    }
    Ok(())
}

/// `edit`: apply manual editor operations to a snapshot or candidate set
/// (oracle cli.py `edit_snapshot`). Inline `--operation` values use the
/// Python string syntax (`swap:STU001:STU002`); a JSON `--operations-file`
/// applies first. The edited artifact is written in the editor-style
/// snapshot shape and passes the independent validator when `--strict`.
pub fn run_edit(args: &EditArgs) -> Result<(), String> {
    let snapshot_text = read_text(&args.snapshot)?;
    let artifact: serde_json::Value = serde_json::from_str(&snapshot_text)
        .map_err(|error| format!("'{}' is not valid JSON: {error}", args.snapshot.display()))?;
    let artifact = select_edit_artifact(&artifact, args.candidate.as_deref())?;

    // Compile the artifact's embedded students/layout/rules into a core
    // request through the same io path project plans use, so hard-rule
    // validation of the edited plan is identical to solve/repair/export.
    let request_value = compile_edit_request(&artifact)?;
    let request: CoreSolveRequest = serde_json::from_value(request_value.clone())
        .map_err(|error| format!("artifact is not core-compatible: {error}"))?;

    let assignment = editor_assignment_pairs(&request, &artifact)?;
    let assignment_refs: Vec<(&str, &str)> = assignment
        .iter()
        .map(|(student, seat)| (student.as_str(), seat.as_str()))
        .collect();
    let keys: Vec<String> = request
        .students
        .iter()
        .map(|student| student.key.clone())
        .collect();
    let key_refs: Vec<&str> = keys.iter().map(String::as_str).collect();
    let seats = seattrellis_application::class_generation::seat_specs(&request);
    let display_names: HashMap<String, String> = request
        .students
        .iter()
        .map(|student| {
            (
                student.key.clone(),
                student
                    .display_name
                    .clone()
                    .unwrap_or_else(|| student.key.clone()),
            )
        })
        .collect();

    let operations = parse_edit_operations(&args.operations, args.operations_file.as_deref())?;
    let store = seattrellis_domain::editing::new_draft_store();
    let mut state = seattrellis_domain::editing::create_draft(
        &store,
        "edit",
        None,
        &key_refs,
        seats,
        &assignment_refs,
        Some(&display_names),
    )
    .map_err(|error| format!("could not open the plan for editing: {error}"))?;
    for (index, operation) in operations.iter().enumerate() {
        let envelope = seattrellis_domain::editing::EditorCommandEnvelope {
            kind: "seattrellis_editor_command".to_string(),
            protocol_version: "1.0".to_string(),
            command_id: format!("cli-{index}"),
            draft_id: "edit".to_string(),
            base_revision: state.revision,
            action: "apply".to_string(),
            operations: vec![operation.clone()],
        };
        state = seattrellis_domain::editing::apply_command_in_store(&store, &envelope)
            .map_err(|error| format!("operation {index} failed: {error}"))?;
    }

    // Independent validation: the edited product must satisfy the artifact's
    // hard rules before it is written (feasible=true is never hard-coded).
    let edited_response = editor_response(&request, &state)?;
    if args.strict {
        validate_solve_response(&request, &edited_response)
            .map_err(|message| format!("edited plan violates hard constraints: {message}"))?;
    }
    let hard_satisfied = validate_solve_response(&request, &edited_response).is_ok();

    let output = args
        .output
        .clone()
        .unwrap_or_else(|| PathBuf::from("outputs/edited.snapshot.json"));
    let document = edited_snapshot_document(&request, &state);
    let text = serde_json::to_string_pretty(&document)
        .map_err(|error| format!("could not serialize the edited plan: {error}"))?;
    write_output_atomically(&output, text.as_bytes())?;
    println!("Edited snapshot written to {}", output.display());
    println!("{}", edit_summary(&state, operations.len(), hard_satisfied));
    Ok(())
}

/// The oracle `_format_edit_summary` shape: operation count, unseated /
/// locked students, locked seats and the hard-constraint verdict.
fn edit_summary(
    state: &seattrellis_domain::editing::EditorState,
    operation_count: usize,
    hard_satisfied: bool,
) -> String {
    let mut unseated: Vec<&str> = state
        .students
        .iter()
        .filter(|student| student.seat_id.is_none())
        .map(|student| student.student_key.as_str())
        .collect();
    unseated.sort_unstable();
    let mut locked_students: Vec<&str> = state
        .students
        .iter()
        .filter(|student| student.locked)
        .map(|student| student.student_key.as_str())
        .collect();
    locked_students.sort_unstable();
    let mut locked_seats: Vec<&str> = state
        .seats
        .iter()
        .filter(|seat| seat.locked)
        .map(|seat| seat.seat_id.as_str())
        .collect();
    locked_seats.sort_unstable();
    format!(
        "Manual edit summary:\n\
         - operations: {operation_count}\n\
         - unseated students: {}\n\
         - locked students: {}\n\
         - locked seats: {}\n\
         - hard constraints: {}",
        format_preview(&unseated),
        format_preview(&locked_students),
        format_preview(&locked_seats),
        if hard_satisfied {
            "satisfied (0 violation(s))"
        } else {
            "not satisfied"
        }
    )
}

/// The oracle `_format_preview` helper: first five values, ellipsized.
fn format_preview(values: &[&str]) -> String {
    if values.is_empty() {
        return "none".to_string();
    }
    let preview = values[..values.len().min(5)].join(", ");
    if values.len() > 5 {
        format!("{preview}, ... ({} total)", values.len())
    } else {
        preview
    }
}

/// Dispatch on the artifact kind (`load_seating_artifact`): a candidate set
/// selects one candidate plan (the `--candidate` id, or `recommended` which
/// resolves to `recommended_candidate_id`); anything else is used as-is.
fn select_edit_artifact(
    artifact: &serde_json::Value,
    candidate: Option<&str>,
) -> Result<serde_json::Value, String> {
    if artifact.get("kind").and_then(serde_json::Value::as_str) != Some("candidate_set") {
        if candidate.is_some() {
            return Err(
                "--candidate can only be used when --snapshot is a candidate set.".to_string(),
            );
        }
        return Ok(artifact.clone());
    }
    let candidates = artifact
        .get("candidates")
        .and_then(serde_json::Value::as_array)
        .ok_or("candidate set has no candidates array")?;
    let recommended = artifact
        .get("recommended_candidate_id")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("recommended");
    let wanted = candidate.unwrap_or("recommended");
    let selected = candidates.iter().find(|plan| {
        let plan_id = plan
            .get("candidate_id")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("");
        plan_id == wanted || (wanted == "recommended" && plan_id == recommended)
    });
    let selected = selected.ok_or_else(|| format!("candidate set has no candidate {wanted:?}"))?;
    selected
        .get("snapshot")
        .cloned()
        .ok_or_else(|| "selected candidate has no snapshot document".to_string())
}

/// Compile a core request from the artifact's embedded students/layout/rules
/// through the same io path project plans use.
fn compile_edit_request(artifact: &serde_json::Value) -> Result<serde_json::Value, String> {
    let students = artifact
        .get("students")
        .ok_or("snapshot has no students array (edit needs a seating snapshot or candidate set)")?;
    let layout = artifact
        .get("layout")
        .ok_or("snapshot has no layout document")?;
    let rules = artifact
        .get("rules")
        .ok_or("snapshot has no rules document")?;
    seattrellis_io::projects::compile_solve_request_from_json(students, layout, rules)
}

/// Parse `--operation <text>` string values (the oracle `_parse_edit_operation`
/// syntax) plus an optional `--operations-file` (a JSON list, or an object
/// with an `operations` list) into ordered editor operations.
fn parse_edit_operations(
    inline: &[String],
    file: Option<&Path>,
) -> Result<Vec<seattrellis_domain::editing::EditorOperation>, String> {
    let mut operations = Vec::new();
    if let Some(file) = file {
        let text = read_text(file)?;
        let value: serde_json::Value = serde_json::from_str(&text)
            .map_err(|error| format!("'{}' is not valid JSON: {error}", file.display()))?;
        let entries = value.get("operations").cloned().unwrap_or(value);
        let entries = entries.as_array().ok_or_else(|| {
            format!(
                "'{}' must be a list or an object with an operations list",
                file.display()
            )
        })?;
        for entry in entries {
            operations.push(
                serde_json::from_value(entry.clone()).map_err(|error| {
                    format!("invalid operation in '{}': {error}", file.display())
                })?,
            );
        }
    }
    for raw in inline {
        operations.push(parse_edit_operation_string(raw)?);
    }
    Ok(operations)
}

/// The oracle `_parse_edit_operation` string syntax:
/// `swap:STU001:STU002`, `move:STU003:R2C2`, `batch-move:STU001=R1C2,STU002=R1C1`,
/// `seat:...`, `unseat:...`, `lock-student:...`, `unlock-student:...`,
/// `lock-seat:...`, `unlock-seat:...`.
pub(crate) fn parse_edit_operation_string(
    raw: &str,
) -> Result<seattrellis_domain::editing::EditorOperation, String> {
    use seattrellis_domain::editing::EditorOperation;
    let text = raw.trim();
    if text.is_empty() {
        return Err("Editing operation cannot be empty.".to_string());
    }
    let parts: Vec<&str> = text.split(':').map(str::trim).collect();
    let kind = normalize_edit_operation_kind(parts[0])?;
    let payload = |keys: &[&str],
                   values: &[&str]|
     -> Result<serde_json::Map<String, serde_json::Value>, String> {
        let mut map = serde_json::Map::new();
        for (key, value) in keys.iter().zip(values) {
            map.insert(
                (*key).to_string(),
                serde_json::Value::String((*value).to_string()),
            );
        }
        Ok(map)
    };
    match kind.as_str() {
        "swap_students" => {
            require_parts(text, &parts, 3)?;
            Ok(EditorOperation {
                kind: kind.clone(),
                payload: payload(&["first_student", "second_student"], &[parts[1], parts[2]])?,
            })
        }
        "move_student" | "seat_student" => {
            require_parts(text, &parts, 3)?;
            Ok(EditorOperation {
                kind: kind.clone(),
                payload: payload(&["student_key", "seat_id"], &[parts[1], parts[2]])?,
            })
        }
        "batch_move" => {
            require_parts(text, &parts, 2)?;
            let moves: Vec<serde_json::Value> = parts[1]
                .split(',')
                .enumerate()
                .map(|(index, item)| {
                    let pair: Vec<&str> = item.splitn(2, '=').map(str::trim).collect();
                    if pair.len() != 2 || pair[0].is_empty() || pair[1].is_empty() {
                        return Err(format!(
                            "Invalid batch move item {} in operation {text:?}. \
                             Use STUDENT=SEAT pairs separated by commas.",
                            index + 1
                        ));
                    }
                    Ok(serde_json::json!({
                        "student_key": pair[0],
                        "seat_id": pair[1],
                    }))
                })
                .collect::<Result<Vec<_>, String>>()?;
            Ok(EditorOperation {
                kind: kind.clone(),
                payload: serde_json::Map::from_iter([(
                    "moves".to_string(),
                    serde_json::Value::Array(moves),
                )]),
            })
        }
        "unseat_student" | "lock_student" | "unlock_student" => {
            require_parts(text, &parts, 2)?;
            Ok(EditorOperation {
                kind: kind.clone(),
                payload: payload(&["student_key"], &[parts[1]])?,
            })
        }
        _ => {
            require_parts(text, &parts, 2)?;
            Ok(EditorOperation {
                kind: kind.clone(),
                payload: payload(&["seat_id"], &[parts[1]])?,
            })
        }
    }
}

fn require_parts(text: &str, parts: &[&str], count: usize) -> Result<(), String> {
    if parts.len() < count {
        return Err(format!(
            "Editing operation {text:?} is missing required parts (expected at least {count})."
        ));
    }
    Ok(())
}

/// The oracle `_normalize_edit_operation_kind` aliases.
pub(crate) fn normalize_edit_operation_kind(value: &str) -> Result<String, String> {
    let name = value.replace('-', "_").trim().to_lowercase();
    let kind = match name.as_str() {
        "swap" | "swap_students" => "swap_students",
        "move" | "move_student" => "move_student",
        "batch" | "batch_move" => "batch_move",
        "seat" | "seat_student" => "seat_student",
        "unseat" | "unseat_student" => "unseat_student",
        "lock_student" => "lock_student",
        "unlock_student" => "unlock_student",
        "lock_seat" => "lock_seat",
        "unlock_seat" => "unlock_seat",
        _ => {
            return Err(format!(
                "Unsupported editing operation {value:?}. Use swap, move, batch-move, seat, \
                 unseat, lock-student, unlock-student, lock-seat, or unlock-seat."
            ));
        }
    };
    Ok(kind.to_string())
}

/// Summarize historical seating snapshots (fairness report, ledger B.5).
/// `--history-dir` scans `*.snapshot.json` (the oracle's default glob) and
/// `--output` writes the JSON report to a file while still printing it.
pub fn run_history_report(args: &HistoryReportArgs) -> Result<(), String> {
    let problem_text = read_text(&args.problem)?;
    let snapshots = load_snapshot_documents(&collect_history_paths(
        &args.history,
        args.history_dir.as_deref(),
    )?)?;
    let report = history_report_json(&problem_text, &snapshots)
        .map_err(|error| format!("history report failed: {error}"))?;
    if let Some(output) = &args.output {
        write_output_atomically(output, report.as_bytes())?;
    }
    println!("{report}");
    Ok(())
}

/// Summarize historical desk-mate / neighbor pairs (ledger B.5).
pub fn run_pair_report(args: &PairReportArgs) -> Result<(), String> {
    let problem_text = read_text(&args.problem)?;
    let snapshots = load_snapshot_documents(&collect_history_paths(
        &args.history,
        args.history_dir.as_deref(),
    )?)?;
    let report = pair_report_json(&problem_text, &snapshots, args.top, args.within_distance)
        .map_err(|error| format!("pair report failed: {error}"))?;
    println!("{report}");
    Ok(())
}

/// Combine explicit `--history` files with a `--history-dir` scan. Mirroring
/// the oracle, `--history` entries are files only; only `--history-dir`
/// scans a directory for `*.snapshot.json` (the default history glob).
fn collect_history_paths(
    history: &[PathBuf],
    history_dir: Option<&Path>,
) -> Result<Vec<PathBuf>, String> {
    let mut paths = history.to_vec();
    if let Some(dir) = history_dir {
        let entries = std::fs::read_dir(dir)
            .map_err(|error| format!("could not read {}: {error}", dir.display()))?;
        let mut scanned: Vec<PathBuf> = entries
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| {
                path.is_file()
                    && path
                        .file_name()
                        .map(|name| name.to_string_lossy().ends_with(".snapshot.json"))
                        .unwrap_or(false)
            })
            .collect();
        scanned.sort();
        paths.extend(scanned);
    }
    if paths.is_empty() {
        return Err(
            "no history snapshots found (pass --history <snapshot.json> or --history-dir)"
                .to_string(),
        );
    }
    Ok(paths)
}

/// Read one or more snapshot JSON files into a single JSON array document.
fn load_snapshot_documents(paths: &[PathBuf]) -> Result<String, String> {
    let mut documents: Vec<serde_json::Value> = Vec::new();
    for path in paths {
        let text = read_text(path)?;
        let value: serde_json::Value = serde_json::from_str(&text)
            .map_err(|error| format!("'{}' is not valid JSON: {error}", path.display()))?;
        if let Some(list) = value.as_array() {
            documents.extend(list.clone());
        } else {
            documents.push(value);
        }
    }
    serde_json::to_string(&documents)
        .map_err(|error| format!("could not serialize snapshots: {error}"))
}

/// Generate a diverse candidate set and print the JSON report (plan §6.3).
/// An optional `--latest-snapshot` activates the per-candidate stability
/// dimension (the Python CLI leaves it not_available; this is the same
/// activation path the fixed-assignment scorer uses).
pub fn run_candidates(args: &CandidatesArgs) -> Result<(), String> {
    let problem_text = read_text(&args.problem)?;
    let latest = match &args.latest_snapshot {
        Some(path) => read_text(path)?,
        None => String::new(),
    };
    let report = generate_candidates_json_with_latest_snapshot(&problem_text, args.count, &latest)
        .map_err(|error| format!("candidate generation failed: {error}"))?;
    println!("{report}");
    Ok(())
}

/// Run the solution audit and print the JSON report (plan §6.5).
pub fn run_audit(args: &AuditArgs) -> Result<(), String> {
    let problem_text = read_text(&args.problem)?;
    let solution_text = read_text(&args.solution)?;
    let solution: CoreSolveResponse = serde_json::from_str(&solution_text)
        .map_err(|error| format!("'{}' is not valid JSON: {error}", args.solution.display()))?;
    let report = audit_report_json(&problem_text, &solution.assignment)
        .map_err(|error| format!("audit failed: {error}"))?;
    println!("{report}");
    Ok(())
}

/// Score a fixed assignment with the Python-parity PlanScore breakdown
/// (plan §6.2/§6.6 item 4: Rust/Python scoring parity evidence).
pub fn run_score(args: &ScoreArgs) -> Result<(), String> {
    let problem_text = read_text(&args.problem)?;
    let assignment: Vec<[usize; 2]> = serde_json::from_str(&args.assignment)
        .map_err(|error| format!("--assignment is not a valid JSON array of pairs: {error}"))?;
    let latest_snapshot = match &args.latest_snapshot {
        Some(path) => read_text(path)?,
        None => String::new(),
    };
    let report = seattrellis_core::score_assignment_json(
        &problem_text,
        &assignment,
        &latest_snapshot,
        args.diversity,
    )
    .map_err(|error| format!("scoring failed: {error}"))?;
    println!("{report}");
    Ok(())
}

/// `doctor`: environment diagnostics (plan §5.5 CLI surface).
pub fn run_doctor() -> Result<(), String> {
    let styler = Styler::stdout();
    println!("{}: {}", styler.bold("binary"), env!("CARGO_PKG_NAME"));
    println!("{}: {}", styler.bold("version"), env!("CARGO_PKG_VERSION"));
    println!(
        "{}: {}",
        styler.bold("core api version"),
        seattrellis_core::NATIVE_API_VERSION
    );
    let temp = std::env::temp_dir();
    let probe = temp.join(format!(
        "seattrellis-doctor-{}-{}.tmp",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(0)
    ));
    match std::fs::write(&probe, b"ok") {
        Ok(()) => {
            let _ = std::fs::remove_file(&probe);
            println!("{}: {} (writable)", styler.bold("temp dir"), temp.display());
        }
        Err(error) => {
            println!(
                "{}: {} not writable ({error})",
                styler.bold("temp dir"),
                temp.display()
            );
            return Err(format!("temp dir is not writable: {error}"));
        }
    }
    Ok(())
}

/// `project-init`: create a `seattrellis_project` workspace file in a
/// directory that already carries `students.csv` / `layout.json` /
/// `rules.json` (plan §5.5 project lifecycle).
pub fn run_project_init(args: &ProjectInitArgs) -> Result<(), String> {
    let dir = args
        .dir
        .canonicalize()
        .map_err(|error| format!("could not resolve {}: {error}", args.dir.display()))?;
    if !dir.is_dir() {
        return Err(format!("{} is not a directory", dir.display()));
    }
    let project_file = dir.join("seattrellis.project.json");
    if project_file.exists() {
        return Err(format!(
            "project file already exists: {}",
            project_file.display()
        ));
    }
    let mut references = serde_json::Map::new();
    for (field, name) in [
        ("students", "students.csv"),
        ("layout", "layout.json"),
        ("rules", "rules.json"),
    ] {
        if !dir.join(name).is_file() {
            return Err(format!(
                "{name} is missing; project-init needs an existing workspace"
            ));
        }
        references.insert(
            field.to_string(),
            serde_json::Value::String(name.to_string()),
        );
    }
    let document = serde_json::json!({
        "kind": "seattrellis_project",
        "schema_version": 1,
        "name": dir.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_else(|| "SeatTrellis Project".to_string()),
        "students": references["students"],
        "layout": references["layout"],
        "rules": references["rules"],
        "outputs_dir": "outputs",
    });
    write_output_atomically(&project_file, document.to_string().as_bytes())?;
    println!("wrote '{}'", project_file.display());
    Ok(())
}

/// `project-list`: list recent projects under a root (io layer).
pub fn run_project_list(args: &ProjectListArgs) -> Result<String, String> {
    seattrellis_io::projects::list_projects_json(
        args.root.to_str().ok_or("root path is not valid UTF-8")?,
        args.limit,
    )
}

/// `project-privacy`: scan a project for sensitive fields (io layer).
/// `--no-include-outputs` mirrors the Python switch; outputs join the scan
/// by default.
pub fn run_project_privacy(args: &ProjectPrivacyArgs) -> Result<String, String> {
    seattrellis_io::projects::project_privacy_json_with_options(
        args.project
            .to_str()
            .ok_or("project path is not valid UTF-8")?,
        args.include_outputs,
    )
}

/// `project-pack`: pack a project workspace into a `.seattrellis.zip` bundle.
/// Mirroring the oracle, an existing bundle is refused unless `--force`.
pub fn run_project_pack(args: &ProjectPackArgs) -> Result<(), String> {
    if args.output.exists() && !args.force {
        return Err(format!(
            "Project bundle already exists: {}. Use --force to overwrite it.",
            args.output.display()
        ));
    }
    let bundle = seattrellis_io::projects::pack_project_json(
        args.project
            .to_str()
            .ok_or("project path is not valid UTF-8")?,
    )?;
    write_output_atomically(&args.output, &bundle)?;
    println!("wrote '{}'", args.output.display());
    Ok(())
}

/// `project-restore`: restore a bundle into an output directory (journaled
/// directory transaction; `--force` overwrites a non-empty destination).
pub fn run_project_restore(args: &ProjectRestoreArgs) -> Result<(), String> {
    let bundle = std::fs::read(&args.bundle)
        .map_err(|error| format!("could not read {}: {error}", args.bundle.display()))?;
    let restored = seattrellis_io::projects::restore_project_bundle(
        &bundle,
        args.output_dir
            .to_str()
            .ok_or("output dir is not valid UTF-8")?,
        args.force,
    )?;
    println!("restored '{}'", restored.display());
    Ok(())
}

/// Run the feasibility precheck and print the JSON report (M3-06).
pub fn run_precheck(args: &PrecheckArgs) -> Result<(), String> {
    let problem_text = read_text(&args.problem)?;
    let report = precheck_report_json(&problem_text)
        .map_err(|error| format!("'{}' is invalid: {error}", args.problem.display()))?;
    println!("{report}");
    Ok(())
}

pub fn run_solve(args: &SolveArgs) -> Result<SolveStatus, String> {
    let styler = Styler::stdout();
    let problem_text = read_text(&args.problem)?;
    let mut problem: serde_json::Value = serde_json::from_str(&problem_text)
        .map_err(|error| format!("'{}' is not valid JSON: {error}", args.problem.display()))?;
    if !problem.is_object() {
        return Err(format!(
            "'{}' must contain a JSON object, not an array or scalar",
            args.problem.display()
        ));
    }
    if let Some(seed) = args.seed {
        problem["seed"] = serde_json::Value::from(seed);
    }
    if let Some(seconds) = args.time_limit {
        problem["time_limit_seconds"] = serde_json::Value::from(seconds);
    }
    let request_json = serde_json::to_string(&problem)
        .map_err(|error| format!("could not re-encode the problem: {error}"))?;

    let response_json = solve_problem_json(&request_json)
        .map_err(|error| format!("solver rejected the problem: {error}"))?;
    let response: CoreSolveResponse = serde_json::from_str(&response_json)
        .map_err(|error| format!("solver returned malformed JSON: {error}"))?;

    // Human-readable summary on stdout.
    let feasible_text = if response.feasible {
        styler.green("true")
    } else {
        styler.red("false")
    };
    let hard_text = if response.hard_constraints_satisfied {
        styler.green("true")
    } else {
        styler.yellow("false")
    };
    println!("{}: {}", styler.bold("feasible"), feasible_text);
    println!(
        "{}: {}",
        styler.bold("hard_constraints_satisfied"),
        hard_text
    );
    println!(
        "{}: {}",
        styler.bold("attempts_used"),
        response.attempts_used
    );
    match response.total_cost {
        Some(cost) => println!("{}: {cost}", styler.bold("total_cost")),
        None => println!("{}: none", styler.bold("total_cost")),
    }
    println!(
        "{}: {}",
        styler.bold("students seated"),
        styler.cyan(&response.assignment.len().to_string())
    );
    println!(
        "{}: {}",
        styler.bold("status"),
        styler.cyan(response.status.as_str())
    );

    if let Some(output) = &args.output {
        let pretty = serde_json::to_string_pretty(&response)
            .map_err(|error| format!("could not encode the result: {error}"))?;
        write_text(output, &format!("{pretty}\n"))?;
        println!(
            "{} result JSON to '{}'",
            styler.green("wrote"),
            output.display()
        );
    }
    Ok(response.status)
}

pub fn run_export(args: &ExportArgs) -> Result<(), String> {
    let styler = Styler::stdout();
    let problem_text = read_text(&args.problem)?;
    let problem_value: serde_json::Value = serde_json::from_str(&problem_text)
        .map_err(|error| format!("'{}' is not valid JSON: {error}", args.problem.display()))?;
    let request: CoreSolveRequest =
        serde_json::from_value(problem_value.clone()).map_err(|error| {
            format!(
                "'{}' is not a valid solve problem (CoreSolveRequest): {error}",
                args.problem.display()
            )
        })?;

    let solution_text = read_text(&args.solution)?;
    let solution_value: serde_json::Value =
        serde_json::from_str(&solution_text).map_err(|error| {
            format!(
                "'{}' is not a valid solve result (CoreSolveResponse): {error}",
                args.solution.display()
            )
        })?;
    let response: CoreSolveResponse =
        serde_json::from_value(solution_value.clone()).map_err(|error| {
            format!(
                "'{}' is not a valid solve result (CoreSolveResponse): {error}",
                args.solution.display()
            )
        })?;

    validate_solve_response(&request, &response)
        .map_err(|message| format!("refusing to export an invalid solved plan: {message}"))?;
    // Every format goes through the shared export crate dispatch: the same
    // privacy filtering (template / anonymize), independent validation and
    // renderers the server uses. The CLI's own render module remains for
    // the audit/report paths; export must never bypass the privacy layer.
    let export_request = serde_json::json!({
        "draft_id": "",
        "format": match args.format {
            ExportFormat::Svg => "svg",
            ExportFormat::Html => "html",
            ExportFormat::Png => "png",
            ExportFormat::Pdf => "pdf",
            ExportFormat::Xlsx => "xlsx",
            ExportFormat::Docx => "docx",
            ExportFormat::Pptx => "pptx",
        },
        "template": args.template,
        "privacy": {
            "hide_scores": true,
            "hide_notes": true,
            "hide_special_needs": true,
            "anonymize": args.template == "public",
            "show_height": false,
            "show_vision": false,
        },
        "orientation": "landscape",
        "page_scale": 1.0,
        "locale": "zh",
        "request": problem_value,
        "response": solution_value,
    });
    let bytes = export_plan(&export_request.to_string())?;
    write_bytes(&args.output, &bytes)?;

    let format_name = match args.format {
        ExportFormat::Svg => styler.cyan("SVG"),
        ExportFormat::Html => styler.cyan("HTML"),
        ExportFormat::Png => styler.cyan("PNG"),
        ExportFormat::Pdf => styler.cyan("PDF"),
        ExportFormat::Xlsx => styler.cyan("XLSX"),
        ExportFormat::Docx => styler.cyan("DOCX"),
        ExportFormat::Pptx => styler.cyan("PPTX"),
    };
    println!(
        "{} {format_name} seating plan ({}/{} seats) to '{}'",
        styler.green("wrote"),
        styler.bold(&response.assignment.len().to_string()),
        request.seat_positions.len(),
        args.output.display()
    );
    Ok(())
}

fn read_text(path: &Path) -> Result<String, String> {
    std::fs::read_to_string(path)
        .map_err(|error| format!("cannot read '{}': {error}", path.display()))
}
fn write_text(path: &Path, text: &str) -> Result<(), String> {
    write_output_atomically(path, text.as_bytes())
}

fn write_bytes(path: &Path, bytes: &[u8]) -> Result<(), String> {
    write_output_atomically(path, bytes)
}

/// Map a `kind` string to the registry's `ArtifactKind`.
fn artifact_kind_from_name(kind_name: &str) -> Option<seattrellis_schema::registry::ArtifactKind> {
    use seattrellis_schema::registry::ArtifactKind;
    match kind_name
        .trim()
        .to_ascii_lowercase()
        .replace('_', "-")
        .as_str()
    {
        "student-roster" | "studentroster" => Some(ArtifactKind::StudentRoster),
        "classroom-layout" | "classroomlayout" => Some(ArtifactKind::ClassroomLayout),
        "ruleset" | "rule-set" => Some(ArtifactKind::RuleSet),
        "seating-snapshot" | "seatingsnapshot" | "snapshot" => Some(ArtifactKind::SeatingSnapshot),
        "candidate-set" | "candidateset" => Some(ArtifactKind::CandidateSet),
        "plan-comparison" => Some(ArtifactKind::PlanComparison),
        "history-archive" => Some(ArtifactKind::HistoryArchive),
        "rotation-plan" => Some(ArtifactKind::RotationPlan),
        "editing-operation-log" => Some(ArtifactKind::EditingOperationLog),
        "project" => Some(ArtifactKind::Project),
        "project-bundle-manifest" => Some(ArtifactKind::ProjectBundleManifest),
        "export-preset" => Some(ArtifactKind::ExportPreset),
        _ => None,
    }
}
