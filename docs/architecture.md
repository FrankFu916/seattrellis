# Architecture

SeatTrellis v2.0.0 is a Rust-only product. Domain logic is split into layered
crates while React remains a presentation and interaction layer. Rust owns rule
compilation, legality, editing state, migration, privacy, and solver statuses;
the frontend must not duplicate those rules.

## Layers

| Crate | Responsibility |
| --- | --- |
| `seattrellis-schema` | Versioned JSON contracts and the artifact registry |
| `seattrellis-rules` | Rule DSL and registry for goals and presets |
| `seattrellis-domain` | Editing state, layout drafts, rotation, and group models |
| `seattrellis-application` | Use-case orchestration for generation, export, and audits |
| `seattrellis-io` | CSV/Excel import, migration, projects, rotation, and roster drafts |
| `seattrellis-export` | Eight renderers: SVG, HTML, print HTML, PNG, PDF, XLSX, DOCX, PPTX |
| `seattrellis-server` | Loopback HTTP transport and embedded workbench assets |
| `seattrellis-core` | Hard-rule search, local search, candidates, scoring, audit, and validation |
| `seattrellis-cli` | The command-line adapter with 27 operational commands plus help |

`app/` is the thin `seattrellis_web` facade over the server. `app/src-tauri/` is
the Tauri 2 shell; it owns the window lifecycle and does not contain a second
seating-rule implementation.

## Runtime shape

```text
React workbench (clients/web)
          |
seattrellis_web (loopback HTTP, 127.0.0.1:8765) / Tauri 2 shell
          |
seattrellis-server -> seattrellis-application -> seattrellis-core
          |
local project and export I/O (seattrellis-io / seattrellis-export)
```

`seattrellis_core` and the App use coarse, versioned JSON DTOs
(`CoreSolveRequest` and `CoreSolveResponse`). The frontend does not call the
solver one seat at a time. Production frontend assets are embedded in the App
binary; `SEATTRELLIS_WEB_STATIC` is a development-only asset override.

## Editing protocol

Cross-surface editing uses versioned `EditorCommandEnvelope` and
`EditorStateEnvelope` messages with `protocol_version: "1.0"`, implemented in
`seattrellis-domain::editing`. Every draft has a unique `draft_id` and monotonic
revision; every command has a unique `command_id`. The server rejects the wrong
draft, duplicate commands, and stale revisions before writing, and treats all
operations in one command as one atomic undo batch. See
[Editor protocol](editor-protocol.md).

The state protocol is deliberately minimal: student names and student/seat
relationships, lock state, and constraint diagnostics. It excludes scores,
notes, special needs, height, vision, tags, and extension attributes.

## Solver boundary

- Hard constraints (fixed seats, required/forbidden adjacency, minimum distance,
  and groups) are checked for static conflicts before candidate-domain and
  matching search.
- Solver status vocabulary is frozen: `Solved`, `ProvenInfeasible`, `Timeout`,
  `Unknown`, `InvalidInput`, `Cancelled`, and `InternalError`.
- Heuristic exhaustion is `Unknown`, never a fabricated `ProvenInfeasible`. A
  valid incumbent remains `Solved` even when a time limit fires.
- Every solve, edit, repair, rotation, and export artifact is independently
  validated before it is accepted. No path may hard-code `feasible=true`.

## HTTP and security boundary

Every `/api/*` endpoint except `GET /api/v1/session` requires
`Authorization: Bearer <token>`. The `Host` header must identify the loopback
address and bound port to prevent DNS rebinding. When present, `Origin` must be
same-origin to prevent CSRF. Responses include CSP, `X-Frame-Options: DENY`, and
`Referrer-Policy: no-referrer`.

The server generates a 256-bit token at startup. Tauri injects it through an
initialization script; the browser workbench obtains it through the session
bootstrap endpoint. Body limits, concurrency limits, and graceful shutdown are
enforced in `seattrellis-server`, and new write paths must use the same boundary.
The stable side-effect-free solver endpoint is `POST /api/v2/solve`; see the
[API reference](api.md).

All file operations are local by default. The product has no cloud sync or
telemetry. The Python v1 line is frozen at 1.9.0 on `v1.x-maintenance` and is a
legacy package only; its migration-era oracle/differential infrastructure was
removed after v2.0.0 and is not part of the v2 tree.
