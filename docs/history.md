# Fair-Rotation History

**SeatTrellis v2.0.0 is released.**

`history-report` reads historical seating snapshots and counts each student's
front, back, middle, side, corner, near-window, near-door, near-platform, and
near-AC categories.

```bash
seattrellis_cli history-report \
  --problem problem.json \
  --history-dir examples/history \
  --output outputs/history-report.json
```

The `fair_rotation` soft objective uses recent snapshots to reduce repeated seat
categories. Missing history does not make a solve fail; it makes the affected
dimension `not_available` and records the reason in the report.

Snapshots are interpreted using the current roster's stable student keys and the
current layout's seat IDs, rows, columns, zones, and landmark flags. Unknown
students or seats produce warnings rather than silently changing the current
layout.

Keep real historical records de-identified and outside the repository. The
history under `examples/` is fictional.
