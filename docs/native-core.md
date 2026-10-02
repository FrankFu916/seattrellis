# Rust Core

SeatTrellis 2.1.0 uses shared Rust layers for rule compilation, legality,
editing, migration contracts, privacy, scoring, export and solver statuses.
The CLI, loopback App server, Tauri shell, React workbench and macOS native
preview reuse those layers. The core solver does not own all of those concerns;
the domain, IO, export and application crates provide their respective behavior.

## Runtime

The v2 runtime has no Python, Node.js, or OR-Tools dependency:

- `seattrellis` is the standalone solve, report, project, migration, and
  export tool;
- `seattrellis_web` is the loopback HTTP server at `127.0.0.1` by default and
  embeds the React workbench assets;
- `app/src-tauri/` is the Tauri 2 desktop shell;
- `seattrellis-native-bridge` exposes an in-process [C ABI](native-bridge.md);
- `clients/macos/` is the first SwiftUI/AppKit preview using that ABI. Its
  unsigned bundle and model tests build on macOS CI; manual accessibility,
  signing and notarization remain target-platform work.

The temporary PyO3 compatibility extension used during the v1-to-v2 migration
was never the default solver. It was retired before the v2.0.0 release and is
not part of the v2 source tree or release artifacts.

## Build and test

```bash
cargo test --locked -p seattrellis_core
cargo test --locked -p seattrellis
cargo clippy --all-targets -p seattrellis_core -p seattrellis -- -D warnings
```

The Python line remains frozen at 1.9.0 on `v1.x-maintenance` as a legacy
package only. Its migration-era oracle/differential infrastructure was removed
after v2.0.0 and does not affect v2 builds, runs, or distributions.
