//! Exact differential gate for small hard-constraint problems.
//!
//! The v2 plan requires a brute-force oracle for n <= 8 so a heuristic search
//! can never turn exhaustion into a false `ProvenInfeasible`.  This test keeps
//! the oracle deliberately independent from the core validator/search: it
//! enumerates seat permutations and checks the wire-level hard rules directly.

use std::collections::{HashSet, VecDeque};

use seattrellis_core::{solve_problem_json, CoreSolveResponse, SolveStatus};
use serde_json::{json, Value};

#[derive(Clone)]
struct ExactProblem {
    positions: Vec<[f64; 2]>,
    edges: Vec<[usize; 2]>,
    fixed: Vec<[usize; 2]>,
    must: Vec<[usize; 2]>,
    cannot: Vec<[usize; 2]>,
    min_euclidean: Vec<([usize; 2], f64)>,
}

impl ExactProblem {
    fn student_count(&self) -> usize {
        self.positions.len()
    }

    fn request(&self, seed: u64) -> Value {
        json!({
            "api_version": 2,
            "student_count": self.student_count(),
            "seat_positions": self.positions,
            "edges": self.edges,
            "fixed_seats": self.fixed,
            "must_be_adjacent": self.must,
            "cannot_be_adjacent": self.cannot,
            "min_distance": self.min_euclidean.iter().map(|(students, distance)| json!({
                "students": students,
                "distance": distance,
                "metric": "euclidean"
            })).collect::<Vec<_>>(),
            "seed": seed
        })
    }

    fn assignment_is_feasible(&self, assignment: &[usize]) -> bool {
        if assignment.len() != self.student_count() {
            return false;
        }
        let mut seen = vec![false; self.positions.len()];
        for seat in assignment {
            if *seat >= seen.len() || std::mem::replace(&mut seen[*seat], true) {
                return false;
            }
        }
        if self
            .fixed
            .iter()
            .any(|[student, seat]| assignment[*student] != *seat)
        {
            return false;
        }
        let edges: HashSet<(usize, usize)> = self
            .edges
            .iter()
            .map(|[a, b]| ((*a).min(*b), (*a).max(*b)))
            .collect();
        let adjacent = |first_student: usize, second_student: usize| {
            let first = assignment[first_student];
            let second = assignment[second_student];
            edges.contains(&(first.min(second), first.max(second)))
        };
        if self
            .must
            .iter()
            .any(|[first, second]| !adjacent(*first, *second))
        {
            return false;
        }
        if self
            .cannot
            .iter()
            .any(|[first, second]| adjacent(*first, *second))
        {
            return false;
        }
        !self.min_euclidean.iter().any(|([first, second], minimum)| {
            let a = self.positions[assignment[*first]];
            let b = self.positions[assignment[*second]];
            let dx = a[0] - b[0];
            let dy = a[1] - b[1];
            (dx * dx + dy * dy).sqrt() < *minimum
        })
    }

    fn exact_solution(&self) -> Option<Vec<usize>> {
        let mut assignment = vec![usize::MAX; self.student_count()];
        let mut unused: VecDeque<usize> = (0..self.positions.len()).collect();
        self.enumerate(0, &mut assignment, &mut unused)
    }

    fn enumerate(
        &self,
        student: usize,
        assignment: &mut [usize],
        unused: &mut VecDeque<usize>,
    ) -> Option<Vec<usize>> {
        if student == assignment.len() {
            return self
                .assignment_is_feasible(assignment)
                .then(|| assignment.to_vec());
        }
        let candidates = unused.len();
        for _ in 0..candidates {
            let seat = unused.pop_front().expect("candidate exists");
            let fixed_mismatch = self.fixed.iter().any(|[fixed_student, fixed_seat]| {
                *fixed_student == student && *fixed_seat != seat
            });
            if !fixed_mismatch {
                assignment[student] = seat;
                if let Some(solution) = self.enumerate(student + 1, assignment, unused) {
                    unused.push_back(seat);
                    return Some(solution);
                }
                assignment[student] = usize::MAX;
            }
            unused.push_back(seat);
        }
        None
    }
}

/// Tiny deterministic generator so this Gate has no random/dev dependency.
struct Lcg(u64);

impl Lcg {
    fn next(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        self.0
    }

    fn chance(&mut self, numerator: u64, denominator: u64) -> bool {
        self.next() % denominator < numerator
    }
}

fn grid_positions(count: usize) -> Vec<[f64; 2]> {
    (0..count)
        .map(|index| [(index % 4) as f64, (index / 4) as f64])
        .collect()
}

fn grid_edges(positions: &[[f64; 2]]) -> Vec<[usize; 2]> {
    let mut edges = Vec::new();
    for first in 0..positions.len() {
        for second in (first + 1)..positions.len() {
            let dx = (positions[first][0] - positions[second][0]).abs();
            let dy = (positions[first][1] - positions[second][1]).abs();
            if (dx == 1.0 && dy == 0.0) || (dx == 0.0 && dy == 1.0) {
                edges.push([first, second]);
            }
        }
    }
    edges
}

fn generated_problem(count: usize, rng: &mut Lcg) -> ExactProblem {
    let positions = grid_positions(count);
    let edges = grid_edges(&positions);
    let mut must = Vec::new();
    let mut cannot = Vec::new();
    let mut min_euclidean = Vec::new();
    for first in 0..count {
        for second in (first + 1)..count {
            if rng.chance(1, 10) {
                must.push([first, second]);
            } else if rng.chance(1, 6) {
                cannot.push([first, second]);
            } else if rng.chance(1, 12) {
                min_euclidean.push(([first, second], 1.1));
            }
        }
    }
    let fixed = if rng.chance(1, 4) {
        vec![[(rng.next() as usize) % count, (rng.next() as usize) % count]]
    } else {
        Vec::new()
    };
    ExactProblem {
        positions,
        edges,
        fixed,
        must,
        cannot,
        min_euclidean,
    }
}

fn response_assignment(response: &CoreSolveResponse, student_count: usize) -> Option<Vec<usize>> {
    if response.assignment.len() != student_count {
        return None;
    }
    let mut assignment = vec![usize::MAX; student_count];
    for [student, seat] in &response.assignment {
        if *student >= student_count || assignment[*student] != usize::MAX {
            return None;
        }
        assignment[*student] = *seat;
    }
    assignment
        .iter()
        .all(|seat| *seat != usize::MAX)
        .then_some(assignment)
}

#[test]
fn n_le_8_status_matches_independent_exhaustive_oracle() {
    let mut rng = Lcg(0x005e_a77e_1115);
    let mut checked = 0usize;
    for student_count in 1..=8 {
        for case_index in 0..24 {
            let problem = generated_problem(student_count, &mut rng);
            let exact = problem.exact_solution();
            let request = serde_json::to_string(&problem.request(case_index)).unwrap();
            let response: CoreSolveResponse = serde_json::from_str(
                &solve_problem_json(&request)
                    .unwrap_or_else(|error| panic!("valid generated request rejected: {error}")),
            )
            .unwrap();

            match response.status {
                SolveStatus::Solved => {
                    let assignment = response_assignment(&response, student_count)
                        .expect("Solved must contain one assignment per student");
                    assert!(
                        problem.assignment_is_feasible(&assignment),
                        "Solved assignment violates the independent oracle; request={request}"
                    );
                    assert!(
                        exact.is_some(),
                        "solver found a plan the exhaustive oracle missed; request={request}"
                    );
                }
                SolveStatus::ProvenInfeasible => assert!(
                    exact.is_none(),
                    "false ProvenInfeasible; witness={exact:?}; request={request}"
                ),
                other => panic!("n<=8 must finish exhaustively, got {other:?}; request={request}"),
            }
            checked += 1;
        }
    }
    assert_eq!(checked, 192);
}
