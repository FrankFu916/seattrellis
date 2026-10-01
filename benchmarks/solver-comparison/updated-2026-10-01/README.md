# Optimized Rust / OR-Tools 9.15 comparison — 2026-10-01

Rust core: `d8a799047d2708c9a16d7762ccb6f2b4cedcb76a`, rebuilt with Rust 1.88 release, opt-level 3, thin LTO and one codegen unit. CP-SAT: OR-Tools 9.15.6755 with protobuf 6.33.5. This is a new measurement; earlier results and package metadata remain unchanged.

The same nine frozen inputs are referenced from `../corpus/`; every input SHA-256 matches the earlier experiment. Each case has three trials (27 total), alternating engine order, seed 42, one worker and a shared two-second model/preparation/search budget. All ten soft objectives are disabled.

All 36 feasible assignments passed the independent Python validator and current release Rust CLI audit. Three triangle-free graph proofs establish infeasibility independently. The invalid fixed-seat control was rejected by both validators (CLI exit 2). See `verification.json` and `validation-controls.json`.

Medians in milliseconds. Rust core includes internal preparation/search; its full subprocess additionally includes reading, decoding, startup and output. CP warm includes decoding, model construction, native search and extraction after Python/OR-Tools import.

| Case | Rust core | Rust subprocess | CP native search | CP warm model + search + adapter |
| --- | ---: | ---: | ---: | ---: |
| 40-easy | 51.3 | 53.1 | 10.7 | 22.7 |
| 40-mixed | 64.4 | 66.1 | 63.7 | 76.0 |
| 40-infeasible | 25.1 | 26.5 | 8.8 | 17.6 |
| 60-easy | 167.7 | 170.4 | 19.3 | 37.9 |
| 60-mixed | 196.3 | 198.8 | 161.8 | 183.9 |
| 60-infeasible | 86.6 | 88.1 | 20.0 | 61.5 |
| 80-easy | 375.2 | 377.2 | 41.5 | 71.9 |
| 80-mixed | 403.3 | 405.0 | 293.1 | 330.3 |
| 80-infeasible | 215.9 | 220.7 | 32.5 | 65.6 |

CP warm is faster in eight of nine cases; Rust wins the 40-mixed case including its subprocess overhead. The 60-mixed difference is small. These three-trial synthetic measurements do not establish a universal backend ranking, a latency guarantee, equivalent soft-objective quality, or a Rust/C++ language speed comparison. CP-SAT OPTIMAL here certifies satisfaction, not classroom-objective optimality.

A separate single cold CP process for 60-easy took 529.5ms, including 339.8ms of imports. It is not a median and is excluded from the warm table; a native C++ adapter would need separate measurement.

`results.json` retains raw samples, summaries, hardware/tool versions, source commit and source/script/requirements/binary hashes. `results/` contains 36 response/audit pairs. `controls/` contains the invalid response. `provenance/` stores the exact measured script, requirement pins and harness source snapshots matching their hashes; the harness manifest retains the original measurement path. For reproduction, copy these snapshots into a fresh isolated directory, adjust the manifest core path, and set `SEATTRELLIS_REPO`, `SEATTRELLIS_CLI` and `SEATTRELLIS_RUST_HARNESS` before running.
