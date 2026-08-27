# Development Guide

SeatTrellis v2.0.0 is a Rust-first workspace. `crates/` contains the layered
schema, rules, domain, application, I/O, export, server, core, and CLI crates;
`app/` is a thin server facade; `app/src-tauri/` is the Tauri 2 desktop shell;
and `clients/web/` is the React 19 workbench. Python is used only by selected
development or performance tooling, not as a v2 application runtime.

## Build and test

The App server embeds `clients/web/dist`, so build the frontend before a
workspace-level Cargo command that compiles the server:

```bash
cd clients/web && npm ci && npm run build && cd ../..

cargo test --locked -p seattrellis_core
cargo test --locked -p seattrellis_cli
cargo clippy --all-targets -p seattrellis_core -p seattrellis_cli -- -D warnings

cargo test --locked -p seattrellis_app
cargo clippy --all-targets -p seattrellis_app -- -D warnings

# Tauri shell; the workspace pins Rust 1.88 as its MSRV
cargo build --locked -p seattrellis_desktop

cd clients/web && npm test && npm run typecheck && npm run build

# Generated schemas, OpenAPI, and TypeScript client contract
cargo run -p xtask -- contract check
```

## Architecture rules

- Rust is the single source of truth for rule compilation, legality, editing
  state, migration, privacy, and solver status. React renders and edits through
  DTOs; it must not re-derive domain truth.
- Transport and UI code must not reach into domain, rules, or solver internals.
  `serde_json::Value` is for migration, extension namespaces, and transport
  boundaries.
- Every solve, edit, repair, rotation, and export artifact passes an independent
  validator before acceptance. No path may hard-code `feasible=true`.
- Solver statuses are frozen as `Solved`, `ProvenInfeasible`, `Timeout`,
  `Unknown`, `InvalidInput`, `Cancelled`, and `InternalError`. Heuristic
  exhaustion is `Unknown`, never a false `ProvenInfeasible`.
- CLI exit codes are frozen as `0 / 2 / 3 / 4 / 5 / 70 / 130`.
- Every `/api/*` write uses the loopback host/origin checks and bearer session
  token. New write paths must not bypass the server middleware.

## Retired migration tooling

During the v1-to-v2 migration, Rust behavior was compared with the frozen
Python 1.9.0 line. That oracle, its differential harness, fixture generators,
and related CI jobs were removed after v2.0.0. There is no oracle installation
or regeneration workflow.

Current regression coverage comes from the Rust test suite, committed CLI
goldens, browser E2E, fuzz targets, and the Rust solver performance gate. The
Python performance runner measures the Rust binary only; it is not a Python
oracle or a parity test. Frozen inputs and their ownership are documented in
`fixtures/README.md`.

See [Testing](testing.md), [Architecture](architecture.md), and
[Rust migration](rust-migration.md) for the current boundaries.
