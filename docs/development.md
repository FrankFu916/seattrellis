# Development Guide

SeatTrellis v2 is a Rust-only workspace: `crates/` holds the layered crates
(schema, rules, domain, application, io, export, server, core, cli), `app/`
is a thin server facade, `app/src-tauri/` is the Tauri 2 desktop shell, and
`clients/web/` is the React 19 workbench. There is no Python runtime in the
v2 tree.

## Build & Test

```bash
# The server build script embeds clients/web/dist (the React build) — build
# the frontend before any workspace-level cargo command:
cd clients/web && npm ci && npm run build && cd ../..

cargo test --locked -p seattrellis_core
cargo test --locked -p seattrellis_cli
cargo clippy --all-targets -p seattrellis_core -p seattrellis_cli -- -D warnings

# Rust app server
cargo test --locked -p seattrellis_app
cargo clippy --all-targets -p seattrellis_app -- -D warnings

# Tauri shell (requires the 1.88 toolchain)
cargo build --locked -p seattrellis_desktop

# React workbench
cd clients/web && npm test && npm run typecheck && npm run build

# Contract drift check (generated schemas / OpenAPI / TS client)
cargo run -p xtask -- contract check
```

## Architecture Rules

- Rust is the single source of truth: rule compilation, legality, the editing
  state machine, migration, privacy and solver status are decided in Rust.
  The React layer renders and edits; it must not re-derive domain truth.
- Transport/UI code must not reach back into domain/rules/solver internals.
  `serde_json::Value` appears only at migration, extension namespaces and
  transport boundaries.
- Every solve/edit/repair/rotation/export artifact passes an independent
  validator before it is accepted — no hardcoded `feasible = true`.
- Solver status vocabulary (frozen): `Solved / ProvenInfeasible / Timeout /
  Unknown / InvalidInput / Cancelled / InternalError`. Heuristic exhaustion is
  `Unknown`, never a fake `ProvenInfeasible`.
- CLI exit codes (frozen): 0 / 2 / 3 / 4 / 5 / 70 / 130.
- The loopback HTTP boundary (M1-05) is mandatory: `/api/*` requires the
  bearer token, Host must be a loopback name + bound port, Origin is checked
  when present, and responses carry CSP / X-Frame-Options / Referrer-Policy.
  New write paths must not bypass these middleware checks.

## Oracle Differentials

The parity corpus and the Rust↔Python differential harness compare the Rust
implementation against the frozen v1.9.0 oracle (installed from the
`v1.9.0` tag):

```bash
python -m venv .oracle-venv
.oracle-venv/bin/pip install "seattrellis[all] @ git+https://github.com/FrankFu916/seattrellis@v1.9.0"
.oracle-venv/bin/python scripts/rust_python_diff.py --fixtures   # 41-case seven-state diff
.oracle-venv/bin/python scripts/rust_python_diff.py --cli-golden # 38-command golden diff
```
