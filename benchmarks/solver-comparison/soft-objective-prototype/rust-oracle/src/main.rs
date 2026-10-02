//! Experimental scoring oracle. Calls the production core; not a new backend.
use std::collections::{HashMap, HashSet};
use std::io::{self, BufRead};
use std::time::Instant;

use seattrellis_core::cost::individual_cost;
use seattrellis_core::objectives::{
    compile_soft_objectives_with_adjacency, evaluate_soft_objectives,
};
use seattrellis_core::rng::SplitMix64;
use serde_json::{json, Value};

fn evaluate(input: Value) -> Result<Value, Box<dyn std::error::Error>> {
    let document = serde_json::to_string(&input["request"])?;
    let request = seattrellis_core::parse_core_solve_request(&document)?;
    let rules = request.rules.as_ref().ok_or("explicit rules required")?;
    let layout = request.layout.as_ref().ok_or("explicit layout required")?;
    let resolved = seattrellis_core::resolve_group_rules(&request)?;
    let edges: HashSet<(String, String)> = request
        .edges
        .iter()
        .map(|[a, b]| {
            seattrellis_core::cost::normalize_edge(
                &layout.seats[*a].seat_id,
                &layout.seats[*b].seat_id,
            )
        })
        .collect();
    let context = compile_soft_objectives_with_adjacency(
        &request.students,
        layout,
        rules,
        request.pair_history.as_ref(),
        Some(&edges),
    );
    let min_row = layout.seats.iter().map(|s| s.row).min().unwrap_or(0);
    let max_row = layout.seats.iter().map(|s| s.row).max().unwrap_or(0);
    let permutations: Vec<Vec<usize>> = serde_json::from_value(input["assignments"].clone())?;
    let mut scores = Vec::with_capacity(permutations.len());
    for permutation in permutations {
        let pairs: Vec<[usize; 2]> = permutation
            .iter()
            .enumerate()
            .map(|(student, seat)| [student, *seat])
            .collect();
        let mut evaluation_input = input["request"].clone();
        evaluation_input["assignments"] = json!(pairs);
        evaluation_input["must_be_adjacent"] = json!(resolved.must_be_adjacent);
        evaluation_input["cannot_be_adjacent"] = json!(resolved.cannot_be_adjacent);
        let hard: Value = serde_json::from_str(&seattrellis_core::evaluate_problem_json(
            &serde_json::to_string(&evaluation_input)?,
        )?)?;
        let by_key: HashMap<String, String> = permutation
            .iter()
            .enumerate()
            .map(|(i, seat)| {
                (
                    request.students[i].key.clone(),
                    layout.seats[*seat].seat_id.clone(),
                )
            })
            .collect();
        let mut rng = SplitMix64::new(request.seed);
        let mut individual = 0.0;
        for (i, seat) in permutation.iter().enumerate() {
            individual += individual_cost(
                &request.students[i],
                &layout.seats[*seat],
                layout,
                rules,
                request.history.as_ref(),
                &mut rng,
                min_row,
                max_row,
            ) as f64;
        }
        let soft = evaluate_soft_objectives(&by_key, &context, rules);
        scores.push(json!({
            "legal": hard["hard_constraints_satisfied"],
            "individual_cost": individual,
            "weighted_costs": soft.weighted_costs,
            "losses": soft.losses,
            "total_cost": individual + soft.total_cost(),
        }));
    }
    let started = Instant::now();
    let response = seattrellis_core::solve_problem(&request)?;
    let solve_seconds = started.elapsed().as_secs_f64();
    // A complete fixed assignment checks the oracle against the solver's
    // actual total-cost path; independent permutations check the other scores.
    let fixed_probe = if response.feasible {
        let mut fixed_input = input["request"].clone();
        fixed_input["fixed_seats"] = json!(response.assignment);
        Some(seattrellis_core::solve_problem(
            &seattrellis_core::parse_core_solve_request(&serde_json::to_string(&fixed_input)?)?,
        )?)
    } else {
        None
    };
    Ok(json!({"scores": scores, "rust_response": response,
              "rust_solve_seconds": solve_seconds, "fixed_probe": fixed_probe}))
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    for line in io::stdin().lock().lines() {
        let result = evaluate(serde_json::from_str(&line?)?)?;
        println!("{}", serde_json::to_string(&result)?);
    }
    Ok(())
}
