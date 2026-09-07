# Scenario Presets

**SeatTrellis v2.0.0 is released.**

SeatTrellis v2.0.0 treats a preset as a standard `RuleSet` base configuration,
not as a separate solver. In the workbench, a goal starts from a preset and can
receive an explicit rules overlay. In the standalone CLI,
`validate --preset <name>` checks for missing preferred data; it does not merge
rules into the problem.

| Name | Focus | Preferred data |
| --- | --- | --- |
| `random` | Fast reproducible shuffle | None |
| `exam` | Stronger reproducible variation | None |
| `daily` | Combined everyday seating goals | History, score, height, vision |
| `fair-rotation` | Rotate seat categories over time | History |
| `neighbor-aware` | Reduce repeated desk-mate/neighbor pairs | History |
| `balanced` / `peer-mixing` | Mix score levels | Score |
| `score-high-front` | Prefer higher scores toward the front | Score |
| `score-high-back` | Prefer higher scores toward the back | Score |
| `row-score-balanced` | Balance scores across rows | Score |
| `group-score-balanced` | Balance scores across seat groups | Score |
| `mentor-pairing` | Pair higher and lower score percentiles | Score |
| `height-aware` | Prefer taller students toward the back | Height |
| `vision-friendly` | Prefer front seats for vision/front-seat needs | Vision or needs markers |

Missing preferred data disables only the affected soft preference and produces a
warning. Hard constraints are never relaxed automatically.

```bash
seattrellis validate \
  --problem problem.json \
  --preset daily \
  --history-dir examples/history
```

Use `--strict` when those warnings should fail validation. See [Rules](rules.md)
for the fields and overlay behavior.
