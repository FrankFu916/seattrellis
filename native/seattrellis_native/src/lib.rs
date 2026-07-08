use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use seattrellis_core::{assignment_is_unique as core_assignment_is_unique, seat_distance as core_seat_distance};

const VERSION: &str = env!("CARGO_PKG_VERSION");

#[pyfunction]
fn assignment_is_unique(
    student_count: usize,
    seat_count: usize,
    assignments: Vec<(usize, usize)>,
) -> PyResult<bool> {
    Ok(core_assignment_is_unique(student_count, seat_count, &assignments))
}

#[pyfunction]
fn seat_distance(first_row: f64, first_col: f64, second_row: f64, second_col: f64) -> PyResult<f64> {
    match core_seat_distance(first_row, first_col, second_row, second_col) {
        Some(distance) => Ok(distance),
        None => Err(PyValueError::new_err("seat coordinates must be finite numbers")),
    }
}

#[pymodule]
fn seattrellis_native(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add("__version__", VERSION)?;
    module.add_function(wrap_pyfunction!(assignment_is_unique, module)?)?;
    module.add_function(wrap_pyfunction!(seat_distance, module)?)?;
    Ok(())
}
