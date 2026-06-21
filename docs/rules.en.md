# Rules

Rule files are JSON and separate `hard` constraints from `soft` preferences.

## hard Rules

Hard rules must be satisfied. If they cannot be satisfied, solving fails.

```json
{
  "seed": 42,
  "hard": {
    "fixed_seats": [{"student": "STU001", "seat_id": "R1C1"}],
    "must_be_adjacent": [{"students": ["STU002", "STU003"]}],
    "cannot_be_adjacent": [{"students": ["STU004", "STU005"]}],
    "min_distance": [{"students": ["STU006", "STU007"], "distance": 2, "metric": "euclidean"}]
  }
}
```

| Rule | Description |
| --- | --- |
| `fixed_seats` | Fix one student to one enabled seat |
| `must_be_adjacent` | Require two students to be adjacent |
| `cannot_be_adjacent` | Prevent two students from being adjacent |
| `min_distance` | Require at least a given distance between two students |

`student` may refer to `student_id` or `name`. `seat_id` must exist and must be enabled.

Current validation checks:

- rules referencing unknown students;
- rules referencing unknown or disabled seats;
- one student fixed to multiple seats;
- one seat fixed to multiple students;
- the same pair appearing in both `must_be_adjacent` and `cannot_be_adjacent`;
- obvious `min_distance` and `must_be_adjacent` conflicts for the same pair;
- fixed seats that already violate adjacency or distance rules;
- unknown rule fields as errors.

Preflight command:

```bash
seattrellis validate --students examples/students.csv --layout examples/classroom.json --rules examples/rules.json
```

If the solver cannot find a feasible plan, the CLI prints the student count, enabled-seat count, hard-rule count, and possible causes such as fixed seats, dense cannot-adjacent rules, minimum distances, or disabled seats.

## soft Rules

Soft rules are preferences. They are not guaranteed. Each rule has `enabled` and a non-negative integer `weight`. Negative weights fail validation, and unknown soft-rule names are reported as errors.

```json
{
  "soft": {
    "vision_front": {"enabled": true, "weight": 20},
    "height_back": {"enabled": true, "weight": 1},
    "randomize": {"enabled": true, "weight": 1},
    "score_balance": {"enabled": false, "weight": 1}
  }
}
```

| Rule | Description |
| --- | --- |
| `vision_front` | Prefer front seats for students with vision needs |
| `height_back` | Prefer back seats for taller students |
| `randomize` | Add reproducible seed-based variation |
| `score_balance` | Prefer score-gap patterns among adjacent students |

`seed` controls reproducibility. The same inputs and seed should produce stable output.
