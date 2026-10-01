# Testing and Acceptance

SeatTrellis v2.0.0 is a Rust-first product. The test strategy has five layers:
Rust unit/integration tests, application smoke tests, browser E2E, performance
gates, and release acceptance. Fast targeted tests are suitable during
development; the complete gate runs before a release.

## Local automated tests

The App server embeds `clients/web/dist`. Build the frontend before running
workspace-level commands that compile the server:

```bash
cd clients/web && npm ci && npm run build && cd ../..

cargo test --locked -p seattrellis_core
cargo test --locked -p seattrellis
cargo clippy --all-targets -p seattrellis_core -p seattrellis -- -D warnings

cargo test --locked -p seattrellis_web
cargo clippy --all-targets -p seattrellis_web -- -D warnings

# Tauri shell
cargo build --locked -p seattrellis_desktop

# React workbench
cd clients/web && npm test && npm run typecheck && npm run build

# Generated schemas, OpenAPI, and TypeScript client contract
cargo run -p xtask -- contract check

# Repository hygiene and release-runtime boundary
python3 scripts/check_repository_hygiene.py
python3 scripts/check_no_python_runtime.py --tree --expect-retired
```

Rust is the semantic source of truth for rule compilation, legality, editing,
migration, privacy, and solver status. The seven statuses are `Solved`,
`ProvenInfeasible`, `Timeout`, `Unknown`, `InvalidInput`, `Cancelled`, and
`InternalError`; CLI exit codes are `0/2/3/4/5/70/130`. Every solve, edit,
repair, rotation, and export artifact is independently validated. No test or
runtime path may hard-code `feasible=true`.

## Retired oracle tooling

The v1-to-v2 migration once used a frozen Python 1.9.0 oracle for cross-
implementation and differential checks. The oracle environment, comparison
harness, fixture generators, and their CI jobs were removed after v2.0.0. Do
not install or run that retired workflow; it is not a current release test.

## Current v2 baseline

Regression coverage now comes from:

- Rust unit, integration, property-style, and fuzz tests;
- live CLI subprocess tests in `cli_arg_sweep.rs` and `lifecycle_regressions.rs`,
  including exit codes, dry-run immutability, saved-source roundtrips and project outputs;
  `fixtures/cli-goldens/` are archived oracle references with no current consumer;
- browser E2E for the workbench;
- release-mode candidate and rotation gates;
- the committed Rust solver performance baseline in
  `benchmarks/solver-baseline.json`.

The directories under `fixtures/` are frozen inputs or archived references, not generated data. See
`fixtures/README.md` for their ownership and purpose. Heuristic exhaustion must
remain `Unknown`, and no error may be recorded as `ProvenInfeasible`.

## Web smoke tests

Frontend tests and the production build run from `clients/web`:

```bash
cd clients/web
npm test -- --run
npm run typecheck
npm run build
```

Editor contract tests cover operation kinds, required version fields, strict
IDs and revisions, stale revisions, invalid drafts, duplicate command IDs,
atomic batch failure, and command-level undo/redo. State tests ensure the
protocol does not carry scores, notes, special needs, height, vision, or
extension attributes. Generated editor schemas and registry output are checked
for drift.

Before release, run a real Chromium flow covering roster mapping, room editing,
common and advanced rules, future rotation, manual adjustment, export, and the
Project panel.

## Browser E2E

The `web-e2e-rust` CI job uses Python only as a Playwright runner; it does not
install the v1 Python application. The browser scenarios cover:

- roster upload and mapping, candidate selection, lock/swap/undo and export;
- real-name and anonymous print/PDF exports and inert previews;
- two-period rotation save/reopen using captured source despite changed workspace files;
- portable class download/upload after page reload, preserving source fields and locks,
  followed by local repair and export; and context switching after a real Save As.

Native file pickers, Tauri windows and screen readers require target-platform tests;
Chromium upload/download coverage cannot certify them.

## Performance tests

The current performance gate measures the Rust release CLI on planted-feasible
40-, 50-, 60-, and 80-student instances:

```bash
cargo build --release --locked -p seattrellis
python3 scripts/bench_solver.py --check
```

The Python runner is only a harness for timing the Rust binary. It is not an
oracle, parity check, or differential test. It compares the committed baseline
with a 10% tolerance and an absolute bound; see [Benchmarks](benchmarks.md).

The long-run CI gates also exercise candidate generation, rotation, cancellation
and planted feasibility:

```bash
cargo test --release --locked -p seattrellis_core \
  --test candidates_gate --test long_run_gate -- --ignored
cargo test --release --locked -p seattrellis-application \
  --test rotation_gate -- --ignored
```

## Release smoke

Before publishing, exercise the CLI against fictional data:

```bash
seattrellis doctor
seattrellis validate --problem problem.json
seattrellis solve --problem problem.json --output plan.json
seattrellis candidates --problem problem.json --count 5
seattrellis history-report --problem problem.json --history-dir examples/history
seattrellis pair-report --problem problem.json --history-dir examples/history
seattrellis project-info --project examples/project.seattrellis.json
seattrellis project-validate --project examples/project.seattrellis.json
seattrellis export \
  --problem problem.json \
  --solution plan.json \
  --format png \
  --output plan.png
```

The Web acceptance flow must complete roster import, quick solve, result review,
export settings, and Project workspace operations. Use real school data only on
the local machine; never commit data, screenshots, exports, or logs.

## Related documents

- [Development guide](development.md)
- [Release checklist](release-checklist.md)
- [Benchmarks](benchmarks.md)
- [Editor protocol](editor-protocol.md)
