# ADR-0003: Rust-first compact desktop runtime

## Status

Accepted — 2026-08-02

## Context

The Python implementation is mature and remains the compatibility path, but a
desktop application that bundles Python, Streamlit, OR-Tools, and their native
dependencies is too large and slow for the intended offline teacher workflow.
The project also needs one local runtime that can be copied to another machine
without a language environment installation.

## Decision

Rust is the primary runtime for the compact desktop distribution:

- `seattrellis_core` owns versioned native DTOs, validation, graph work,
  scoring, and the native heuristic solver;
- the Rust App owns the loopback service and embeds the built React workbench;
- Tauri is the desktop shell; the App server can also run standalone for
  diagnostics and headless smoke tests;
- Python remains a supported compatibility/library path while migration gates
  are completed;
- Python OR-Tools is retained as an explicit compatibility backend until the
  Rust solver has passed differential quality and performance checks;
- C/C++ is not introduced as a second business-logic implementation.

## Consequences

Positive consequences:

- small, offline binaries with no Python or Node runtime requirement;
- memory-safe native code reusable by CLI, App, Tauri, and future bindings;
- a single React workbench shared between browser and desktop;
- explicit JSON contracts make staged migration and differential testing
  possible.

Costs and constraints:

- the Rust CLI and solver must grow toward Python feature parity;
- the project temporarily maintains Python and Rust implementations;
- installers, signing, cross-platform E2E, and native release CI become first
  class work;
- the heuristic solver cannot be advertised as exact CP-SAT behavior until the
  comparison gates are met.

## Rejected alternatives

- A full Python rewrite was rejected because it does not meet the binary-size
  and offline distribution goal.
- C++ as a parallel core was rejected because it would duplicate domain logic
  and add another toolchain without a demonstrated benefit.
- Rewriting the React workbench in Rust was rejected; the existing frontend is
  shared by browser and desktop and should remain the UI contract.
