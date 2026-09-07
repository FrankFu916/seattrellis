# Scoring

**SeatTrellis v2.0.0 is released.**

SeatTrellis compares candidate plans with explainable dimensions on a 0-100
scale. Higher is better for an available dimension.

- fair seat-category rotation;
- recent desk-mate and neighbor avoidance;
- score mixing or score placement;
- height preference;
- vision/front-seat preference;
- candidate diversity;
- stability relative to the latest history snapshot.

The total is a weighted average of dimensions whose status is
`available`. Disabled objectives, missing history, and missing student fields
produce `not_available`; they are not silently treated as zero scores.

Scoring is a heuristic comparison tool. It does not prove a global optimum and
never overrides hard constraints. A plan that fails hard-rule verification is
not a valid candidate, regardless of its score.

See [Rules](rules.md) for objective fields and [Candidates](candidates.md) for
recommendation behavior.
