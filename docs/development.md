# Development Guide

SeatTrellis v2.0.0 is a Rust-first workspace. `crates/` contains the layered
schema, rules, domain, application, I/O, export, server, core, and CLI crates;
`app/` is a thin server facade; `app/src-tauri/` is the Tauri 2 desktop shell;
and `clients/web/` is the React 19 workbench. Python is used only by selected
development or performance tooling, not as a v2 application runtime.

Use Rust 1.88 or newer, Node.js 22.12 or newer, and npm 10 or newer for local
development. End users do not need Node.js or Python.

## Build and test

The App server embeds `clients/web/dist`, so build the frontend before a
workspace-level Cargo command that compiles the server:

```bash
cd clients/web && npm ci && npm run build && cd ../..

cargo test --locked -p seattrellis_core
cargo test --locked -p seattrellis
cargo clippy --all-targets -p seattrellis_core -p seattrellis -- -D warnings

cargo test --locked -p seattrellis_web
cargo clippy --all-targets -p seattrellis_web -- -D warnings

# Tauri shell; the workspace pins Rust 1.88 as its MSRV
cargo build --locked -p seattrellis_desktop

cd clients/web && npm test && npm run typecheck && npm run build

# Generated schemas, OpenAPI, and TypeScript client contract
cargo run -p xtask -- contract check

# Repository privacy boundary and JavaScript supply-chain policy
python3 scripts/check_repository_hygiene.py
python3 scripts/check_npm_audit.py clients/web website
```

The documentation build currently receives `image-size` 2.0.2 transitively
from Docusaurus, and upstream has not released a fixed version. The repository
therefore allows only the two reviewed build-time denial-of-service advisories
until the expiry recorded in `security/npm-audit-allowlist.json`, while the
hygiene gate rejects the affected ICNS, JPEG XL, JPEG 2000, HEIF/HEIC, and AVIF
formats by both extension and magic bytes. A new advisory, version change,
expired exception, or stale resolved exception fails CI.

## Documentation localization

English source documents live in `docs/`. Every Markdown file has a matching
Simplified Chinese translation under
`website/i18n/zh/docusaurus-plugin-content-docs/current/`; navigation and theme
strings live beside them in the locale JSON files. `npm run build` builds both
the default English site and `/zh/`, and the repository hygiene gate rejects
missing, orphaned, or obviously wrong-language document pairs.

After adding or renaming a page, update both documents and the shared sidebar.
Run `npm run write-translations -- --locale zh` only when Docusaurus introduces
new UI keys, then translate the generated messages before committing them.

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
