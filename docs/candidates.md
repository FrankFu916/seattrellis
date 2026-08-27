# Candidate Plans

**SeatTrellis v2.0.0 is released.**

`candidates --count N` uses the problem's seed and a deterministic sequence of
attempts to generate up to `N` distinct plans. Every returned candidate must
satisfy all hard constraints and includes its assignment, total score, and score
breakdown.

```bash
seattrellis_cli candidates \
  --problem problem.json \
  --count 5 \
  > outputs/candidates.json
```

## Recommendation

1. Exclude candidates that fail hard-constraint verification.
2. Rank the remaining candidates by weighted total over available score
   dimensions.
3. Use `candidate_id` as the stable tie-breaker.

If the feasible candidate space is too small, the CLI returns the distinct plans
it found and records a warning. It does not duplicate a plan to reach the
requested count.

Candidate generation is heuristic. It does not enumerate every feasible plan or
prove a global optimum. Use `--latest-snapshot` when the stability dimension
should compare candidates with the latest historical plan.

The v2 report uses `api_version: 2`. Legacy candidate-set artifacts remain
readable where the project workflow supports them; `project-export --candidate
<id>` selects a candidate by ID and defaults to the recommended candidate.

See [Scoring](scoring.md) for dimension semantics and [Rules](rules.md) for the
objectives used during generation.
