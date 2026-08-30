# Multi-Candidate Seating Examples

This directory contains reference examples for multi-candidate seating generation and comparison in [SeatTrellis (席序)](https://github.com/FrankFu916/seattrellis).

---

## 💡 Quick Command

To generate candidate plans and a comparison report using the sample files:

```bash
# 1. Solve and generate 5 diverse candidates
seattrellis solve \
  --problem problem.json \
  --output outputs/candidates.json

# 2. Export the recommended candidate plan as an interactive HTML chart
seattrellis export \
  --problem problem.json \
  --solution outputs/candidates.json \
  --format html \
  --output outputs/recommended.html
```

> 🔒 **Privacy Note**: All names, student IDs, and classroom data in `examples/` are 100% fictional. Real classroom records should always be kept in `.gitignore`-protected directories (such as `outputs/`).
