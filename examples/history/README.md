# Fictional history snapshots

These files are fictional SeatTrellis seating snapshots for local fair-rotation and neighbor-history demos.

They do not represent a real class, school, teacher, student, grade, score record, or seating history. Keep real historical seating records outside the repository, for example under ignored directories such as `private/`, `data/`, or `snapshots/`.

Run:

```bash
seattrellis history-report --students examples/students.csv --layout examples/classroom.json --history-dir examples/history
seattrellis pair-report --students examples/students.csv --layout examples/classroom.json --history-dir examples/history
```

The sample history intentionally repeats a few fictional desk-mate / neighbor relationships so `pair-report` and `avoid_recent_neighbors` have visible behavior. Keep real historical seating records outside the repository, and store them only under ignored local paths.
