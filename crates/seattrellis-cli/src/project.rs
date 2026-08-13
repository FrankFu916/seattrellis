//! Project workflows (plan §5.5): thin transport over the io-layer project
//! workspace API. Workspace parsing, reference containment and rule
//! resolution live in `seattrellis_io::projects`; the CLI only injects
//! per-run options (seed / time limit) and renders output.

use std::path::Path;

use serde_json::Value;

use seattrellis_core::CoreSolveRequest;

/// Build the core `CoreSolveRequest` JSON from a project workspace,
/// delegating roster/layout/rules parsing and hard-rule resolution to the
/// io layer (canonical containment included).
pub fn build_request(project_path: &Path) -> Result<Value, String> {
    seattrellis_io::projects::build_project_solve_request(project_path)
}

/// `project-info`: print the project document (no referenced files touched).
pub fn project_info(project_path: &Path) -> Result<String, String> {
    let (project, root) = seattrellis_io::projects::load_project_document(project_path)?;
    let name = project
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or("SeatTrellis Project");
    let mut lines = Vec::new();
    lines.push(format!("project:    {}", project_path.display()));
    lines.push(format!("name:       {name}"));
    lines.push(format!("root:       {}", root.display()));
    for field in ["students", "layout", "rules"] {
        let raw = project.get(field).and_then(Value::as_str).unwrap_or("");
        let status = seattrellis_io::projects::resolve_project_reference(&root, raw, field)
            .map(|_| "ok".to_string())
            .unwrap_or_else(|error| format!("missing ({error})"));
        lines.push(format!("{field:<10} {raw} ({status})"));
    }
    lines.push(format!(
        "outputs_dir: {}",
        project
            .get("outputs_dir")
            .and_then(Value::as_str)
            .unwrap_or("outputs")
    ));
    // "Defaults:" section mirrors Python `compute_project_info`
    // (service.py): candidates / candidate / export format.
    let defaults = seattrellis_io::projects::project_defaults(project_path)?;
    lines.push("Defaults:".to_string());
    lines.push(format!("- candidates: {}", defaults.candidates));
    lines.push(format!("- candidate: {}", defaults.candidate));
    lines.push(format!(
        "- export format: {}",
        defaults.export_format.as_str()
    ));
    Ok(lines.join("\n"))
}

/// The project's display name (for plan/artifact naming).
pub fn project_name(project_path: &Path) -> Result<String, String> {
    let (project, _) = seattrellis_io::projects::load_project_document(project_path)?;
    Ok(project
        .get("name")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("SeatTrellis")
        .to_string())
}

/// `project-validate`: validate the project document, its referenced files,
/// and the compiled solve request. `--strict` turns warnings into failures
/// (oracle `project_validate --strict`; warnings become errors -> non-zero
/// exit), judged on the same rule-capability warnings as `validate`.
pub fn project_validate(project_path: &Path, strict: bool) -> Result<String, String> {
    let request = build_request(project_path)?;
    let request_json = serde_json::to_string(&request)
        .map_err(|error| format!("could not serialize the compiled request: {error}"))?;
    seattrellis_core::validate_solve_request_json(&request_json)
        .map_err(|error| format!("project is invalid: {error}"))?;
    let parsed: CoreSolveRequest = serde_json::from_str(&request_json)
        .map_err(|error| format!("compiled request is malformed: {error}"))?;
    let mut report = format!(
        "valid: true\nstudents: {}\nseats: {}\nedges: {}\nhard rules: {} fixed, {} must, {} cannot, {} min-distance",
        parsed.student_count,
        parsed.seat_positions.len(),
        parsed.edges.len(),
        parsed.fixed_seats.len(),
        parsed.must_be_adjacent.len(),
        parsed.cannot_be_adjacent.len(),
        parsed.min_distance.len(),
    );
    // The same rule-capability warnings `validate` reports (no preset and no
    // history reach the project path, mirroring the oracle `project_validate`
    // which passes only students/layout/rules/strict to `run_validate`).
    let warnings = crate::commands::capability_warnings(&parsed);
    if !warnings.is_empty() {
        report.push_str(&format!("\nwarnings: {}", warnings.len()));
        for warning in &warnings {
            report.push_str(&format!("\n- {warning}"));
        }
    }
    if strict && !warnings.is_empty() {
        return Err(format!(
            "Warnings treated as errors by --strict:\n{}",
            warnings
                .iter()
                .map(|warning| format!("- {warning}"))
                .collect::<Vec<_>>()
                .join("\n")
        ));
    }
    Ok(report)
}
