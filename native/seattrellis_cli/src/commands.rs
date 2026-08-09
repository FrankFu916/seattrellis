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
    audit_report_json, generate_candidates_json, history_report_json, pair_report_json,
    precheck_report_json, repair_json, solve_problem_json, validate_solve_request_json,
    validate_solve_response, CoreSolveRequest, CoreSolveResponse, SolveStatus,
};

use crate::render::SeatingGrid;
use crate::style::Styler;
use crate::ValidateArgs;
use crate::{
    AuditArgs, CandidatesArgs, ExportArgs, ExportFormat, HistoryReportArgs, PairReportArgs,
    PrecheckArgs, ProjectArgs, RepairArgs, SolveArgs,
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
    Ok(())
}

/// Run the solver and return the frozen v2 `SolveStatus` so the caller
/// can map it onto the frozen CLI exit-code table (plan §四.1, M1-03).
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
        .ok_or("project-export requires --format <svg|html|png|pdf>")?;
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

/// Re-solve a snapshot while preserving requested anchors (D.11 repair).
pub fn run_repair(args: &RepairArgs) -> Result<(), String> {
    let problem_text = read_text(&args.problem)?;
    let snapshot_text = read_text(&args.snapshot)?;
    let repaired = repair_json(
        &problem_text,
        &snapshot_text,
        &args.affected,
        &args.locked_students,
        &args.locked_seats,
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

/// Summarize historical seating snapshots (fairness report, ledger B.5).
pub fn run_history_report(args: &HistoryReportArgs) -> Result<(), String> {
    let problem_text = read_text(&args.problem)?;
    let snapshots = load_snapshot_documents(&args.history)?;
    let report = history_report_json(&problem_text, &snapshots)
        .map_err(|error| format!("history report failed: {error}"))?;
    println!("{report}");
    Ok(())
}

/// Summarize historical desk-mate / neighbor pairs (ledger B.5).
pub fn run_pair_report(args: &PairReportArgs) -> Result<(), String> {
    let problem_text = read_text(&args.problem)?;
    let snapshots = load_snapshot_documents(&args.history)?;
    let report = pair_report_json(&problem_text, &snapshots, args.top, args.within_distance)
        .map_err(|error| format!("pair report failed: {error}"))?;
    println!("{report}");
    Ok(())
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
pub fn run_candidates(args: &CandidatesArgs) -> Result<(), String> {
    let problem_text = read_text(&args.problem)?;
    let report = generate_candidates_json(&problem_text, args.count)
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
    let request: CoreSolveRequest = serde_json::from_value(problem_value).map_err(|error| {
        format!(
            "'{}' is not a valid solve problem (CoreSolveRequest): {error}",
            args.problem.display()
        )
    })?;

    let solution_text = read_text(&args.solution)?;
    let response: CoreSolveResponse = serde_json::from_str(&solution_text).map_err(|error| {
        format!(
            "'{}' is not a valid solve result (CoreSolveResponse): {error}",
            args.solution.display()
        )
    })?;

    validate_solve_response(&request, &response)
        .map_err(|message| format!("refusing to export an invalid solved plan: {message}"))?;
    let grid = SeatingGrid::build(&request, &response)?;
    match args.format {
        ExportFormat::Svg => write_text(&args.output, &crate::render::render_svg(&grid))?,
        ExportFormat::Html => write_text(&args.output, &crate::render::render_html(&grid))?,
        ExportFormat::Png => write_bytes(&args.output, &crate::render::render_png(&grid)?)?,
        ExportFormat::Pdf => write_text(&args.output, &crate::render::render_pdf(&grid))?,
    }

    let format_name = match args.format {
        ExportFormat::Svg => styler.cyan("SVG"),
        ExportFormat::Html => styler.cyan("HTML"),
        ExportFormat::Png => styler.cyan("PNG"),
        ExportFormat::Pdf => styler.cyan("PDF"),
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
