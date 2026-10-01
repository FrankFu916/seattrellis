# Benchmarks

SeatTrellis tracks large-class performance with fixed synthetic data. The
fixture dataset is `synthetic-classroom` / `synthetic-v1`; the CLI performance
generator has its own revision recorded below. All students, seats, and
metrics are fictional.

## Solver regression gate

`benchmarks/solver-baseline.json` records release-mode wall-clock medians for
planted-feasible 40-, 50-, 60-, and 80-student instances. The current CI gate is:

```bash
cargo build --release --locked -p seattrellis
python3 scripts/bench_solver.py --check
```

The Python program only times and checks the Rust CLI. It is not an oracle,
parity comparison, or differential test. A run must stay within 1.10 times the
committed baseline and the absolute interactive bounds:

| Students | Absolute bound |
| ---: | ---: |
| 40 | 1.5 s |
| 50 | 2.5 s |
| 60 | 3.5 s |
| 80 | 6 s |

Each size runs five times and uses the median to reduce false alarms from
transient load on shared CI runners. Refresh the relative baseline only after
several clean, code-equivalent runs show persistent runner drift; never loosen
the absolute bounds to accommodate runner drift.

The median reduces transient noise; neither it nor the 10% margin makes
different hardware equivalent. Compare like-for-like runners. Legacy baseline
machine/compiler metadata was not recorded; new records include input and binary
hashes, compiler, commit and machine information. Updating a baseline is a
reviewed release-maintenance operation, not an ordinary documentation change.

The corpus revision `planted-hard-v1-explicit-soft` explicitly disables every soft objective to preserve the historical effective hard-only workload. Previously `soft={}` relied on a deserialization defect; corrected defaults would change that workload. The timed CLI command keeps response-file output disabled, as in the historical baseline, so storage locking/fsync latency is excluded. After timing, the gate checks both the frozen exit code and the printed `Solved` status. The legacy baseline lacks machine/compiler metadata and remains a regression threshold rather than a controlled comparison.

CI reports the solver performance gate separately from long-run quality gates. Both remain required for release publication. Measured timing tables and provenance are included in the Actions job summary. The [2026-10-01 same-host regression investigation](https://github.com/FrankFu916/seattrellis/blob/main/benchmarks/solver-regression-2026-10-01.json) preserves raw paired measurements and source/binary hashes; it does not replace the historical baseline.

## Long-run quality gates

Rust CI also runs release-mode candidate and rotation gates:

```bash
cargo test --release --locked -p seattrellis_core \
  --test candidates_gate --test long_run_gate -- --ignored
cargo test --release --locked -p seattrellis-application \
  --test rotation_gate -- --ignored
```

These gates exercise candidate generation, planted feasibility, cancellation,
resource stability, and 1/3/5/10/20-period rotation behavior. The v2 quality
contract is Rust tests plus committed fixtures and baselines.

## Retired migration-era gates

The migration previously included an OR-Tools quality comparison and a
cross-implementation corpus comparison against the frozen Python line. Both
depended on the v1 oracle and were removed after v2.0.0. They are historical
evidence only and are not runnable v2 gates.

## Dataset shape

The planted-feasible performance cases use deterministic synthetic rosters and
seat grids. Long-running quality tests additionally vary candidate counts and
rotation periods. No case reads real student data.

If the synthetic data construction changes, create a new dataset version rather
than changing `synthetic-v1`; historical reports must remain comparable.

## Reports and historical baseline

The long-run gates run on the main and pull-request paths. Regression review
compares like-for-like runners and also watches feasibility rate, candidate
yield, and candidate diversity. The performance gate enforces both the reviewed
relative baseline and the explicit absolute bounds above.

[v1.4 performance baseline](benchmark-baseline-v1.4.md) is retained as a
historical Python/OR-Tools measurement record. v2.0.0 quality and performance
are governed by the Rust gates described here.

## Current backend experiment

The 2026-10-01 hard-only Rust/OR-Tools CP-SAT experiment is separate from the retired oracle comparison. Its reproducible corpus, raw trials and independent validation are under `benchmarks/solver-comparison/`; see [technology decisions](technology-decisions.md). It does not compare weighted objectives or establish a language-level speed claim.
