# seattrellis_core

Cost-ranked classroom seating solver core for [SeatTrellis](https://github.com/FrankFu916/seattrellis) (席序).

Provides a deterministic, dependency-light solver that assigns students to seats while satisfying hard constraints (fixed seats, must/cannot-be-adjacent, minimum distance) and optimizing soft objectives (score position preference, score distribution balance, mentor pairing, height and vision preferences, fairness and recent-neighbor avoidance). All input and output is plain JSON, so the core is usable from any language with a JSON bridge.

```rust
let response_json = seattrellis_core::solve_problem_json(&request_json)?;
```

## Solver status

Seven frozen statuses: `Solved`, `ProvenInfeasible`, `Timeout`, `Unknown`, `InvalidInput`, `Cancelled`, `InternalError`. Heuristic exhaustion reports `Unknown` — never a false proof of infeasibility.

## Features

- Deterministic, seeded search with MRV/backtracking for hard constraints and local search for soft objectives
- Candidate-set generation with diversity and stability dimensions
- Independent validator for every produced plan
- Works for classes up to 10,000 seats (input boundary enforced)

License: Apache-2.0.
