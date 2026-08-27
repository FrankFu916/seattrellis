# API Reference

SeatTrellis v2.0.0 exposes a loopback HTTP API through `seattrellis_web` and
`seattrellis-server`. The complete OpenAPI contract is
[`api-v1-openapi.json`](api-v1-openapi.json).

Every `/api/*` endpoint except `GET /api/v1/session` requires
`Authorization: Bearer <session-token>`. The request `Host` must be the loopback
address and bound port. When present, `Origin` must be same-origin. The server
also sends CSP, `X-Frame-Options: DENY`, and `Referrer-Policy: no-referrer`.

The v1 Python APIs (`seattrellis.service` and `seattrellis.service_types`) are
not part of v2. They remain only in the frozen 1.9.0 legacy package on
`v1.x-maintenance`.

## Solver responses

`POST /api/v2/solve` is the stable, side-effect-free solver contract. Valid
solver outcomes are returned as HTTP 200 domain results with a status of
`Solved`, `ProvenInfeasible`, `Timeout`, or `Unknown`; malformed input is an
HTTP error. A valid result is independently checked before it is returned.

`POST /api/v1/classes/generate` and `POST /api/v1/solve` are workbench adapters.
They accept either a raw `CoreSolveRequest` or a `GenerateClassRequest` with a
`draft`, room template/custom layout, and goal. A solved response includes
candidate summaries and an editable draft; infeasible, timeout, and unknown
results remain normal domain responses without a draft.

`POST /api/v1/classes/rotation` sequentially generates a multi-period rotation
plan. Failure for an individual period is represented in the domain response,
not converted into a successful-looking plan.

## System and rule endpoints

| Method and path | Purpose |
| --- | --- |
| `GET /api/v1/session` | Bootstrap the browser session token; the one unauthenticated API endpoint |
| `GET /api/v1/health` | Return service health and API version |
| `GET /api/v1/catalogs` | Return bilingual room, goal, and export catalogs |
| `GET /api/v1/rules/templates` | Return rule-builder sentence templates |
| `POST /api/v1/rules/compile` | Compile a filled rule-builder template in Rust |
| `POST /api/v1/rules/validate` | Validate a custom rules JSON document and return diagnostics |
| `POST /api/v1/files/read` | Read a relative file under the trusted local root |
| `GET /api/v1/files/root` | Return the trusted local root |

File reads reject absolute paths, traversal, NUL bytes, backslash separators,
symlink escapes, non-files, and oversized files.

## Roster, layout, and editing endpoints

| Method and path | Purpose |
| --- | --- |
| `POST /api/v1/rosters/drafts` | Upload a CSV/XLSX/XLSM roster draft |
| `GET /api/v1/rosters/drafts/{draft_id}` | Fetch a parsed roster draft |
| `POST /api/v1/rosters/drafts/{draft_id}/preview` | Preview an incremental or replacement roster update |
| `DELETE /api/v1/rosters/drafts/{draft_id}` | Delete a roster draft |
| `POST /api/v1/layouts/drafts` | Create a layout draft from rows/columns or a template |
| `GET /api/v1/layouts/drafts/{draft_id}` | Fetch a layout draft |
| `POST /api/v1/layouts/drafts/{draft_id}/commands` | Apply a revision-checked layout command |
| `GET /api/v1/layouts/drafts/{draft_id}/compiled` | Compile a layout into a solvable classroom layout |
| `DELETE /api/v1/layouts/drafts/{draft_id}` | Delete a layout draft |
| `GET /api/v1/editing/drafts/{draft_id}` | Fetch the current editor state |
| `POST /api/v1/editing/drafts/{draft_id}/commands` | Apply, undo, or redo an editor command |
| `GET /api/v1/editing/drafts/{draft_id}/audit` | Audit the current draft and score |

Editing commands use `EditorCommandEnvelope` and return a minimal
`EditorStateEnvelope` plus validation for mutations. See
[Editor protocol](editor-protocol.md).

## Export endpoint

`POST /api/v1/exports` renders the current editor draft and returns a binary
attachment. It supports all eight formats: `svg`, `html`, `print-html`, `png`,
`pdf`, `xlsx`, `docx`, and `pptx`.

The request supports `template: public | teacher | report`; privacy options can
hide scores, notes, special needs, height, and vision, and can anonymize labels.
`orientation`, `paper_size`, `margin_mm`, `page_scale`, `locale`, and
`show_student_ids` control presentation. The `public` template is fail-closed
and cannot be used to expose identifiers or sensitive fields. PDF and PNG
rasterize with a local system font; see [Export formats](export.md).

## Project endpoints

| Method and path | Purpose |
| --- | --- |
| `GET /api/v1/projects/recent` | List recent projects by local root and limit |
| `POST /api/v1/projects/history` | Return privacy-safe project artifact metadata |
| `POST /api/v1/projects/artifacts/compare` | Compare two project artifacts without returning student values |
| `POST /api/v1/projects/artifacts/restore` | Restore an artifact as a new output snapshot |
| `POST /api/v1/projects/privacy` | Scan a project for sensitive fields |
| `POST /api/v1/projects/bundle` | Download a `.seattrellis.zip` bundle |
| `POST /api/v1/projects/restore` | Restore an uploaded bundle to a local directory |
| `POST /api/v1/projects/migration/preview` | Preview a project or artifact migration |
| `POST /api/v1/projects/migration/apply` | Apply a migration with backup and validation |
| `POST /api/v1/projects/migration/reference-checks` | Check cross-artifact references |
| `POST /api/v1/projects/migration/batch/preview` | Preview several project migrations |
| `POST /api/v1/projects/migration/batch/apply` | Apply a validated batch migration |
| `POST /api/v1/projects/migration/restore` | Restore a migration backup |
| `POST /api/v1/projects/rotation/save` | Save current period drafts as a rotation plan |
| `POST /api/v1/projects/rotation/load` | Load a rotation plan into editable drafts |
| `POST /api/v1/projects/rotation/group-register` | Download an HTML or CSV group register |
| `POST /api/v1/projects/rotation/group-register/preview` | Preview group sizes and period changes |
| `POST /api/v1/projects/rotation/group-register/save` | Save group-register assignments |

Project responses expose metadata and anonymous references where possible. Path,
manifest, bundle, and migration operations reject traversal and unsafe archive
entries. Migration defaults to a new `*.migrated.json` file; in-place replacement
creates a backup and validates the result.

## Editor and project privacy

Editor state includes student keys, display names, seat relationships, locks,
undo/redo depth, and constraint diagnostics. It excludes scores, notes, special
needs, height, vision, tags, and extension attributes. Minimal state is not
anonymous, so do not send it to remote telemetry or public logs.

Project history, comparison, restore, and rotation endpoints avoid returning
student records where their contract permits. Uploaded project JSON is only a
manifest; the browser cannot read the files it references unless the user
explicitly uses local path mode.

Solver outcomes are domain values, not a reason to change HTTP status. Malformed
requests, authentication failures, missing drafts, and internal failures still
use the documented HTTP error responses.

## Related documents

- [Architecture](architecture.md)
- [Editor protocol](editor-protocol.md)
- [Project workflow](project.md)
- [Export formats](export.md)
