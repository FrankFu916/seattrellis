use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use seattrellis_core::{
    assignment_is_unique as core_assignment_is_unique,
    evaluate_problem_json as core_evaluate_problem_json, seat_distance as core_seat_distance,
    solve_problem_json as core_solve_problem_json, NATIVE_API_VERSION,
};

const VERSION: &str = env!("CARGO_PKG_VERSION");

#[pyfunction]
fn assignment_is_unique(
    student_count: usize,
    seat_count: usize,
    assignments: Vec<(usize, usize)>,
) -> PyResult<bool> {
    Ok(core_assignment_is_unique(
        student_count,
        seat_count,
        &assignments,
    ))
}

#[pyfunction]
fn seat_distance(first_x: f64, first_y: f64, second_x: f64, second_y: f64) -> PyResult<f64> {
    match core_seat_distance(first_x, first_y, second_x, second_y) {
        Some(distance) => Ok(distance),
        None => Err(PyValueError::new_err(
            "seat coordinates must be finite numbers",
        )),
    }
}

#[pyfunction]
fn evaluate_problem(request_json: String) -> PyResult<String> {
    core_evaluate_problem_json(&request_json).map_err(PyValueError::new_err)
}

#[pyfunction]
fn solve_problem(request_json: String) -> PyResult<String> {
    core_solve_problem_json(&request_json).map_err(PyValueError::new_err)
}

#[pymodule]
fn seattrellis_native(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add("__version__", VERSION)?;
    module.add("NATIVE_API_VERSION", NATIVE_API_VERSION)?;
    module.add_function(wrap_pyfunction!(assignment_is_unique, module)?)?;
    module.add_function(wrap_pyfunction!(seat_distance, module)?)?;
    module.add_function(wrap_pyfunction!(evaluate_problem, module)?)?;
    module.add_function(wrap_pyfunction!(solve_problem, module)?)?;
    Ok(())
}
