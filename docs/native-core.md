# Rust Core

SeatTrellis v2.0.0 is implemented in Rust. `seattrellis_core` is the semantic
source of truth for rule compilation, legality checks, the editing state model,
migration-facing contracts, privacy decisions, scoring, and solver statuses.
The CLI, loopback App server, Tauri shell, and React workbench build on this
core rather than maintaining separate seating logic.

## Runtime

The v2 runtime has no Python, Node.js, or OR-Tools dependency:

- `seattrellis_cli` is the standalone solve, report, project, migration, and
  export tool;
- `seattrellis_app` is the loopback HTTP server at `127.0.0.1` by default and
  embeds the React workbench assets;
- `app/src-tauri/` is the Tauri 2 desktop shell.

The temporary PyO3 compatibility extension used during the v1-to-v2 migration
was never the default solver. It was retired before the v2.0.0 release and is
not part of the v2 source tree or release artifacts.

## Build and test

```bash
cargo test --locked -p seattrellis_core
cargo test --locked -p seattrellis_cli
cargo clippy --all-targets -p seattrellis_core -p seattrellis_cli -- -D warnings
```

The Python line remains frozen at 1.9.0 on `v1.x-maintenance` as a legacy
package only. Its migration-era oracle/differential infrastructure was removed
after v2.0.0 and does not affect v2 builds, runs, or distributions.
