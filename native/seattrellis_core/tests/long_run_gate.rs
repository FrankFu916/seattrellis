//! Long-run and reliability gate (plan §11.9 / §6.6): 500 consecutive
//! solves without state leakage, cancellation latency bounds, cancel-then-
//! solve-again, and a planted-feasible random corpus asserting >=99.5%
//! `Solved` with zero false `ProvenInfeasible`.
//!
//! The corpus is planted: each instance starts from a random assignment and
//! derives hard constraints that assignment satisfies, so every instance is
//! feasible by construction. A solver reporting `ProvenInfeasible` on any of
//! them would be a soundness bug.

use std::thread;
use std::time::{Duration, Instant};

use seattrellis_core::{
    solve_problem, solve_problem_with_control, CoreSolveRequest, SolveControl, SolveStatus,
};
use serde_json::{json, Value};

const SIZES: [usize; 5] = [20, 40, 50, 60, 80];
const INSTANCES_PER_SIZE: usize = 20;

struct Lcg(u64);

impl Lcg {
    fn next(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        self.0
    }

    fn below(&mut self, bound: usize) -> usize {
        (self.next() % bound as u64) as usize
    }
}

fn grid_positions(count: usize) -> Vec<[f64; 2]> {
    (0..count)
        .map(|index| [(index % 6) as f64, (index / 6) as f64])
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

fn adjacent(edges: &[[usize; 2]], first: usize, second: usize) -> bool {
    edges
        .iter()
        .any(|[a, b]| (*a == first && *b == second) || (*a == second && *b == first))
}

fn euclidean(a: [f64; 2], b: [f64; 2]) -> f64 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2)).sqrt()
}

/// One planted-feasible instance: a random assignment plus constraints the
/// assignment satisfies.
fn planted_request(count: usize, seed: u64) -> Value {
    let positions = grid_positions(count);
    let edges = grid_edges(&positions);
    let mut rng = Lcg(seed ^ (count as u64) << 32);

    // Random permutation = the planted assignment.
    let mut assignment: Vec<usize> = (0..count).collect();
    for index in (1..count).rev() {
        let swap_with = rng.below(index + 1);
        assignment.swap(index, swap_with);
    }
    let seat_of = |student: usize| assignment[student];

    let mut fixed: Vec<[usize; 2]> = Vec::new();
    for _ in 0..(count / 20).clamp(1, 4) {
        let student = rng.below(count);
        if !fixed.iter().any(|[s, _]| *s == student) {
            fixed.push([student, seat_of(student)]);
        }
    }
    let mut must = Vec::new();
    let mut cannot = Vec::new();
    let mut min_distance = Vec::new();
    for first in 0..count {
        for second in (first + 1)..count {
            let roll = rng.below(48);
            let first_seat = seat_of(first);
            let second_seat = seat_of(second);
            if roll == 0 && adjacent(&edges, first_seat, second_seat) {
                must.push([first, second]);
            } else if roll == 1 && !adjacent(&edges, first_seat, second_seat) {
                cannot.push([first, second]);
            } else if roll == 2 && euclidean(positions[first_seat], positions[second_seat]) >= 1.2 {
                // A diagonal pair at planted distance >= sqrt(2): a
                // min-distance threshold the planted assignment satisfies.
                min_distance.push(([first, second], 1.1));
            }
        }
    }

    json!({
        "api_version": 2,
        "student_count": count,
        "seat_positions": positions,
        "edges": edges,
        "fixed_seats": fixed,
        "must_be_adjacent": must,
        "cannot_be_adjacent": cannot,
        "min_distance": min_distance.iter().map(|([a, b], distance)| json!({
            "students": [a, b],
            "distance": distance,
            "metric": "euclidean"
        })).collect::<Vec<_>>(),
        "seed": seed,
        "rules": { "seed": seed, "soft": {} },
    })
}

fn parse(request: &Value) -> CoreSolveRequest {
    serde_json::from_value(request.clone()).expect("planted request parses")
}

/// Linux resident-set size in bytes (CI runs this gate on ubuntu).
fn resident_set_bytes() -> Option<u64> {
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    let line = status.lines().find(|line| line.starts_with("VmRSS:"))?;
    let kb: u64 = line.split_whitespace().nth(1)?.parse().ok()?;
    Some(kb * 1024)
}

#[test]
#[ignore = "expensive: run in release mode via the CI long-run-gate step"]
fn five_hundred_consecutive_solves_are_stable() {
    let count = 40;
    let start = Instant::now();
    let rss_before = resident_set_bytes();
    let mut rss_peak = rss_before;
    for seed in 0..500u64 {
        let request = planted_request(count, seed);
        let response = solve_problem(&parse(&request)).expect("solve terminates");
        assert_eq!(
            response.status,
            SolveStatus::Solved,
            "seed {seed}: planted-feasible instance must solve"
        );
        if seed % 100 == 99 {
            if let Some(rss) = resident_set_bytes() {
                rss_peak = Some(rss_peak.map_or(rss, |peak| peak.max(rss)));
            }
        }
    }
    let elapsed = start.elapsed();
    // The 500 solves must complete in a bounded wall time (release build;
    // n=40 solves take well under a second each on CI-class hardware).
    assert!(
        elapsed < Duration::from_secs(1200),
        "500 solves took {elapsed:?}"
    );
    if let (Some(before), Some(peak)) = (rss_before, rss_peak) {
        let growth = peak.saturating_sub(before);
        // A leak of more than 64 MiB over 500 solves would be a red flag;
        // allocator caching may hold some headroom, so the bound is loose.
        assert!(
            growth < 64 * 1024 * 1024,
            "resident set grew by {growth} bytes over 500 solves"
        );
    }
}

#[test]
#[ignore = "expensive: run in release mode via the CI long-run-gate step"]
fn cancellation_is_prompt_and_a_fresh_solve_still_works() {
    // A solve running on another thread must observe the cancel within a
    // bounded latency (plan §6.1 "可取消"; §11.9 "取消正在运行的 solve 后
    // 再次 solve").
    let request = planted_request(80, 0xCA11);
    let control = SolveControl::new();
    let cancel_flag = control.clone();
    let request_clone = request.clone();
    let handle =
        thread::spawn(move || solve_problem_with_control(&parse(&request_clone), &cancel_flag));
    thread::sleep(Duration::from_millis(20));
    let started = Instant::now();
    control.cancel();
    let response = handle
        .join()
        .expect("solve thread finishes")
        .expect("no error");
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "cancelled solve took {:?} to return",
        started.elapsed()
    );
    assert_eq!(response.status, SolveStatus::Cancelled);
    assert!(response.assignment.is_empty());

    // The same request solves normally with a fresh control.
    let response = solve_problem(&parse(&request)).expect("solve terminates");
    assert_eq!(response.status, SolveStatus::Solved);
}

#[test]
#[ignore = "expensive: run in release mode via the CI long-run-gate step"]
fn planted_corpus_solves_at_least_99_5_percent_with_no_false_infeasible() {
    let mut total = 0usize;
    let mut solved = 0usize;
    let mut proven_infeasible = 0usize;
    let mut unknown = 0usize;
    let start = Instant::now();
    for count in SIZES {
        for instance in 0..INSTANCES_PER_SIZE {
            let seed = (count as u64) * 10_000 + instance as u64;
            let request = planted_request(count, seed);
            let response = solve_problem(&parse(&request)).expect("solve terminates");
            total += 1;
            match response.status {
                SolveStatus::Solved => solved += 1,
                SolveStatus::ProvenInfeasible => {
                    proven_infeasible += 1;
                    eprintln!(
                        "FALSE ProvenInfeasible: count={count} instance={instance} seed={seed}"
                    );
                }
                SolveStatus::Unknown => {
                    unknown += 1;
                    eprintln!("Unknown: count={count} instance={instance} seed={seed}");
                }
                other => panic!("unexpected status {other:?} on a planted-feasible instance"),
            }
        }
    }
    let elapsed = start.elapsed();
    let rate = solved as f64 / total as f64;
    eprintln!(
        "planted corpus: {total} instances in {elapsed:?}, solved {solved} ({rate:.4}), \
         proven_infeasible {proven_infeasible}, unknown {unknown}"
    );
    assert_eq!(
        proven_infeasible, 0,
        "false ProvenInfeasible on planted-feasible instances (soundness bug)"
    );
    assert!(rate >= 0.995, "solved rate {rate:.4} below the 99.5% gate");
    assert!(
        elapsed < Duration::from_secs(3600),
        "corpus took {elapsed:?}"
    );
}
