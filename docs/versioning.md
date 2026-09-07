# Versioning and Compatibility

## SemVer

SeatTrellis follows [Semantic Versioning 2.0.0](https://semver.org/):

- **MAJOR** introduces incompatible API or file-format changes;
- **MINOR** adds backward-compatible functionality;
- **PATCH** contains backward-compatible fixes.

SeatTrellis v2.0.0 is the current released Rust line. Crate versions are read
from `Cargo.toml`. The Python line is frozen at 1.9.0 (`v1.9.0` on
`v1.x-maintenance`) and is a legacy package only. From v2 onward, incompatible
changes to the public CLI, file formats, or HTTP API require a new MAJOR
version.

## Schema versions

Long-lived artifacts carry `schema_version`. The v2 artifact registry contains
these kinds, each with a current outer artifact version of `2`:

`student_roster`, `classroom_layout`, `rule_set`, `seating_snapshot`,
`candidate_set`, `plan_comparison`, `history_archive`, `rotation_plan`,
`editing_operation_log`, `project`, `project_bundle_manifest`, and
`export_preset`.

The project payload itself retains its project-file schema (`schema_version: 1`)
inside the v2 artifact envelope. Do not infer migration support from the
registry list. `schema-migrate` currently has explicit v1-to-v2 transforms for
student rosters, classroom layouts, and project files. Other artifact kinds are
rejected when no typed migration step exists.

Schemas are stored under `schemas/` and can be exported with:

```bash
seattrellis schema-export \
  --kind seating_snapshot \
  --output seating-snapshot.v2.schema.json
```

Migration validates and rewrites supported legacy inputs:

```bash
seattrellis schema-migrate --input roster-v1.json --dry-run
seattrellis schema-migrate --input roster-v1.json --output roster-v2.json
seattrellis schema-migrate --input project-v1.json --in-place
```

`--dry-run` validates without writing. In-place or destination replacement
creates a backup before the write; the backup name is implementation-managed.
Newer schema versions are rejected and never downgraded.

Editor commands and state are short-lived transport contracts, not durable
artifacts, and are not handled by `schema-migrate`. Their current protocol
version is `"1.0"`; clients must send `protocol_version`, and unsupported
versions are rejected before execution.

## CLI compatibility

Run `seattrellis --help` for the installed binary's exact options. The core
v2 surface keeps these names stable:

- `seattrellis solve`, `validate`, and `export`;
- `--problem`, `--solution`, `--output`, `--seed`, and `--time-limit`;
- exit codes `0 / 2 / 3 / 4 / 5 / 70 / 130`.

The detailed meanings are in the [CLI reference](cli.md).

## Deprecation policy

The v1 CLI and Python API exist only in the frozen legacy package. The v2 CLI
uses `seattrellis`; migration support is explicit rather than inferred.
When a future v2 feature is deprecated, documentation will identify it and the
runtime will provide a warning before removal in a later MAJOR line.

## Compatibility matrix

| Component | v2.0.0 baseline |
| --- | --- |
| Rust | MSRV 1.88; CI covers Linux, Windows, and macOS |
| Operating systems | macOS 13+, Windows 10+, Ubuntu 22.04+ targets |
| Desktop shell | Tauri 2 in `app/src-tauri` |
| Frontend | React 19 and TypeScript in `clients/web`; Node.js is build-only |
| Runtime dependencies | No Python, Node.js, OR-Tools, or Streamlit runtime |

## Frozen v1 compatibility

- `seattrellis==1.9.0` remains installable from PyPI through
  `v1.x-maintenance`;
- documented v1 roster, layout, and project inputs have v2 migration paths;
- v2 does not promise v1 command names or Python API compatibility;
- migration-era Python oracle/parity tooling was removed after v2.0.0 and is not
  a v2 compatibility requirement.
