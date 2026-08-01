//! Solve and export command flows.
//!
//! `run_solve` reads a `CoreSolveRequest` JSON file, runs the core solver
//! (via the public `solve_problem_json` entry point), prints a human-readable
//! summary to stdout and optionally writes the `CoreSolveResponse` JSON.
//!
//! `run_export` reads both the problem and the solve result, recovers the seat
//! grid, and renders SVG or HTML through `render`.

use std::path::Path;

use seattrellis_core::{solve_problem_json, CoreSolveRequest, CoreSolveResponse};

use crate::render::SeatingGrid;
use crate::{ExportArgs, ExportFormat, SolveArgs};

pub fn run_solve(args: &SolveArgs) -> Result<(), String> {
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
    let request_json = serde_json::to_string(&problem)
        .map_err(|error| format!("could not re-encode the problem: {error}"))?;

    let response_json = solve_problem_json(&request_json)
        .map_err(|error| format!("solver rejected the problem: {error}"))?;
    let response: CoreSolveResponse = serde_json::from_str(&response_json)
        .map_err(|error| format!("solver returned malformed JSON: {error}"))?;

    // Human-readable summary on stdout.
    println!("feasible: {}", response.feasible);
    println!("hard_constraints_satisfied: {}", response.hard_constraints_satisfied);
    println!("attempts_used: {}", response.attempts_used);
    match response.total_cost {
        Some(cost) => println!("total_cost: {cost}"),
        None => println!("total_cost: none"),
    }
    println!("students seated: {}", response.assignment.len());

    if let Some(output) = &args.output {
        let pretty = serde_json::to_string_pretty(&response)
            .map_err(|error| format!("could not encode the result: {error}"))?;
        write_text(output, &format!("{pretty}\n"))?;
        println!("wrote result JSON to '{}'", output.display());
    }
    Ok(())
}

pub fn run_export(args: &ExportArgs) -> Result<(), String> {
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
    let output_text = match args.format {
        ExportFormat::Svg => crate::render::render_svg(&grid),
        ExportFormat::Html => crate::render::render_html(&grid),
    };
    write_text(&args.output, &output_text)?;

    let format_name = match args.format {
        ExportFormat::Svg => "SVG",
        ExportFormat::Html => "HTML",
    };
    println!(
        "wrote {format_name} seating plan ({}/{} seats) to '{}'",
        response.assignment.len(),
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
