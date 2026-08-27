# Desk-Mate and Neighbor History

**SeatTrellis v2.0.0 is released.**

`pair-report` counts how often two students were desk mates, horizontally
adjacent, vertically adjacent, diagonally adjacent, adjacent by any current
graph relation, or within a configured distance in historical snapshots.

```bash
seattrellis_cli pair-report \
  --problem problem.json \
  --history-dir examples/history \
  --top 10
```

The `avoid_recent_neighbors` soft objective uses these counts to reduce recent
repetition. `cooling` is stricter: it penalizes a relationship seen in any of a
configured number of recent snapshots. Both objectives use the same pair-history
calculation and never relax fixed seats, adjacency, or minimum-distance hard
constraints.

The `within_distance` relation uses row/column Chebyshev distance and defaults
to a threshold of `2`. Irregular layouts are evaluated as their actual seat
nodes; the solver does not fill missing cells into a rectangular matrix.

Unknown historical students and seats produce warnings. A disabled historical
seat remains unavailable for a new solve, but its relationship can be counted
from coordinates when possible.
