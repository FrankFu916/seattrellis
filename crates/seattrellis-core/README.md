# seattrellis-core

High-performance, constraint-satisfaction seating solver core for [SeatTrellis (席序)](https://github.com/FrankFu916/seattrellis).

---

## ⚡ Overview

`seattrellis-core` is the algorithmic engine of SeatTrellis. It provides deterministic, dependency-light solving capabilities that assign students to seats while strictly guaranteeing hard constraints (fixed seats, required/forbidden adjacency, minimum distance) and optimizing multi-dimensional soft preferences (vision accommodation, height ordering, score balancing, historical room-category rotation, and recent-neighbor avoidance).

```rust
let response_json = seattrellis_core::solve_problem_json(&request_json)?;
```

---

## 🎯 Key Capabilities

- **Deterministic Constraint Engine**: MRV heuristic and backtracking search for hard constraints, paired with local search optimization for weighted soft goals.
- **Multi-Candidate Generation**: Generates distinct, hard-valid candidate plans with diversity and stability metrics.
- **Independent Plan Validation**: Every produced plan is verified against all constraints by an independent checker.
- **Frozen Solver Statuses**: Strictly reports `Solved`, `ProvenInfeasible`, `Timeout`, `Unknown`, `InvalidInput`, `Cancelled`, or `InternalError`.

---

## 📄 License

Licensed under [Apache-2.0](../../LICENSE).
