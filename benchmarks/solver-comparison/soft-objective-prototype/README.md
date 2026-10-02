# Bounded CP-SAT soft-objective parity prototype

This isolated experiment addresses part of the previous hard-only comparison's
main gap. It implements five of SeatTrellis' ten goals, with an explicit domain
restriction for score distribution. It does **not** register a production
backend, introduce a Python application dependency, or replace Rust's validator.
The solver engine is OR-Tools' native C++; Python is this experiment's adapter.
The original hard-only corpus, results and updated October 1 evidence are unchanged.

## Reproduce

From the repository root, with Rust 1.88 and Python 3.12 or newer:

```sh
python3 -m venv /tmp/seattrellis-soft-objective-venv
/tmp/seattrellis-soft-objective-venv/bin/python -m pip install \
  -r benchmarks/solver-comparison/soft-objective-prototype/requirements.lock.txt
cargo build --release --locked \
  --manifest-path benchmarks/solver-comparison/soft-objective-prototype/rust-oracle/Cargo.toml \
  --target-dir /tmp/seattrellis-soft-objective-target
/tmp/seattrellis-soft-objective-venv/bin/python \
  benchmarks/solver-comparison/soft-objective-prototype/soft_compare.py \
  --oracle /tmp/seattrellis-soft-objective-target/release/seattrellis-soft-objective-oracle \
  --output /tmp/seattrellis-soft-objective-results.json
```

This runs generated deterministic synthetic cases and negative controls. It
checks every permutation against the production Rust core's cost and objective
functions and its independent hard-rule evaluator. Each CP-SAT incumbent passes
those same independent checks; its objective must equal the exhaustive minimum.
Rust solver results and a solve with the Rust incumbent's entire assignment
fixed also have to agree with the oracle. Separately, each feasible case probes
the actual CP model at its lowest, middle and highest-cost legal assignments;
its native `ObjectiveValue` must equal the independently scored scaled integer
cost. A nonzero Rust gap would be reported honestly; Rust is not
required to prove a soft optimum. `--write-cases` regenerates inspectable case
documents in this experiment's `cases/` directory. Timing includes validation,
objective compilation, model construction, native search and extraction within
one nominal two-second operation budget; imports and exhaustive checks are excluded.
Preparation time is subtracted from the native search allowance. Extraction is
included in the reported total, but a strict end-to-end deadline including that
phase still needs a production implementation. If preparation consumes the
budget, no search starts. Timing is diagnostic, not
a performance guarantee or a comparison against historical classroom benchmarks.

The separate Rust workspace only depends on the existing Rust core and JSON
serialization. It does not alter the root workspace or production dependencies.

## Covered semantics and boundaries

| Goal | Model and supported domain |
| --- | --- |
| `vision_front` | Exact integer cost `weight × (row − min_row) × 100`, with the core's numeric vision priority and known vision/tag/need keywords. Numeric recognition follows Rust's ASCII `f64::parse` grammar; Python's extra whitespace, underscore and Unicode-digit acceptance is excluded. |
| `height_back` | Exact integer cost `weight × ties_to_even(height) × (max_row − row)`. Missing heights contribute zero. Saturating or overflowing values are rejected. |
| `score_position` | Average ranks preserve score ties; missing scores do not enter the mean. Distinct-row percentiles, `high_front` and `high_back`, and one-row `0.5` semantics are represented as exact rational assignment coefficients. Constant scores make the goal unavailable, contributing zero exactly as the core does. Unused seats are allowed. |
| `fair_rotation` | Exact per-student/seat coefficients from category history: recent repetition, long-term imbalance and **negative compensation bonuses**. Indexed histories use global periods, including gaps; legacy histories use occurrence windows. Zone/category inference and custom selected categories are included. This does not implement multi-period rotation generation. |
| `score_distribution` | **Exactly two equal-capacity populated rows or groups, every seat occupied, every student scored, and at least two distinct scores.** In this domain, `2 × RMS(bucket means)` equals `abs(mean₁ − mean₂)`, so an exact absolute integer sum implements the original loss. Groups may cross rows. |

The last restriction matters: squaring the distribution loss preserves its
ordering only when considered alone, and can change tradeoffs with other goals.
This experiment does not substitute a squared or linear approximation for
general RMS. Three or more buckets, unequal capacities, unoccupied seats, missing
scores, or incomplete group identifiers are rejected when distribution is active.

Enabled `randomize`, `score_balance`, `mentor_pairing`, `avoid_recent_neighbors`
and `cooling` are rejected, including enabled rules with weight zero, so supported
scope remains explicit. All ten goal fields must specify `enabled`: omitted
fields can re-enable production defaults and are rejected. Pair history is not
supported. This is a harness for the synthetic cases, not a general-purpose
public input adapter or a replacement for production validation.

Euclidean hard-distance rules are limited to integer thresholds at most
3,000,000 and integer coordinates within ±1,000,000; this experiment compares
squared distances as exact integers. Python `math.dist` and Rust `hypot` can
differ by one ULP on fractional inputs, making a boundary assignment legal in
one engine and illegal in another. Fractional inputs are explicitly rejected,
including a recorded concrete counterexample. A production adapter must share
the core's distance legality semantics before extending this domain. Graph
distances remain integer path lengths. Score and height fields accept JSON
numbers/null only, enforce the production ranges, and reject booleans, numeric
strings and nonfinite values. Omitted scores and explicit null scores both
retain the core's unavailable-score semantics. Vision remains a string/null
DTO: textual `NaN`/infinity follow Rust's parsing priority, while a numeric JSON
vision value is rejected just as the Rust DTO rejects it.

## Scaling and error

Score ranks and row ranks are exact fractions. A least common multiple of all
coefficient denominators scales the **combined** objective to integers; goals do
not receive unrelated scales that change their relative weights. The exact
integer model introduces **zero rounding error** within the documented domain.
Binary64 evaluation in Rust introduces ordinary floating-point differences;
the experiment compares costs with absolute tolerance `1e-9`, recording the
maximum observed difference rather than claiming bitwise equality. Extremely
close floating-point ties outside these tested cases are not certified.

All scaled coefficients and the conservative sum of their absolute expression
domains must fit below `2^60`, leaving headroom below CP-SAT's signed-int64 limit.
Models also pass OR-Tools' own validator. Negative/zero scaling, excessive scales,
negative/out-of-range weights (including the production enabled limit 1,000,000),
saturation and overflow are rejected. Negative
**costs** caused by fair-rotation bonuses remain valid and are tested.

`OPTIMAL` means a proven optimum of **this restricted five-goal model**, never
an optimum of all ten SeatTrellis goals. `results.json` records the exact objective
scope, environment, source/data/binary hashes, raw timing, exhaustive minima,
Rust results and rejection reasons. No sampled timing establishes a speed claim.

## Recorded validation

The committed run checks **17 cases and all 4,224 permutations**, with **29
rejection controls** and a separate preparation-budget exhaustion control. The
largest discrepancy against Rust's binary64 cost is approximately `2.27e-13`.
All sixteen feasible cases return the exhaustive optimum for both engines in
this small corpus; the impossible adjacency triangle is independently proven
infeasible. This is not evidence that Rust generally finds a global soft optimum.
The actual CP model's objectives also match **48 fixed-assignment probes**,
including nonoptimal assignments. A second replay reproduced the
objective/feasibility results and CP assignments.
The oracle passes Rust formatting and Clippy with warnings denied.

## Before a native C++ optional backend can ship

1. Specify equivalents for the remaining five goals, including deterministic
   randomization, pair-history windows and merged cooling/repetition semantics.
2. Implement the general distribution RMS without silently changing mixed-goal
   tradeoffs; define any approximation and its aggregate error contract, or
   explicitly route unsupported requests to the existing Rust backend.
3. Preserve `Solved`, `ProvenInfeasible`, `Timeout`, `Unknown`, `InvalidInput`,
   `Cancelled`, and `InternalError`, including a valid incumbent at deadline.
   OR-Tools status names alone do not provide this application contract.
4. Include preprocessing in operation budgets and support cancellation across
   preprocessing, native search, extraction and validation. Validate every
   incumbent with Rust before exposing it to the application. Share exact
   numeric parsing and hard-distance boundary semantics, rather than assuming
   that language/library floating-point helpers agree.
5. Verify locked editing, repair, candidate diversity, multi-period rotation,
   history updates and export behavior against production expectations.
6. Build a versioned, memory-owned C ABI/C++ adapter with reproducible native
   binaries, licensing review, platform packaging and realistic parity/load
   checks. Mobile and WebAssembly support require separate evidence.

Only after these requirements pass should the product expose a backend option.
The current production Rust backend remains the default.
