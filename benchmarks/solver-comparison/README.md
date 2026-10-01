# Solver comparison experiment

This directory is an isolated engineering experiment. It does not add Python,
OR-Tools, or a new solver dependency to SeatTrellis production packages.

The timings below were recorded with OR-Tools 9.14.6206 and protobuf 6.31.1.
The current reproduction requirements use OR-Tools 9.15.6755 and patched
protobuf 6.33.5. Only correctness and dependency compatibility were checked
with the updated environment; its performance has not been measured here.

On this hard-feasibility corpus, CP-SAT was faster in all nine cases even after
including its warm model construction and adapter work. Both engines solved
all feasible instances and proved all infeasible instances in all three
trials. This supports further evaluation of an optional CP-SAT backend;
full-objective equivalence and native distribution remain untested.

Recorded medians in milliseconds:

| Case | Rust core call | CP native solve | CP model construction | CP warm model + solve + adapter | Result |
| --- | ---: | ---: | ---: | ---: | --- |
| 40 easy | 114.8 | 8.7 | 14.2 | 23.1 | Feasible |
| 40 mixed | 102.8 | 58.5 | 18.8 | 77.2 | Feasible |
| 40 infeasible | 38.1 | 8.3 | 16.5 | 25.0 | Proven infeasible |
| 60 easy | 344.3 | 18.2 | 34.6 | 52.4 | Feasible |
| 60 mixed | 338.9 | 132.0 | 39.5 | 171.7 | Feasible |
| 60 infeasible | 117.0 | 18.1 | 30.7 | 49.1 | Proven infeasible |
| 80 easy | 854.7 | 36.0 | 63.4 | 100.3 | Feasible |
| 80 mixed | 665.3 | 241.8 | 64.0 | 306.1 | Feasible |
| 80 infeasible | 296.3 | 37.1 | 66.2 | 103.6 | Proven infeasible |

All 36 feasible assignments passed both validators. The Rust subprocess adds
about 1.5–3.7 ms on this host, including output and startup. A separate single
cold Python CP-SAT process for 60 easy took 499.1 ms, including 314.7 ms to
import OR-Tools; this cold sample is not a median and is excluded from the warm
table. A production C++ integration would have different startup/adapter costs.

The comparison uses the same frozen raw request files for SeatTrellis Rust core
and an independently implemented OR-Tools CP-SAT model. The corpus contains
40, 60, and 80 students and seats. Each size has an unconstrained instance, a
mixed hard-constraint instance, and a mathematically infeasible instance.
The mixed cases cover fixed seats, required/forbidden adjacency, Euclidean and
graph distance, and groups that must sit together or separately. The infeasible
cases require a three-person adjacent clique on a triangle-free horizontal
seat graph.

All ten SeatTrellis soft objectives are disabled. This experiment evaluates
hard-constraint feasibility and infeasibility proofs. CP-SAT reports `OPTIMAL`
when its satisfaction model is solved; that status does not establish an
optimal classroom arrangement under SeatTrellis soft objectives. No standalone
SAT solver, MaxSAT encoding, or equivalent C++ implementation of SeatTrellis'
algorithm is measured.

Both engines use seed 42, one worker, and a two-second operation budget. The
budget includes Rust preparation/search or CP-SAT model construction/search;
JSON decoding is measured separately. The CP-SAT adapter is Python and calls
the packaged native C++ engine. Rust is built in release mode with Rust 1.88.0,
optimization level 3, thin LTO, and one code generation unit. Comparisons of
these different algorithms and adapters cannot establish a language speed
ranking.

Every returned assignment is independently checked in Python, then submitted
to the existing Rust CLI `audit` command. A negative control violates a fixed
seat and both validators reject it. Known infeasibility is additionally checked
by counting graph triangles, rather than inferred from the engines agreeing.
Each case has three trials, with engine order alternated between trials.
Timing data includes solver time, model/adapter time, full Rust subprocess time,
and a separate cold CP-SAT subprocess example. Results from a shared cloud host
are exploratory and do not establish latency guarantees.

Reproduction from the repository root on Linux with Rust 1.88 or newer and
Python 3.12 (the recorded tool versions are in `results.json`):

```sh
python3.12 -m venv benchmarks/solver-comparison/venv
benchmarks/solver-comparison/venv/bin/python -m pip install \
  -r benchmarks/solver-comparison/requirements.lock.txt
cargo build -p seattrellis --locked
CARGO_TARGET_DIR="$PWD/benchmarks/solver-comparison/rust-target" \
  CARGO_BUILD_JOBS=1 cargo build --locked --release \
  --manifest-path benchmarks/solver-comparison/rust-harness/Cargo.toml
benchmarks/solver-comparison/venv/bin/python \
  benchmarks/solver-comparison/compare.py
```

Use `SEATTRELLIS_REPO`, `SEATTRELLIS_CLI`, or `SEATTRELLIS_RUST_HARNESS`
environment variables to override discovery. The script writes a fresh corpus,
raw samples, and audit/control evidence; rerunning replaces recorded results.

`requirements.lock.txt` pins the current reproduction environment. The earlier
protobuf 6.31.1 pin is affected by
[CVE-2026-0994 / GHSA-7gcm-g887-7qv7](https://pypi.org/pypi/protobuf/6.31.1/json),
an `Any` JSON parsing recursion-limit bypass. OR-Tools 9.14 requires protobuf
`>=6.31.1,<6.32`, which excludes the fixed 6.33.5 release; OR-Tools 9.15 permits
`>=6.33.1,<6.34`. Both pins are therefore updated together.

The updated requirements were installed in a fresh Python 3.12 environment;
`pip check` passed. All nine model cases passed correctness validation:
six feasible outputs passed the independent validator and Rust CLI audit,
and three infeasible outputs matched the independent triangle-free proofs.
The invalid fixed-seat control was rejected by both validators, and patched
protobuf rejected excessive nested `Any` messages at the configured recursion
limit. All nine pinned packages, including NumPy 2.5.3 and pandas 3.0.6, had
no vulnerabilities listed by PyPI at verification time. The separate
`reproduction-validation.json` preserves this environment's `pip freeze`,
metadata, advisory sources, and correctness results.

Historical `results.json`, audit reports, `installed-package-metadata.json`,
and `packaging-footprint.json` are unchanged. `results.json` records input and
compiled binary SHA-256 hashes, current core source hashes, tool versions, CPU/platform,
parameters, raw trials, and medians. `results/` retains CLI audit reports for
every feasible output. The standalone Rust harness intentionally calls the
same public core parsing and solving entry points used by the application.

Official OR-Tools documentation fetches were blocked by the environment's
network proxy (HTTP 403). `official-sources.json` records the attempted URLs
and errors. Installed wheel metadata and native library presence are separately
recorded in `installed-package-metadata.json`; no network verification checks
were bypassed. Desktop or mobile native SDK compilation is outside this
experiment's measured scope.
