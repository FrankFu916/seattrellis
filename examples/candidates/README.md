# Multi-candidate example

The repository does not commit generated candidate sets, reports, or seating
exports. Generate them under the ignored `outputs/` directory:

```bash
seattrellis solve \
  --students examples/students.csv \
  --layout examples/classroom.json \
  --rules examples/rules_multi_candidate.json \
  --history-dir examples/history \
  --candidates 5 \
  --output outputs/candidates.json \
  --report outputs/plan-report.json

seattrellis export \
  --snapshot outputs/candidates.json \
  --candidate recommended \
  --format html \
  --output outputs/recommended.html
```

All names and records in `examples/` are fictional.
