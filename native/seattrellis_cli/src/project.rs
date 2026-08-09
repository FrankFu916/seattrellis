//! Project workflows (plan §5.5): build a `CoreSolveRequest` from a portable
//! project workspace and run the project lifecycle commands
//! (`project-info` / `project-validate` / `project-solve` / `project-export`).

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde_json::{json, Value};

use seattrellis_core::CoreSolveRequest;

/// Read a project document and resolve its sibling file references, mirroring
/// the io layer's `resolve_project` (which stays private to the library).
pub fn load_project(project_path: &Path) -> Result<(Value, PathBuf), String> {
    let text = std::fs::read_to_string(project_path).map_err(|error| {
        format!(
            "could not read project file {}: {error}",
            project_path.display()
        )
    })?;
    let document: Value = serde_json::from_str(&text)
        .map_err(|error| format!("project file is not valid JSON: {error}"))?;
    if document.get("kind").and_then(Value::as_str) != Some("seattrellis_project") {
        return Err(format!(
            "{} is not a SeatTrellis project file",
            project_path.display()
        ));
    }
    let root = project_path
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));
    Ok((document, root))
}

/// Resolve a required sibling reference, rejecting absolute paths and `..`
/// escapes (mirrors the io layer's relative-path validation).
fn resolve_sibling(root: &Path, raw: &str, label: &str) -> Result<PathBuf, String> {
    let path = Path::new(raw);
    if path.is_absolute()
        || path
            .components()
            .any(|component| !matches!(component, std::path::Component::Normal(_)))
    {
        return Err(format!(
            "{label} must be a relative path inside the project: {raw:?}"
        ));
    }
    let resolved = root.join(path);
    if !resolved.is_file() {
        return Err(format!("{label} does not exist: {}", resolved.display()));
    }
    Ok(resolved)
}

/// Build the core `CoreSolveRequest` JSON from a project workspace:
/// roster CSV -> students, layout JSON -> seat grid, rules JSON -> soft rules
/// + resolved hard-rule index pairs.
pub fn build_request(project_path: &Path) -> Result<Value, String> {
    let (project, root) = load_project(project_path)?;

    let students_raw = project
        .get("students")
        .and_then(Value::as_str)
        .ok_or_else(|| "project file is missing 'students'".to_string())?;
    let layout_raw = project
        .get("layout")
        .and_then(Value::as_str)
        .ok_or_else(|| "project file is missing 'layout'".to_string())?;
    let rules_raw = project
        .get("rules")
        .and_then(Value::as_str)
        .ok_or_else(|| "project file is missing 'rules'".to_string())?;

    let students_path = resolve_sibling(&root, students_raw, "students")?;
    let layout_path = resolve_sibling(&root, layout_raw, "layout")?;
    let rules_path = resolve_sibling(&root, rules_raw, "rules")?;

    // Roster CSV -> core student records (automatic header mapping).
    let roster_bytes = std::fs::read(&students_path)
        .map_err(|error| format!("could not read {}: {error}", students_path.display()))?;
    let students = seattrellis_io::roster::parse_roster_students(&roster_bytes)?;
    let core_students: Vec<Value> = students
        .iter()
        .map(|student| {
            let key = student
                .student_id
                .clone()
                .filter(|id| !id.is_empty())
                .or_else(|| student.name.clone())
                .unwrap_or_default();
            json!({
                "key": key,
                "display_name": student.name,
                "height_cm": student.height_cm,
                "score": student.score,
                "vision": student.vision.as_ref().map(|v| match v {
                    seattrellis_io::roster::VisionValue::Num(value) => value.to_string(),
                    seattrellis_io::roster::VisionValue::Str(value) => value.clone(),
                }),
                "tags": student.tags,
                "needs": student.needs,
            })
        })
        .collect();
    if core_students.iter().any(|student| {
        student
            .get("key")
            .and_then(Value::as_str)
            .unwrap_or("")
            .is_empty()
    }) {
        return Err("roster rows must carry a student_id or name".to_string());
    }

    // Layout JSON -> enabled seat grid + adjacency.
    let layout_text = std::fs::read_to_string(&layout_path)
        .map_err(|error| format!("could not read {}: {error}", layout_path.display()))?;
    let layout: Value = serde_json::from_str(&layout_text)
        .map_err(|error| format!("layout file is not valid JSON: {error}"))?;
    let seats = layout
        .get("seats")
        .and_then(Value::as_array)
        .ok_or_else(|| "layout has no seats array".to_string())?;
    let enabled: Vec<&Value> = seats
        .iter()
        .filter(|seat| seat.get("enabled").and_then(Value::as_bool).unwrap_or(true))
        .collect();
    if enabled.is_empty() {
        return Err("layout has no enabled seats".to_string());
    }
    let seat_positions: Vec<[f64; 2]> = enabled
        .iter()
        .map(|seat| {
            let x = seat.get("x").and_then(Value::as_f64).unwrap_or(0.0);
            let y = seat.get("y").and_then(Value::as_f64).unwrap_or(0.0);
            [x, y]
        })
        .collect();
    let seat_index_by_id: HashMap<&str, usize> = enabled
        .iter()
        .enumerate()
        .filter_map(|(index, seat)| {
            seat.get("seat_id")
                .and_then(Value::as_str)
                .map(|seat_id| (seat_id, index))
        })
        .collect();
    // The core requires layout.seats to be aligned with seat_positions
    // (layout.seats[i] <-> seat_positions[i]), so strip disabled seats.
    let core_layout_value = json!({
        "layout_id": layout.get("layout_id").cloned().unwrap_or_else(|| json!("project")),
        "name": layout.get("name").cloned().unwrap_or_else(|| json!("Project")),
        "seats": enabled,
        "adjacency": layout.get("adjacency").cloned().unwrap_or_else(|| json!({})),
    });
    let core_layout: seattrellis_core::models::Layout =
        serde_json::from_value(core_layout_value)
            .map_err(|error| format!("layout is not core-compatible: {error}"))?;
    let mut edges: Vec<[usize; 2]> = Vec::new();
    for (first, second) in seattrellis_core::objectives::build_adjacency_edges(&core_layout) {
        let (Some(&first_index), Some(&second_index)) = (
            seat_index_by_id.get(first.as_str()),
            seat_index_by_id.get(second.as_str()),
        ) else {
            continue;
        };
        edges.push([first_index.min(second_index), first_index.max(second_index)]);
    }
    edges.sort_unstable();
    edges.dedup();

    // Rules JSON -> soft rules + resolved hard-rule index pairs.
    let rules_text = std::fs::read_to_string(&rules_path)
        .map_err(|error| format!("could not read {}: {error}", rules_path.display()))?;
    let rules: Value = serde_json::from_str(&rules_text)
        .map_err(|error| format!("rules file is not valid JSON: {error}"))?;
    let student_index: HashMap<&str, usize> = core_students
        .iter()
        .enumerate()
        .filter_map(|(index, student)| {
            student
                .get("key")
                .and_then(Value::as_str)
                .map(|key| (key, index))
        })
        .collect();

    let resolve_pair = |pair: &Value| -> Result<[usize; 2], String> {
        // Accept both the pair-rule object {students: [k1, k2]} and the
        // plain [k1, k2] array (Python PairRule vs index-pair shapes).
        let list = pair
            .get("students")
            .and_then(Value::as_array)
            .or_else(|| pair.as_array())
            .ok_or_else(|| "hard rule pair must be {students: [a, b]} or [a, b]".to_string())?;
        let first = list
            .first()
            .and_then(Value::as_str)
            .and_then(|key| student_index.get(key).copied())
            .ok_or_else(|| format!("hard rule references unknown student: {:?}", pair))?;
        let second = list
            .get(1)
            .and_then(Value::as_str)
            .and_then(|key| student_index.get(key).copied())
            .ok_or_else(|| format!("hard rule references unknown student: {:?}", pair))?;
        Ok([first.min(second), first.max(second)])
    };

    let hard = rules.get("hard").cloned().unwrap_or_else(|| json!({}));
    let mut fixed_seats: Vec<[usize; 2]> = Vec::new();
    if let Some(list) = hard.get("fixed_seats").and_then(Value::as_array) {
        for entry in list {
            let student = entry.get("student").and_then(Value::as_str).unwrap_or("");
            let seat_id = entry.get("seat_id").and_then(Value::as_str).unwrap_or("");
            let student_index = student_index
                .get(student)
                .copied()
                .ok_or_else(|| format!("fixed seat references unknown student {student:?}"))?;
            let seat_index = seat_index_by_id
                .get(seat_id)
                .copied()
                .ok_or_else(|| format!("fixed seat references unknown seat {seat_id:?}"))?;
            fixed_seats.push([student_index, seat_index]);
        }
    }
    let must_be_adjacent = hard
        .get("must_be_adjacent")
        .and_then(Value::as_array)
        .map(|pairs| {
            pairs
                .iter()
                .map(resolve_pair)
                .collect::<Result<Vec<_>, _>>()
        })
        .transpose()?
        .unwrap_or_default();
    let cannot_be_adjacent = hard
        .get("cannot_be_adjacent")
        .and_then(Value::as_array)
        .map(|pairs| {
            pairs
                .iter()
                .map(resolve_pair)
                .collect::<Result<Vec<_>, _>>()
        })
        .transpose()?
        .unwrap_or_default();
    let min_distance: Vec<Value> = hard
        .get("min_distance")
        .and_then(Value::as_array)
        .map(|rules| {
            rules
                .iter()
                .map(|rule| -> Result<Value, String> {
                    let students = rule
                        .get("students")
                        .and_then(Value::as_array)
                        .ok_or_else(|| "min_distance rule is missing students".to_string())?;
                    let first = students
                        .first()
                        .and_then(Value::as_str)
                        .and_then(|key| student_index.get(key).copied())
                        .ok_or_else(|| "min_distance references unknown student".to_string())?;
                    let second = students
                        .get(1)
                        .and_then(Value::as_str)
                        .and_then(|key| student_index.get(key).copied())
                        .ok_or_else(|| "min_distance references unknown student".to_string())?;
                    let distance = rule
                        .get("distance")
                        .and_then(Value::as_f64)
                        .ok_or_else(|| "min_distance rule is missing distance".to_string())?;
                    let metric = rule
                        .get("metric")
                        .and_then(Value::as_str)
                        .unwrap_or("euclidean")
                        .to_string();
                    Ok(json!({
                        "students": [first, second],
                        "distance": distance,
                        "metric": metric,
                    }))
                })
                .collect::<Result<Vec<_>, _>>()
        })
        .transpose()?
        .unwrap_or_default();

    Ok(json!({
        "api_version": 2,
        "student_count": core_students.len(),
        "seat_positions": seat_positions,
        "edges": edges,
        "fixed_seats": fixed_seats,
        "must_be_adjacent": must_be_adjacent,
        "cannot_be_adjacent": cannot_be_adjacent,
        "min_distance": min_distance,
        "seed": rules.get("seed").and_then(Value::as_u64).unwrap_or(42),
        "students": core_students,
        "layout": json!({
            "layout_id": layout.get("layout_id").cloned().unwrap_or_else(|| json!("project")),
            "name": layout.get("name").cloned().unwrap_or_else(|| json!("Project")),
            "seats": enabled,
            "adjacency": layout.get("adjacency").cloned().unwrap_or_else(|| json!({})),
        }),
        "rules": {
            "seed": rules.get("seed").and_then(Value::as_u64).unwrap_or(42),
            "soft": rules.get("soft").cloned().unwrap_or_else(|| json!({})),
            "groups": rules.get("groups").cloned().unwrap_or_else(|| json!([])),
        },
    }))
}

/// `project-info`: print the project document (no referenced files touched).
pub fn project_info(project_path: &Path) -> Result<String, String> {
    let (project, root) = load_project(project_path)?;
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
        let exists = resolve_sibling(&root, raw, field).is_ok();
        lines.push(format!(
            "{field:<10} {raw} ({})",
            if exists { "ok" } else { "missing" }
        ));
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
