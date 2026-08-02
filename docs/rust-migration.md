# Rust-first migration

SeatTrellis now uses Rust as the primary path for a compact, offline desktop
application. The decision is driven by distribution size and startup time: a
release build should be usable without a Python, Node.js, Streamlit, or
OR-Tools installation and should remain in the 5–20 MB desktop range.

This is a migration, not a claim that every Python command has already been
reimplemented. The Python package remains the compatibility and library path
while the native contracts are completed and compared against it.

## Target architecture

```text
React/TypeScript workbench
          │
Rust loopback App / Tauri shell
          │
seattrellis_core (versioned JSON DTOs)
          │
Rust validation, scoring, heuristic solve, and render/export

Python CLI / Streamlit / PyO3 extension
          │
Existing Python service and OR-Tools backend
```

The React build is shared by browser and desktop. The Rust App embeds the
production files into the binary, while `SEATTRELLIS_WEB_STATIC` remains a
development override. A copied release binary therefore does not depend on
the source checkout or a runtime frontend installation.

## Delivered so far

- `native/seattrellis_core`: versioned JSON problem/response contracts,
  hard-rule validation, graph distances, cost scoring, and a deterministic
  cost-ranked heuristic solver;
- `native/seattrellis_cli`: dependency-light single-file `solve` and `export`
  commands for SVG, HTML, PNG, and PDF;
- `app/`: loopback Rust server for roster import, generation, editing,
  export, layouts, projects, migration, rotation, and group registers;
- `app/build.rs`: compile-time embedding of the React workbench (source maps
  are left out of the binary because they are not loaded at runtime);
- `app/src-tauri/`: Tauri 2 shell that starts the Rust App and opens a native
  window;
- Rust CI for core, CLI, and App tests on Linux, Windows, and macOS, with core
  MSRV 1.83 checking.

The current local measurements are approximately 1.6 MiB for the CLI, 2.7 MiB
for the embedded App server, and 9 MiB for the Tauri shell on macOS. These are
engineering measurements, not yet signed release artifacts.

## Known compatibility boundaries

The following gaps must stay visible in release notes and documentation:

1. The Rust CLI is not yet a drop-in replacement for the Python CLI. It now
   exposes native input validation, but history reports, project commands,
   schema migration commands, and full candidate-set reporting are still
   pending.
2. The Rust solver is a heuristic implementation. It satisfies the native
   hard-rule contract and covers the currently ported objectives, but it is not
   an exact replacement for the Python OR-Tools CP-SAT backend.
3. Python/Rust differential coverage must include all supported rule fields,
   not only the current core fixtures. A passing Rust unit suite alone is not
   sufficient evidence of behavioral parity.
4. Tauri installers, signing/notarization, and clean-machine installation tests
   are still release work. The repository now has a reproducible
   `.github/workflows/tauri.yml` path for unsigned `.app`/`.dmg`, `.msi`/NSIS,
   and `.deb`/AppImage bundles. It attaches only to an existing release: run it
   manually with a release tag, or publish a `desktop-v*` release.

## Migration stages

### Stage 1: freeze contracts

- Keep the versioned JSON DTOs and editing command protocol as the boundary
  between UI and runtime.
- Add a capability response so the React workbench can hide commands a backend
  does not support instead of failing after submission.
- Keep Python project, snapshot, and rules files readable by the native path.

### Stage 2: complete the native application surface

- Add native `validate`, history/pair reports, project operations, schema
  migration, and candidate comparison to the CLI or a shared native command
  layer.
- Move file selection, privacy checks, and export configuration into the Rust
  App service rather than duplicating them in the UI.
- Add a short-lived local session token or an equivalent origin-bound guard for
  the loopback API before shipping installers.

### Stage 3: prove solver quality

- Run fixed 40/50/60-student datasets through Python fallback, OR-Tools, and
  Rust with the same hard constraints and seeds.
- Compare feasibility, hard-rule violations, objective values, candidate
  diversity, peak memory, and wall-clock time.
- Keep Python OR-Tools available as a compatibility backend until Rust meets
  the agreed quality and performance gates.

### Stage 4: publish the compact desktop

- Build reproducible CLI, App, and Tauri artifacts for the three supported
  desktop platforms.
- Attach checksums, installation instructions, and measured cold-start/size
  data to a release.
- Complete signing, notarization, clean-machine E2E, uninstall checks, and
  offline behavior checks.

### Stage 5: decide the Python deprecation window

Only after the native parity matrix is green should a future major version
change the default runtime. Python 1.x files and APIs remain supported during
the migration; C/C++ is not part of the mainline architecture.

## Local verification

```bash
cargo test --locked --manifest-path native/Cargo.toml \
  -p seattrellis_core -p seattrellis_cli
cargo test --locked --manifest-path app/Cargo.toml
cargo clippy --all-targets --manifest-path app/Cargo.toml -- -D warnings
cargo build --release --locked --manifest-path app/Cargo.toml
```

To verify the standalone path, run the release binary from a directory that
does not contain `src/seattrellis/web_static`; the startup log should say it is
serving from `<embedded>/src/seattrellis/web_static` and `/api/v1/health` should
return a successful response.
