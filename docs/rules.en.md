# Rule Handbook

[English](rules.md) · [简体中文](rules.zh.md)

In **SeatTrellis**, classroom seating rules are partitioned into two distinct tiers:
- **Hard Constraints**: Mandatory invariants that must be unconditionally satisfied. If any hard constraint cannot be satisfied, the problem is deemed infeasible and diagnostic reasons are provided.
- **Soft Preferences**: Weighted educational objectives. The solver optimizes these weighted goals within the feasible solution space established by hard constraints.

---

## 🧩 1. Hard Constraints

Hard constraints define strict requirements for classroom assignments, serving as the basis for branch pruning and feasibility proofs during solving.

```json
{
  "seed": 42,
  "hard": {
    "fixed_seats": [
      { "student": "STU001", "seat_id": "R1C1" }
    ],
    "must_be_adjacent": [
      { "students": ["STU002", "STU003"] }
    ],
    "cannot_be_adjacent": [
      { "students": ["STU004", "STU005"] }
    ],
    "min_distance": [
      { "students": ["STU006", "STU007"], "distance": 2, "metric": "euclidean" }
    ]
  }
}
```

### Hard Constraint Types

| Rule Field | Description | Common Use Cases |
| :--- | :--- | :--- |
| `fixed_seats` | **Fixed Seat**: Bind a specific student to a designated enabled seat. | Class leaders, students with specific physical accommodations. |
| `must_be_adjacent` | **Required Adjacency**: Force two students to be neighbors (horizontal, vertical, or connected). | Peer-study partners, lab pairs, collaborative learning groups. |
| `cannot_be_adjacent` | **Forbidden Adjacency**: Strictly forbid two students from sitting adjacent to each other. | Preventing classroom distractions, separating disruptive pairs. |
| `min_distance` | **Minimum Distance**: Require a Euclidean or Chebyshev distance threshold between two students. | Standardized testing layouts, distributing active students. |

> 📌 **Identifier Specifications**:
> - `student` may reference either a `student_id` or `name`.
> - `seat_id` must exist in the classroom layout and must be enabled (`enabled: true`).

### Automated Validation & Conflict Detection

Running `validate` inspects problem files prior to execution:

```bash
seattrellis validate --problem problem.json
```

The preflight validator automatically catches and reports:
1. References to non-existent students or disabled seats.
2. The same student fixed to multiple distinct seats.
3. Multiple students assigned to the same fixed seat.
4. The same student pair appearing in both `must_be_adjacent` and `cannot_be_adjacent`.
5. Conflicting `min_distance` and `must_be_adjacent` rules for the same pair.
6. Fixed seat assignments that inherently violate adjacency or distance constraints.

---

## 🎯 2. Soft Preferences

Soft preferences guide the solver toward desirable educational outcomes. Each rule includes an `enabled` toggle and an integer `weight` ranging from `0` to `1,000,000`.

```json
{
  "soft": {
    "vision_front": { "enabled": true, "weight": 20 },
    "height_back": { "enabled": true, "weight": 5 },
    "randomize": { "enabled": true, "weight": 1 },
    "score_balance": { "enabled": false, "weight": 10 },
    "fair_rotation": {
      "enabled": true,
      "weight": 15,
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

### Soft Rule Overview

| Rule Key | Core Objective | Required Input Data |
| :--- | :--- | :--- |
| `vision_front` | **Vision Accommodation**: Prioritizes front rows for students with vision needs. | Student attributes `needs_front` / `vision_score` |
| `height_back` | **Height Ordering**: Places taller students toward the rear to maintain clear board sightlines. | Student attribute `height` |
| `randomize` | **Reproducible Variation**: Injects deterministic pseudo-random variation based on the seed. | Problem `seed` |
| `score_balance` | **Academic Diversity**: Manages academic score distribution among adjacent peers. | Student attribute `score` |
| `fair_rotation` | **Historical Rotation**: Prevents students from repeatedly sitting in the same room category. | Historical snapshots (`*.snapshot.json`) |
| `avoid_recent_neighbors` | **Neighbor Avoidance**: Penalizes recurring desk-mate and neighbor pairings from recent terms. | Historical snapshots |
| `cooling` | **Strict Cooling Period**: Prohibits repeating specific pairings within `cooling_period` terms. | Historical snapshots |

---

## 🔄 3. Historical Rotation & Neighbor Avoidance

### 3.1 `fair_rotation` (Room Zone Equity)

To ensure fair exposure to classroom areas across semesters, `fair_rotation` tracks zone history per student:

- **Tracked Categories**:
  - `front` (Front rows)
  - `back` (Rear rows)
  - `side` (Wall/side aisles)
  - `corner` (Room corners)
  - `near_window` (Adjacent to windows)
  - `near_door` (Adjacent to doors)
  - `near_ac` (Directly under air conditioning units)
  - `near_platform` (Adjacent to the teacher's desk)

> 💡 **Configuration**:
> - `lookback`: Number of recent historical snapshots analyzed (e.g., `4`).
> - If no historical snapshots are provided, the rule gracefully reports `not_available` without failing the solve.

---

### 3.2 `avoid_recent_neighbors` & `cooling` (Partner Variation)

To broaden social interaction and prevent cliquing, multiple relationship tiers can be avoided:

| Relationship Type (`relation_types`) | Geometric Definition |
| :--- | :--- |
| `desk_mate` | **Standard Desk-mate**: Same row with a column delta of 1. |
| `horizontal` | **Horizontal**: Same row, adjacent column. |
| `vertical` | **Vertical**: Same column, adjacent row. |
| `diagonal` | **Diagonal**: Offset by 1 row and 1 column. |
| `adjacent_any` | **Any Adjacency**: Combines horizontal, vertical, diagonal, and layout graph edges. |
| `within_distance` | **Chebyshev Range**: `distance <= within_distance`. |

---

## 📋 4. Built-in Scenario Presets

SeatTrellis includes 14 standard presets covering typical school workflows:

```bash
seattrellis validate --problem problem.json --preset daily --history-dir examples/history
```

| Preset | Target Scenario | Primary Optimizations |
| :--- | :--- | :--- |
| `daily` | **Daily Teaching** | Balances vision needs, height gradient, score diversity, rotation, and neighbor avoidance. |
| `exam` | **Standardized Testing** | High-entropy randomization paired with minimum spacing constraints. |
| `random` | **Quick Random Shuffle** | Pure reproducible random permutation. |
| `fair-rotation` | **Rotation Focus** | Maximizes room category equity across terms. |
| `neighbor-aware` | **Social Mixing** | Disperses recent desk-mate and neighbor relationships. |
| `balanced` | **Academic Mentorship** | Pairs diverse academic performance levels across neighbors. |
| `height-aware` | **Strict Height Gradient** | Optimizes sightlines for classes with wide height distributions. |
| `vision-friendly` | **Vision Priority** | Ensures optimal front-and-center placement for students with visual impairments. |

---

## 📊 5. Multi-Dimensional Scoring & Radar Metrics

Each plan is scored on a normalized `0` to `100` scale across independent dimensions:

| Score Metric | Description | Quality Indicator |
| :--- | :--- | :--- |
| `vision_preference_score` | Front-row proximity for students with vision needs. | Higher score = better front-row placement. |
| `height_preference_score` | Alignment with ascending front-to-back height order. | Higher score = fewer obstructed sightlines. |
| `fair_rotation_score` | Avoidance of repeated historical room categories. | Higher score = more balanced historical rotation. |
| `avoid_recent_neighbors_score` | Diversity of desk-mate and neighbor pairings. | Higher score = fewer repeated partners. |
| `score_balance_score` | Optimization of academic score differentials. | Higher score = better mixed-level pairings. |
| `diversity_score` | Distinctness compared to other generated candidates. | Higher score = more diverse seating alternatives. |
| `stability_score` | Percentage of seats retained from the prior term. | Higher score = fewer desk movements. |

> ⚠️ **The Meaning of `not_available`**:
> When underlying data is missing (e.g., unrecorded heights or missing history), the corresponding metric is explicitly reported as `not_available` rather than fabricating a zero score, ensuring that weighted totals remain unbiased.
