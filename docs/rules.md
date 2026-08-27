# Rules

[English](rules.md) / [简体中文](rules.zh.md)

SeatTrellis v2.0.0 separates JSON rules into **hard constraints** and **soft
objectives**. Hard constraints are absolute; soft objectives are weighted
preferences.

## Hard constraints

Hard rules must be satisfied. If they cannot be satisfied, solving does not
produce a valid plan.

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
| `fixed_seats` | Fix a student to one enabled seat |
| `must_be_adjacent` | Require two students to be adjacent |
| `cannot_be_adjacent` | Prevent two students from being adjacent |
| `min_distance` | Require a minimum distance between two students |

In project and workbench rules, `student` may refer to a stable student key or
name and `seat_id` must identify an enabled seat. The loader resolves these
references to the index-pair form consumed by the native solver.

Validation checks unknown students, unknown or disabled seats, duplicate fixed
assignments, contradictory pair rules, obvious distance/adjacency conflicts,
fixed assignments that already violate a rule, and unknown rule fields.

```bash
seattrellis_cli validate --problem problem.json
seattrellis_cli project-validate \
  --project my-class/seattrellis.project.json \
  --strict
```

If no feasible plan is found, the CLI reports the student count, enabled-seat
count, hard-rule count, and likely causes such as fixed seats, dense forbidden
adjacency, minimum distances, or disabled seats.

## Soft objectives

Soft rules are preferences, not guarantees. Each has `enabled` and a non-negative
integer `weight` from 0 through 1,000,000. Negative or oversized weights and
unknown objective names fail validation.

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

| Objective | Description |
| --- | --- |
| `vision_front` | Prefer front seats for students with vision needs |
| `height_back` | Prefer back seats for taller students |
| `randomize` | Add reproducible seed-based variation |
| `score_balance` | Prefer score-level mixing across adjacent seats |
| `score_position` | Prefer high scores at the front or back |
| `score_distribution` | Reduce score differences across rows or groups |
| `mentor_pairing` | Pair higher- and lower-score students under a relation |
| `fair_rotation` | Prefer rotating seat categories using history |
| `avoid_recent_neighbors` | Reduce repeated desk-mate and neighbor relationships |
| `cooling` | Penalize relationships seen within a recent history window |

The `seed` fixes the pseudorandom sequence. A completed fixed-budget run is
stable for the same inputs and seed. A wall-clock timeout may stop different
machines after different numbers of attempts, so timed-out results are not
promised to match. Snapshots record whether a time limit stopped the run.

## Scenario presets

Presets are standard `RuleSet` configurations, not separate solvers or a new
file format. The workbench can start from a preset and recursively apply an
explicit rules overlay. The standalone CLI receives the final `RuleSet` and
uses `validate --preset <name>` only for missing-data warnings.

| Preset | Focus |
| --- | --- |
| `random` | Reproducible random variation |
| `exam` | Stronger variation; spacing and fixed seats remain explicit hard rules |
| `daily` | Vision, height, score mixing, fair rotation, and neighbor avoidance |
| `fair-rotation` | Historical seat-category rotation |
| `neighbor-aware` | Fewer repeated desk-mate and neighbor relationships |
| `balanced` / `peer-mixing` | Score-level mixing |
| `score-high-front` / `score-high-back` | Score placement by row direction |
| `row-score-balanced` / `group-score-balanced` | Score distribution across rows or groups |
| `mentor-pairing` | Higher/lower score pairing |
| `height-aware` | Taller students toward the back |
| `vision-friendly` | Front seats for vision or front-seat needs |

```bash
seattrellis_cli validate \
  --problem problem.json \
  --preset daily \
  --history-dir examples/history
```

Missing history, score, height, or vision/front-seat markers produces a warning
and disables only the unsupported preference. Its score dimension becomes
`not_available`. `validate --strict` treats the warning as a failure. Presets
never invent or relax hard constraints.

## Fair rotation

`fair_rotation` is a soft rule. It penalizes recent repetition of selected seat
categories and gives a small compensation to students with fewer long-term
category counts. It does not guarantee absolute fairness and never overrides a
hard rule. Without history it is inactive and solving still succeeds.

| Field | Description |
| --- | --- |
| `enabled` | Enable fair rotation |
| `weight` | Non-negative weight; larger values penalize repetition more strongly |
| `avoid_repeating_categories` | Categories to avoid repeating |
| `lookback` | Number of recent snapshots used for repeat penalties; `0` disables them |

Supported categories are `front`, `back`, `middle`, `side`, `corner`,
`near_window`, `near_door`, `near_platform`, and `near_ac`. Position inference is
defined in [Input formats](input-format.md).

## Recent-neighbor avoidance

`avoid_recent_neighbors` adds cost when the next plan repeats relationships seen
in recent snapshots. It cannot override `must_be_adjacent`, `cannot_be_adjacent`,
fixed seats, or minimum distance.

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

| Field | Description |
| --- | --- |
| `enabled` | Enable recent desk-mate/neighbor avoidance |
| `weight` | Non-negative weight; larger values penalize repetition more strongly |
| `lookback` | Recent snapshots used; `0` disables recent-pair penalties |
| `relation_types` | Relationships to avoid |
| `max_recent_count` | Start penalizing after this many recent occurrences; `1` means from the second occurrence |
| `within_distance` | Chebyshev threshold for `within_distance`; default `2` |

Relationship types are:

| Type | Definition |
| --- | --- |
| `desk_mate` | Same-row horizontal neighbor by default; reserved for custom desk groups |
| `horizontal` | Same row and column delta 1 |
| `vertical` | Same column and row delta 1 |
| `diagonal` | Row and column deltas both 1 |
| `adjacent_any` | Horizontal, vertical, diagonal, graph, or custom-edge adjacency |
| `within_distance` | Row/column Chebyshev distance at or below the threshold |

Irregular layouts are not filled into a complete matrix. Missing students or
unknown historical seats are skipped with warnings; disabled historical seats
remain unavailable for new solves but can still contribute relationships when
coordinates are available.

## Cooling

`cooling` is the strict recent-neighbor objective. It penalizes a selected
relationship if it appeared in any of the previous `cooling_period` snapshots,
equivalent to `avoid_recent_neighbors` with `max_recent_count: 0`.

```json
{
  "soft": {
    "cooling": {
      "enabled": true,
      "weight": 12,
      "cooling_period": 3,
      "relation_types": ["desk_mate", "adjacent_any"],
      "within_distance": 2
    }
  }
}
```

When both objectives are enabled, the solver combines their history windows and
relation sets and adds their weights. Solving, candidate scoring, and fairness
summaries use the same pair-history cost.

## Candidates and scoring

`candidates --count N` applies the normal hard constraints and soft costs, then
continues with different seeds and exclusions for previously generated complete
assignments. Candidate generation and recommendation are heuristic; they do not
enumerate every feasible plan or prove a global optimum.

Scores use a 0-100 scale. The total includes only dimensions with
`status: "available"` and applies the corresponding soft-rule weight.
`not_available` means the inputs cannot support a dimension; it is not a zero.
Plans that fail hard-constraint verification never enter a candidate set. See
[Candidates](candidates.md) and [Scoring](scoring.md).
