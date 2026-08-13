# Rust migration (completed)

SeatTrellis v2 is the Rust-only line. The migration from the v1 Python product
is complete: the product now ships as a native Rust CLI, a loopback App server
with the React workbench, and a Tauri 2 desktop shell. There is no Python,
Node.js, Streamlit, OR-Tools, or PyO3 runtime in the v2 tree or in any v2
release artifact.

The v1 Python line is frozen at **1.9.0** (`v1.x-maintenance` branch). It
exists only as a legacy package (`pip install seattrellis==1.9.0`) and as the
behavioral oracle for the Python↔Rust differential suite; it receives no new
features.

## Target architecture

```text
React/TypeScript workbench (clients/web)
          │
Rust loopback App server (seattrellis_app) / Tauri 2 shell (app/src-tauri)
          │
seattrellis_core (versioned JSON DTOs, hard-rule validation, scoring,
                 heuristic solve, candidates, audit, reports)
          │
seattrellis-export renderers (svg/html/print-html/png/pdf/xlsx/docx/pptx)
```

The React build is shared by the browser workbench and the desktop shell. The
App server embeds the production frontend files into the binary, while
`SEATTRELLIS_WEB_STATIC` remains a development override. A copied release
binary therefore does not depend on the source checkout or a runtime frontend
installation.

## Delivered

- `crates/seattrellis-core`: versioned JSON problem/response contracts
  (`CoreSolveRequest` / `CoreSolveResponse`), the frozen seven-status solver
  vocabulary, hard-rule validation, graph distances, cost scoring, candidate
  generation, audit and reports;
- `crates/seattrellis-cli`: the `seattrellis_cli` binary with 28 subcommands —
  `doctor`, `validate`, `precheck`, `audit`, `score`, `candidates`,
  `history-report`, `pair-report`, `repair`, `edit`, the `project-*` lifecycle,
  the `schema-*` tooling, `solve`, and `export`;
- `app/`: the loopback Rust server for roster import, generation, editing,
  export, layouts, projects, migration, rotation, and group registers, with
  the React workbench embedded;
- `app/src-tauri/`: the Tauri 2 shell that starts the App server and opens a
  native window;
- Rust CI for core, CLI, App, and the Tauri shell on Linux, Windows, and
  macOS, with MSRV 1.88 checking and release binary builds.

## Version policy

v2 is the current MAJOR line; crate versions live in `Cargo.toml` (currently
`2.0.0-rc.1`). v1 tags (`v1.*`) belong to the frozen legacy line and do not
receive Rust binaries. v1-era files (CSV rosters, layout/rules JSON, snapshots,
candidate sets, projects) are migrated to v2 automatically, with backups
created before each migration.

## Solver status and exit codes

The solver reports one of `Solved / ProvenInfeasible / Timeout / Unknown /
InvalidInput / Cancelled / InternalError`. Heuristic exhaustion is `Unknown` —
never a fake `ProvenInfeasible` — and a valid incumbent reports `Solved` even
when a timeout fired. The CLI exit table is frozen: 0 / 2 / 3 / 4 / 5 / 70 /
130.

## Local verification

```bash
cd clients/web && npm ci && npm run build && cd ../..   # embed the workbench

cargo test --locked -p seattrellis_core
cargo test --locked -p seattrellis_cli
cargo test --locked -p seattrellis_app
cargo clippy --all-targets -p seattrellis_core -p seattrellis_cli -- -D warnings
cargo clippy --all-targets -p seattrellis_app -- -D warnings
cargo build --locked -p seattrellis_desktop               # Tauri shell
```

To verify the standalone path, run the release binary from a directory that
does not contain a workbench build; the startup log should say it is serving
the embedded assets and `/api/v1/health` should return a successful response.

See [development.md](development.md) for the oracle differential commands that
compare the Rust implementation against the frozen v1.9.0 oracle.
