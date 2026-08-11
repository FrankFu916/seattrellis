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
    Ok(lines.join("\n"))
}

/// `project-validate`: validate the project document, its referenced files,
/// and the compiled solve request.
/// The project's display name (for plan/artifact naming).
pub fn project_name(project_path: &Path) -> Result<String, String> {
    let (project, _) = seattrellis_io::projects::load_project_document(project_path)?;
    Ok(project
        .get("name")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("SeatTrellis")
        .to_string())
}

pub fn project_validate(project_path: &Path) -> Result<String, String> {
    let request = build_request(project_path)?;
    let request_json = serde_json::to_string(&request)
        .map_err(|error| format!("could not serialize the compiled request: {error}"))?;
    seattrellis_core::validate_solve_request_json(&request_json)
        .map_err(|error| format!("project is invalid: {error}"))?;
    let parsed: CoreSolveRequest = serde_json::from_str(&request_json)
        .map_err(|error| format!("compiled request is malformed: {error}"))?;
    Ok(format!(
        "valid: true\nstudents: {}\nseats: {}\nedges: {}\nhard rules: {} fixed, {} must, {} cannot, {} min-distance",
        parsed.student_count,
        parsed.seat_positions.len(),
        parsed.edges.len(),
        parsed.fixed_seats.len(),
        parsed.must_be_adjacent.len(),
        parsed.cannot_be_adjacent.len(),
        parsed.min_distance.len(),
    ))
}
