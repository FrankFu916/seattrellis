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
    "score_balance": {"enabled": false, "weight": 1},
    "fair_rotation": {
      "enabled": true,
      "weight": 10,
      "avoid_repeating_categories": ["front", "back", "side", "corner", "near_window", "near_door", "near_ac"],
      "lookback": 4
    },
    "avoid_recent_neighbors": {
      "enabled": true,
      "weight": 10,
      "lookback": 4,
      "relation_types": ["desk_mate", "adjacent_any"],
      "max_recent_count": 1,
      "within_distance": 2
    }
  }
}
```

| Rule | Description |
| --- | --- |
| `vision_front` | Prefer front seats for students with vision needs |
| `height_back` | Prefer back seats for taller students |
| `randomize` | Add reproducible seed-based variation |
| `score_balance` | Prefer score-gap patterns among adjacent students |
| `fair_rotation` | Prefer rotating seat categories based on historical snapshots |
| `avoid_recent_neighbors` | Prefer reducing repeated desk-mate and neighbor relationships from history |

`seed` controls reproducibility. The same inputs and seed should produce stable output.

## fair_rotation

`fair_rotation` is a soft rule. It never overrides hard rules: fixed seats, must-adjacent, cannot-adjacent, and minimum-distance constraints still take priority. If no historical snapshots are supplied, `fair_rotation` becomes inactive and solving still succeeds with a metrics message. `weight=0` has no solving effect.

Fields:

| Field | Description |
| --- | --- |
| `enabled` | Enable fair rotation |
| `weight` | Non-negative integer weight; larger values avoid repeated categories more strongly |
| `avoid_repeating_categories` | Seat categories to avoid repeating |
| `lookback` | Number of recent snapshots used for repeat penalties; `0` disables recent-repeat penalties |

Supported categories are `front`, `back`, `middle`, `side`, `corner`, `near_window`, `near_door`, `near_platform`, and `near_ac`. The default repeated categories are `front`, `back`, `side`, `corner`, `near_window`, `near_door`, and `near_ac`.

Both the fallback solver and the OR-Tools solver translate fair rotation into a per student-seat heuristic cost: recent repeated categories add cost, and students with fewer long-term counts receive a small compensation. The method is category-count based and does not guarantee absolute fairness.

## avoid_recent_neighbors

`avoid_recent_neighbors` is a soft rule. It analyzes historical snapshots for repeated pair relationships, then adds cost when the next seating plan would repeat recent desk-mate or neighbor relationships. It never overrides hard rules: if `must_be_adjacent` requires two students to be adjacent, or fixed seats make them adjacent, the hard rule still wins; if `cannot_be_adjacent` or `min_distance` forbids a relationship, history avoidance cannot make it valid. If no historical snapshots are supplied, the rule becomes inactive and solving still succeeds with a metrics message. `weight=0` has no solving effect.

Example:

```json
{
  "soft": {
    "avoid_recent_neighbors": {
      "enabled": true,
      "weight": 10,
      "lookback": 4,
      "relation_types": ["desk_mate", "adjacent_any"],
      "max_recent_count": 1,
      "within_distance": 2
    }
  }
}
```

Fields:

| Field | Description |
| --- | --- |
| `enabled` | Enable recent desk-mate / neighbor avoidance |
| `weight` | Non-negative integer weight; larger values avoid repeated relationships more strongly |
| `lookback` | Number of recent snapshots used for pair-history penalties; `0` disables recent-pair penalties |
| `relation_types` | Relationship types to avoid |
| `max_recent_count` | Start penalizing only after this many recent occurrences; `1` means the second recent occurrence and beyond adds cost |
| `within_distance` | Chebyshev distance threshold for the `within_distance` relation, default `2` |

Relationship types:

| Type | Definition |
| --- | --- |
| `desk_mate` | Defaults to horizontal neighbors in the same row with a column delta of 1; reserved for future custom desk groups |
| `horizontal` | Same row and column delta of 1 |
| `vertical` | Same column and row delta of 1 |
| `diagonal` | Row delta of 1 and column delta of 1 |
| `adjacent_any` | Horizontal, vertical, diagonal, or adjacent through the current layout adjacency graph / custom edges |
| `within_distance` | Row/column Chebyshev distance less than or equal to `within_distance` |

Pair history is interpreted against the current student list and current layout; SeatTrellis does not fill irregular layouts into a complete matrix. Missing current students are skipped with a warning. Unknown historical seats are skipped for affected pairs with a warning. If a historical snapshot references an `enabled=false` seat, the seat is still unavailable for new solving, but historical relationships are counted from row/column coordinates when possible and a warning is recorded.

Both the fallback solver and the OR-Tools solver support this rule. The current implementation is heuristic scoring: it tends to reduce repeated desk-mate and neighbor relationships, but it does not guarantee global optimality.

## Multiple Candidates And Scoring

Multi-candidate mode does not add or relax rules. `solve --candidates N` applies the normal hard constraints and soft costs, then continues solving with different seeds and a constraint that excludes each previously generated complete assignment. Hard constraints remain absolute. Candidate generation and recommendation are heuristic and do not guarantee enumeration of every feasible plan or a global optimum.

```bash
seattrellis solve \
  --students examples/students.csv \
  --layout examples/classroom.json \
  --rules examples/rules_multi_candidate.json \
  --history-dir examples/history \
  --candidates 5 \
  --seed 42 \
  --output outputs/candidates.json \
  --report outputs/plan-report.json
```

Available dimensions use a 0–100 scale, where higher is better for that dimension. The total includes only dimensions with `status: "available"` and weights them with the corresponding soft-rule weight. `diversity_score` uses `randomize.weight`; `stability_score` uses a fixed comparison weight of 1. A plan that fails hard-constraint verification is never included in a candidate set.

| Dimension | Meaning | Example unavailable condition |
| --- | --- | --- |
| `fair_rotation_score` | Historical seat-category rotation cost; lower cost produces a higher score | Rule disabled or no history |
| `avoid_recent_neighbors_score` | Recent repeated desk-mate / neighbor cost; lower cost produces a higher score | Rule disabled or no pair history |
| `score_balance_score` | Mixing of different score levels across adjacent seats | Rule disabled or insufficient scores |
| `height_preference_score` | Match between height ordering and front/back row placement | Rule disabled or insufficient heights/rows |
| `vision_preference_score` | How close students with vision needs are to the front | Rule disabled or no matching students |
| `diversity_score` | Percentage of students seated differently from other candidates | Only one candidate |
| `stability_score` | Percentage of unchanged seats versus the latest historical snapshot | No history |
| `hard_constraint_summary` | Assignment completeness and all hard-rule checks | Always evaluated |

`not_available` means the current inputs cannot honestly support that dimension; it is not a zero score. The recommended candidate is the highest weighted-total hard-valid plan, with `candidate_id` as a deterministic tie-breaker.
