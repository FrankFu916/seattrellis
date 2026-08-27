# Benchmarks

SeatTrellis tracks large-class performance with fixed synthetic data. The
dataset is `synthetic-classroom` / `synthetic-v1`; all students, seats, and
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

The tolerance absorbs normal CI hardware noise while the absolute bound catches
a major algorithmic regression. Baselines are recorded on comparable runners;
Apple Silicon local runs are expected to be faster. Updating a baseline is a
reviewed release-maintenance operation, not an ordinary documentation change.

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
yield, and candidate diversity; ordinary CI does not fail on an arbitrary fixed
number of seconds.

[v1.4 performance baseline](benchmark-baseline-v1.4.md) is retained as a
historical Python/OR-Tools measurement record. v2.0.0 quality and performance
are governed by the Rust gates described here.
