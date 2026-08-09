//! Solve and export command flows.
//!
//! `run_solve` reads a `CoreSolveRequest` JSON file, runs the core solver
//! (via the public `solve_problem_json` entry point), prints a human-readable
//! summary to stdout and optionally writes the `CoreSolveResponse` JSON.
//!
//! `run_export` reads both the problem and the solve result, recovers the seat
//! grid, and renders SVG or HTML through `render`.

use std::path::{Path, PathBuf};

use seattrellis_core::{
    audit_report_json, generate_candidates_json, history_report_json, pair_report_json,
    precheck_report_json, solve_problem_json, validate_solve_request_json, CoreSolveRequest,
    CoreSolveResponse, SolveStatus,
};

use crate::render::SeatingGrid;
use crate::style::Styler;
use crate::{
    AuditArgs, CandidatesArgs, ExportArgs, ExportFormat, HistoryReportArgs, PairReportArgs,
    PrecheckArgs, SolveArgs,
};
use crate::ValidateArgs;

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
    println!("{}: {}", styler.bold("status"), styler.cyan(response.status.as_str()));

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
    std::fs::write(path, text)
        .map_err(|error| format!("cannot write '{}': {error}", path.display()))
}

fn write_bytes(path: &Path, bytes: &[u8]) -> Result<(), String> {
    std::fs::write(path, bytes)
        .map_err(|error| format!("cannot write '{}': {error}", path.display()))
}
