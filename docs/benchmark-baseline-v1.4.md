# v1.4 Performance Baseline

> **Historical record only.** This baseline was measured on the v1.4 Python
> line on 2026-07-26 from `1.4.0rc1`, using Python 3.12 and OR-Tools 9.15 on an
> Apple Silicon development machine. The dataset is
> `synthetic-classroom` / `synthetic-v1` and all records are fictional.
>
> SeatTrellis v2.0.0 is released with a single Rust solver. Its quality and
> performance are governed by the automated Rust gates in
> [Benchmarks](benchmarks.md). The former Python oracle comparisons were
> removed after v2.0.0.

Absolute times below are local reference values, not v2 release thresholds.

## Fallback matrix

The v1 fallback backend used a 0.25-second limit per solve attempt and at most
24 attempts per case.

| Students | Profile | 1 candidate | 5 candidates | 20 candidates | Mean diversity at 5 / 20 |
| ---: | --- | ---: | ---: | ---: | ---: |
| 40 | light | 1/1 in 0.26s | 5/5 in 1.29s | 20/20 in 5.04s | 90.8% / 90.4% |
| 40 | dense | 1/1 in 0.26s | 5/5 in 1.26s | 20/20 in 5.10s | 87.0% / 86.7% |
| 50 | light | 1/1 in 0.26s | 5/5 in 1.27s | 20/20 in 5.05s | 92.0% / 91.5% |
| 50 | dense | 1/1 in 0.26s | 5/5 in 1.39s | 20/20 in 5.05s | 91.2% / 88.9% |
| 60 | light | 1/1 in 0.26s | 5/5 in 1.27s | 20/20 in 5.06s | 92.7% / 91.8% |
| 60 | dense | 1/1 in 0.26s | 1/5 in 6.01s | 1/20 in 6.01s | n/a / n/a |

Every generated fallback candidate passed hard-rule verification. The
60-student dense case remained feasible but the fixed attempt budget found only
one distinct assignment; candidate yield was reported rather than hidden.

## OR-Tools calibration

The v1 OR-Tools model returned `UNKNOWN` under the short budget. At two seconds,
all six 40/50/60 light/dense probes still returned `UNKNOWN`. With a five-second
solver limit:

| Students | Light | Dense |
| ---: | --- | --- |
| 40 | feasible in 6.06s end to end | feasible in 5.75s end to end |
| 50 | unknown after 6.69s | unknown after 6.31s |
| 60 | unknown after 7.83s | unknown after 7.51s |

`UNKNOWN` was deliberately not counted as infeasible. This table documents the
old calibration only; the v2.0.0 release does not ship or require the v1 Python
solver stack.

## v1.4 decision record

The v1.4 decision was to keep fallback as the interactive default, retain
OR-Tools as an explicit optional backend, and continue Rust as an experimental
validation and scoring path. That decision is superseded by the completed v2
migration: v2 uses one native Rust solver with no Python or OR-Tools runtime.

Timing comparisons should use the same runner and relative changes. See
[Benchmarks](benchmarks.md) for the current baseline and gates.
