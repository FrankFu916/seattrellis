# Rust Migration (Complete)

SeatTrellis v2.0.0 is the released Rust-only line. The migration from the v1
Python product is complete: releases contain a native Rust CLI, a loopback App
server with the React workbench, and a Tauri 2 desktop shell. End users do not
need a Python or Node.js runtime, and v2 artifacts do not include Streamlit,
OR-Tools, or PyO3.

The Python line is frozen at **1.9.0** on `v1.x-maintenance`. It remains
available only as a legacy package (`pip install seattrellis==1.9.0`) and
receives no new features. During migration it served as a behavioral reference;
that oracle/differential infrastructure was removed after v2.0.0.

## Target architecture

```text
React/TypeScript workbench (clients/web)
          |
Rust loopback App server (seattrellis_web) / Tauri 2 shell (app/src-tauri)
          |
seattrellis_core (versioned DTOs, hard-rule validation, scoring,
                  heuristic solve, candidates, audit, reports)
          |
seattrellis-export (SVG/HTML/print HTML/PNG/PDF/XLSX/DOCX/PPTX)
```

The browser and desktop shell share one React build. The App server embeds the
production frontend in its binary, while `SEATTRELLIS_WEB_STATIC` remains a
development override. A copied release binary therefore does not depend on the
source checkout or a separate frontend installation.

## Delivered in v2.0.0

- `seattrellis-core`: versioned problem/response contracts, seven solver
  statuses, hard-rule validation, graph distances, scoring, candidates, audits,
  and reports;
- `seattrellis-cli`: the solve, validation, report, edit/repair, project,
  schema, and export command surfaces;
- `seattrellis_web`: local roster, generation, editing, layout, project,
  migration, rotation, group-register, and export APIs;
- `app/src-tauri/`: the Tauri 2 desktop shell over the App server;
- Rust CI for core, CLI, App, and desktop builds on Linux, Windows, and macOS,
  with MSRV 1.88 checks and release binary builds;
- eight local export formats with teacher/public privacy templates and a
  print-oriented A4 landscape `print-html` path.

## Version policy

v2.0.0 is the current MAJOR line; crate versions are declared in `Cargo.toml`.
`v1.*` tags belong to the frozen legacy line and do not receive Rust binaries.
Documented v1 roster, layout, and project inputs have explicit migration paths;
unsupported artifact kinds are rejected rather than guessed.

## Solver status and exit codes

The solver reports `Solved`, `ProvenInfeasible`, `Timeout`, `Unknown`,
`InvalidInput`, `Cancelled`, or `InternalError`. Heuristic exhaustion is
`Unknown`, never a fabricated `ProvenInfeasible`; a valid incumbent remains
`Solved` even when a timeout fires. The CLI exit table is frozen at
`0 / 2 / 3 / 4 / 5 / 70 / 130`.

## Local verification

```bash
cd clients/web && npm ci && npm run build && cd ../..

cargo test --locked -p seattrellis_core
cargo test --locked -p seattrellis
cargo test --locked -p seattrellis_web
cargo clippy --all-targets -p seattrellis_core -p seattrellis -- -D warnings
cargo clippy --all-targets -p seattrellis_web -- -D warnings
cargo build --locked -p seattrellis_desktop
```

To verify the standalone App path, run a release binary from a directory that
does not contain a workbench build. It should serve its embedded assets, and
`/api/v1/health` should return a successful response after the session boundary
is satisfied.

The current test baseline is documented in [Testing](testing.md). The retired
Python oracle/differential commands are not part of v2.0.0 and must not be
reintroduced as a release requirement.
