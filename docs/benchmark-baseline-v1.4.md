# v1.4 performance baseline

> Historical record. This baseline was measured on the v1.4 Python line
> (2026-07-26, `1.4.0rc1`). The v2 Rust line's solver quality and performance
> are governed by the automated gates in [benchmarks.md](benchmarks.md):
> the committed `solver-baseline.json` regression gate, the OR-Tools regret
> gate, and the fixed-corpus oracle differential. This page is kept as the
> original measurement record only.

This baseline was recorded on 2026-07-26 from the `1.4.0rc1` source tree with
Python 3.12 and OR-Tools 9.15 on an Apple Silicon development machine. The
dataset is `synthetic-classroom` / `synthetic-v1`; all records are fictional.
Absolute times are local reference values. The scheduled Linux workflow is the
long-term source for comparisons between commits.

## Fallback matrix

The fallback backend used a 0.25-second limit per solve attempt and at most 24
attempts per case.

| Students | Profile | 1 candidate | 5 candidates | 20 candidates | Mean diversity at 5 / 20 |
|---:|---|---:|---:|---:|---:|
| 40 | light | 1/1 in 0.26s | 5/5 in 1.29s | 20/20 in 5.04s | 90.8% / 90.4% |
| 40 | dense | 1/1 in 0.26s | 5/5 in 1.26s | 20/20 in 5.10s | 87.0% / 86.7% |
| 50 | light | 1/1 in 0.26s | 5/5 in 1.27s | 20/20 in 5.05s | 92.0% / 91.5% |
| 50 | dense | 1/1 in 0.26s | 5/5 in 1.39s | 20/20 in 5.05s | 91.2% / 88.9% |
| 60 | light | 1/1 in 0.26s | 5/5 in 1.27s | 20/20 in 5.06s | 92.7% / 91.8% |
| 60 | dense | 1/1 in 0.26s | 1/5 in 6.01s | 1/20 in 6.01s | n/a / n/a |

Every generated fallback candidate passed hard-constraint verification. The
60-student dense case remained feasible, but the fixed attempt budget found
only one distinct assignment. That candidate-yield limitation is now visible
in the report instead of being hidden behind a successful first solution.

## OR-Tools calibration

A 0.25-second limit is not meaningful for the current OR-Tools model: all
single-candidate cases returned `UNKNOWN`. At two seconds the six 40/50/60
light/dense probes still returned `UNKNOWN`. With a five-second solver limit:

| Students | Light | Dense |
|---:|---|---|
| 40 | feasible in 6.06s end to end | feasible in 5.75s end to end |
| 50 | unknown after 6.69s | unknown after 6.31s |
| 60 | unknown after 7.83s | unknown after 7.51s |

`UNKNOWN` is deliberately not counted as infeasible. The weekly benchmark uses
five seconds for OR-Tools and 0.25 seconds for fallback, with separate jobs for
each size, profile and backend. This keeps the budgets explicit while allowing
every result artifact to finish independently.

## v1.4 decision

- Keep fallback as the default interactive backend. It returns a verified plan
  for every fixed scenario within the short budget.
- Keep OR-Tools as an explicit optional backend for users who can grant a
  longer search window. Improve model construction and objective size before
  considering it as the automatic default for 50–60 students.
- Continue the Rust work as an optional validation, graph and scoring
  experiment. The current native path still delegates search to Python, so it
  cannot claim an end-to-end speed improvement.
- Prioritize candidate generation for the 60-student dense fallback case in a
  future native heuristic spike; hard-rule consistency remains the release
  gate before speed or diversity.

(The Rust line has since completed that experiment: v2 ships a single native
solver with no Python or OR-Tools runtime, evaluated by the automated gates
listed at the top of this page.)

Reproduction commands and report field definitions are in
[`benchmarks.md`](benchmarks.md). Timing changes should be compared on the same
runner and reported as relative changes rather than enforced as fixed seconds.
