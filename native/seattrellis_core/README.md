# seattrellis_core

Cost-ranked classroom seating solver core for [SeatTrellis](https://github.com/FrankFu916/seattrellis).

Provides a deterministic, dependency-light solver that assigns students to seats
while satisfying hard constraints (fixed seats, must/cannot-be-adjacent,
minimum distance) and optimizing soft objectives (score position preference,
score distribution balance, and mentor pairing). All input and output is plain
JSON, so the core is usable from any language with a JSON bridge.

```rust
let response_json = seattrellis_core::solve_problem_json(&request_json)?;
```

Licensed under Apache-2.0.
