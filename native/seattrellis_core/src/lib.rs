pub const NATIVE_API_VERSION: u32 = 1;

pub fn assignment_is_unique(
    student_count: usize,
    seat_count: usize,
    assignments: &[(usize, usize)],
) -> bool {
    if assignments.len() != student_count {
        return false;
    }
    let mut seen_students = vec![false; student_count];
    let mut seen_seats = vec![false; seat_count];
    for &(student_index, seat_index) in assignments {
        if student_index >= student_count || seat_index >= seat_count {
            return false;
        }
        if seen_students[student_index] || seen_seats[seat_index] {
            return false;
        }
        seen_students[student_index] = true;
        seen_seats[seat_index] = true;
    }
    seen_students.into_iter().all(|seen| seen)
}

pub fn seat_distance(first_x: f64, first_y: f64, second_x: f64, second_y: f64) -> Option<f64> {
    if !(first_x.is_finite() && first_y.is_finite() && second_x.is_finite() && second_y.is_finite())
    {
        return None;
    }
    let x_delta = first_x - second_x;
    let y_delta = first_y - second_y;
    Some((x_delta * x_delta + y_delta * y_delta).sqrt())
}

#[cfg(test)]
mod tests {
    use super::{assignment_is_unique, seat_distance, NATIVE_API_VERSION};

    #[test]
    fn exposes_expected_native_api_version() {
        assert_eq!(NATIVE_API_VERSION, 1);
    }

    #[test]
    fn accepts_complete_unique_assignment() {
        let assignments = vec![(0, 1), (1, 0), (2, 2)];
        assert!(assignment_is_unique(3, 3, &assignments));
    }

    #[test]
    fn rejects_duplicate_student_or_seat() {
        assert!(!assignment_is_unique(2, 2, &[(0, 0), (0, 1)]));
        assert!(!assignment_is_unique(2, 2, &[(0, 0), (1, 0)]));
    }

    #[test]
    fn rejects_missing_or_out_of_bounds_assignment() {
        assert!(!assignment_is_unique(2, 2, &[(0, 0)]));
        assert!(!assignment_is_unique(2, 2, &[(0, 0), (2, 1)]));
        assert!(!assignment_is_unique(2, 2, &[(0, 0), (1, 2)]));
    }

    #[test]
    fn computes_euclidean_distance() {
        assert_eq!(seat_distance(1.0, 1.0, 4.0, 5.0), Some(5.0));
        assert_eq!(seat_distance(f64::NAN, 1.0, 4.0, 5.0), None);
    }
}
