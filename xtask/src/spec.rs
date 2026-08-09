//! Declarative contract specification for the loopback API (M1-06).
//!
//! Single source of truth for the wire surface: every endpoint, its auth
//! requirement, request/response shapes and the core DTOs. Generators turn
//! this into `docs/api-v1-openapi.json` and the TypeScript client
//! (`clients/web/src/api/generated.ts`); `xtask contract check` fails on
//! drift between this spec and the committed artifacts.
//!
//! Accuracy note: request/response schemas list the fields the server
//! actually emits today. Complex documents (editor state, layout state, ...)
//! are `additionalProperties: true` until M2-01 lands the fully typed DTOs;
//! the required fields are accurate now.

use serde_json::{json, Value};

/// Build the complete OpenAPI 3.0.3 document for the loopback API.
pub fn spec() -> Value {
    json!({
        "openapi": "3.0.3",
        "info": {
            "title": "SeatTrellis loopback API",
            "version": "1.0.0",
            "description": "Loopback-only backend for the SeatTrellis workbench. \
    Every /api/* endpoint except GET /api/v1/session requires \
    `Authorization: Bearer <session token>`. The Host header must be the \
    loopback address the server is bound to; cross-origin requests are \
    rejected (M1-05)."
        },
        "servers": [{ "url": "http://127.0.0.1:{port}", "variables": { "port": { "default": "8765" } } }],
        "tags": [
            { "name": "system", "description": "Health, catalogs, session bootstrap, static assets" },
            { "name": "classes", "description": "Seating plan generation" },
            { "name": "rosters", "description": "Roster upload/preview drafts" },
            { "name": "editing", "description": "Command-driven seating editor drafts" },
            { "name": "layouts", "description": "Classroom layout drafts" },
            { "name": "exports", "description": "Plan export (8 formats)" },
            { "name": "projects", "description": "Project files: history, privacy, bundle, migration, rotation" }
        ],
        "paths": paths(),
        "components": {
            "securitySchemes": {
                "bearerSession": { "type": "http", "scheme": "bearer", "description": "256-bit loopback session token (GET /api/v1/session)" }
            },
            "schemas": schemas(),
            "responses": responses()
        }
    })
}

fn paths() -> Value {
    json!({
        "/": {
            "get": {
                "tags": ["system"],
                "summary": "Workbench entry point (index.html)",
                "security": [],
                "responses": { "200": { "description": "The compiled workbench HTML" } }
            }
        },
        "/api/v1/session": {
            "get": {
                "tags": ["system"],
                "summary": "Bootstrap: issue the session token to a same-origin page",
                "description": "Host-checked; no Bearer required. The token is injected into the \
    WebView memory by the shell, or fetched here by the browser workspace.",
                "security": [],
                "responses": {
                    "200": { "description": "The session token", "content": { "application/json": { "schema": { "$ref": "#/components/schemas/SessionResponse" } } } },
                    "400": { "$ref": "#/components/responses/InvalidHost" }
                }
            }
        },
        "/api/v1/health": {
            "get": {
                "tags": ["system"],
                "summary": "Health probe",
                "responses": { "200": { "description": "Service health", "content": { "application/json": { "schema": { "$ref": "#/components/schemas/HealthResponse" } } } } }
            }
        },
        "/api/v1/catalogs": {
            "get": {
                "tags": ["system"],
                "summary": "Room templates, teacher goals and export formats",
                "responses": { "200": { "description": "Bilingual catalogs" } }
            }
        },
        "/api/v1/classes/generate": {
            "post": {
                "tags": ["classes"],
                "summary": "Generate seating plans and open an editable draft",
                "description": "Two request shapes: the raw CoreSolveRequest or the workbench \
    GenerateClassRequest (draft.students + draft.room + draft.goal).",
                "requestBody": { "required": true, "content": { "application/json": { "schema": { "oneOf": [
                    { "$ref": "#/components/schemas/CoreSolveRequest" },
                    { "$ref": "#/components/schemas/GenerateClassRequest" }
                ] } } } },
                "responses": {
                    "200": { "description": "GenerateClassResponse with candidates + editor draft", "content": { "application/json": { "schema": { "$ref": "#/components/schemas/GenerateClassResponse" } } } },
                    "400": { "$ref": "#/components/responses/InvalidInput" },
                    "409": { "description": "Heuristic exhaustion (status: Unknown)", "content": { "application/json": { "schema": { "$ref": "#/components/schemas/PlanNotFound" } } } },
                    "422": { "$ref": "#/components/responses/Unprocessable" },
                    "500": { "$ref": "#/components/responses/InternalError" }
                }
            }
        },
        "/api/v1/solve": {
            "post": {
                "tags": ["classes"],
                "summary": "Alias of POST /api/v1/classes/generate (raw CoreSolveRequest)",
                "requestBody": { "required": true, "content": { "application/json": { "schema": { "$ref": "#/components/schemas/CoreSolveRequest" } } } },
                "responses": {
                    "200": { "description": "GenerateClassResponse", "content": { "application/json": { "schema": { "$ref": "#/components/schemas/GenerateClassResponse" } } } },
                    "400": { "$ref": "#/components/responses/InvalidInput" },
                    "409": { "description": "Heuristic exhaustion", "content": { "application/json": { "schema": { "$ref": "#/components/schemas/PlanNotFound" } } } }
                }
            }
        },
        "/api/v1/rosters/drafts": {
            "post": {
                "tags": ["rosters"],
                "summary": "Upload a roster file (multipart field `file`)",
                "requestBody": { "required": true, "content": { "multipart/form-data": { "schema": { "type": "object", "properties": { "file": { "type": "string", "format": "binary" } }, "required": ["file"] } } } },
                "responses": {
                    "200": { "description": "RosterDraftResponse", "content": { "application/json": { "schema": { "$ref": "#/components/schemas/RosterDraftResponse" } } } },
                    "400": { "$ref": "#/components/responses/InvalidInput" },
                    "422": { "$ref": "#/components/responses/Unprocessable" }
                }
            }
        },
        "/api/v1/rosters/drafts/{draft_id}": {
            "get": {
                "tags": ["rosters"],
                "summary": "Fetch a roster draft",
                "parameters": [path_param("draft_id")],
                "responses": {
                    "200": { "description": "RosterDraftResponse", "content": { "application/json": { "schema": { "$ref": "#/components/schemas/RosterDraftResponse" } } } },
                    "404": { "$ref": "#/components/responses/NotFound" }
                }
            },
            "delete": {
                "tags": ["rosters"],
                "summary": "Delete a roster draft",
                "parameters": [path_param("draft_id")],
                "responses": {
                    "204": { "description": "Deleted" },
                    "404": { "$ref": "#/components/responses/NotFound" }
                }
            }
        },
        "/api/v1/rosters/drafts/{draft_id}/preview": {
            "post": {
                "tags": ["rosters"],
                "summary": "Preview a roster update against current students",
                "parameters": [path_param("draft_id")],
                "requestBody": { "required": true, "content": { "application/json": { "schema": { "$ref": "#/components/schemas/RosterUpdatePreviewRequest" } } } },
                "responses": {
                    "200": { "description": "Roster update preview" },
                    "400": { "$ref": "#/components/responses/InvalidInput" },
                    "404": { "$ref": "#/components/responses/NotFound" }
                }
            }
        },
        "/api/v1/editing/drafts/{draft_id}": {
            "get": {
                "tags": ["editing"],
                "summary": "Fetch the current editor state",
                "parameters": [path_param("draft_id")],
                "responses": {
                    "200": { "description": "EditorState", "content": { "application/json": { "schema": { "$ref": "#/components/schemas/EditorState" } } } },
                    "404": { "$ref": "#/components/responses/NotFound" }
                }
            }
        },
        "/api/v1/editing/drafts/{draft_id}/commands": {
            "post": {
                "tags": ["editing"],
                "summary": "Dispatch an editor command (revision-checked)",
                "parameters": [path_param("draft_id")],
                "requestBody": { "required": true, "content": { "application/json": { "schema": { "$ref": "#/components/schemas/EditorCommandEnvelope" } } } },
                "responses": {
                    "200": { "description": "Updated EditorState", "content": { "application/json": { "schema": { "$ref": "#/components/schemas/EditorState" } } } },
                    "400": { "$ref": "#/components/responses/InvalidInput" },
                    "404": { "$ref": "#/components/responses/NotFound" },
                    "409": { "$ref": "#/components/responses/Conflict" }
                }
            }
        },
        "/api/v1/exports": {
            "post": {
                "tags": ["exports"],
                "summary": "Export a draft in one of 8 formats",
                "requestBody": { "required": true, "content": { "application/json": { "schema": { "$ref": "#/components/schemas/ExportDraftRequest" } } } },
                "responses": {
                    "200": { "description": "Binary artifact (Content-Disposition attachment)", "content": { "application/octet-stream": { "schema": { "type": "string", "format": "binary" } } } },
                    "400": { "$ref": "#/components/responses/InvalidInput" },
                    "404": { "$ref": "#/components/responses/NotFound" },
                    "500": { "$ref": "#/components/responses/InternalError" }
                }
            }
        },
        "/api/v1/layouts/drafts": {
            "post": {
                "tags": ["layouts"],
                "summary": "Create a layout draft (rows/columns or template)",
                "requestBody": { "required": true, "content": { "application/json": { "schema": { "$ref": "#/components/schemas/CreateLayoutDraftRequest" } } } },
                "responses": {
                    "200": { "description": "LayoutStateResponse" },
                    "400": { "$ref": "#/components/responses/InvalidInput" },
                    "422": { "$ref": "#/components/responses/Unprocessable" }
                }
            }
        },
        "/api/v1/layouts/drafts/{draft_id}": {
            "get": {
                "tags": ["layouts"],
                "summary": "Fetch a layout draft",
                "parameters": [path_param("draft_id")],
                "responses": {
                    "200": { "description": "LayoutStateResponse" },
                    "404": { "$ref": "#/components/responses/NotFound" }
                }
            },
            "delete": {
                "tags": ["layouts"],
                "summary": "Delete a layout draft",
                "parameters": [path_param("draft_id")],
                "responses": {
                    "204": { "description": "Deleted" },
                    "404": { "$ref": "#/components/responses/NotFound" }
                }
            }
        },
        "/api/v1/layouts/drafts/{draft_id}/commands": {
            "post": {
                "tags": ["layouts"],
                "summary": "Dispatch a layout command (revision-checked)",
                "parameters": [path_param("draft_id")],
                "requestBody": { "required": true, "content": { "application/json": { "schema": { "$ref": "#/components/schemas/LayoutCommand" } } } },
                "responses": {
                    "200": { "description": "Updated LayoutStateResponse" },
                    "400": { "$ref": "#/components/responses/InvalidInput" },
                    "404": { "$ref": "#/components/responses/NotFound" },
                    "409": { "$ref": "#/components/responses/Conflict" }
                }
            }
        },
        "/api/v1/layouts/drafts/{draft_id}/compiled": {
            "get": {
                "tags": ["layouts"],
                "summary": "Compile a layout draft into a solvable ClassroomLayout",
                "parameters": [path_param("draft_id")],
                "responses": {
                    "200": { "description": "CompiledLayoutResponse" },
                    "404": { "$ref": "#/components/responses/NotFound" },
                    "422": { "$ref": "#/components/responses/Unprocessable" }
                }
            }
        },
        "/api/v1/projects/recent": {
            "get": {
                "tags": ["projects"],
                "summary": "List recent projects",
                "parameters": [
                    { "name": "root", "in": "query", "required": false, "schema": { "type": "string" } },
                    { "name": "limit", "in": "query", "required": false, "schema": { "type": "integer", "default": 20 } }
                ],
                "responses": {
                    "200": { "description": "Recent project list" },
                    "422": { "$ref": "#/components/responses/Unprocessable" }
                }
            }
        },
        "/api/v1/projects/history": { "post": project_op("history", "Project file history") },
        "/api/v1/projects/privacy": { "post": project_op("privacy", "Scan a project for sensitive fields") },
        "/api/v1/projects/bundle": {
            "post": {
                "tags": ["projects"],
                "summary": "Pack a project into a .seattrellis.zip bundle",
                "requestBody": project_body(),
                "responses": {
                    "200": { "description": "Zip bundle (Content-Disposition attachment)", "content": { "application/zip": { "schema": { "type": "string", "format": "binary" } } } },
                    "400": { "$ref": "#/components/responses/InvalidInput" },
                    "404": { "$ref": "#/components/responses/NotFound" },
                    "422": { "$ref": "#/components/responses/Unprocessable" }
                }
            }
        },
        "/api/v1/projects/restore": {
            "post": {
                "tags": ["projects"],
                "summary": "Restore a project from an uploaded bundle (multipart `bundle` + `output_dir`)",
                "requestBody": { "required": true, "content": { "multipart/form-data": { "schema": { "type": "object", "properties": {
                    "bundle": { "type": "string", "format": "binary" },
                    "output_dir": { "type": "string" },
                    "overwrite": { "type": "string", "description": "1/true/yes/on" }
                }, "required": ["bundle", "output_dir"] } } } },
                "responses": {
                    "200": { "description": "Restored project info" },
                    "400": { "$ref": "#/components/responses/InvalidInput" },
                    "422": { "$ref": "#/components/responses/Unprocessable" }
                }
            }
        },
        "/api/v1/projects/migration/preview": { "post": project_op("migration preview", "Dry-run artifact migration") },
        "/api/v1/projects/migration/apply": { "post": project_op("migration apply", "Migrate an artifact (backup + atomic write)") },
        "/api/v1/projects/migration/reference-checks": { "post": project_op("reference checks", "Cross-artifact reference checks") },
        "/api/v1/projects/migration/batch/preview": {
            "post": {
                "tags": ["projects"],
                "summary": "Batch migration preview",
                "requestBody": { "required": true, "content": { "application/json": { "schema": { "type": "object", "properties": { "project_paths": { "type": "array", "items": { "type": "string" } }, "in_place": { "type": "boolean" } }, "required": ["project_paths"] } } } },
                "responses": { "200": { "description": "Batch preview report" }, "422": { "$ref": "#/components/responses/Unprocessable" } }
            }
        },
        "/api/v1/projects/migration/batch/apply": {
            "post": {
                "tags": ["projects"],
                "summary": "Batch migration apply",
                "requestBody": { "required": true, "content": { "application/json": { "schema": { "type": "object", "properties": { "project_paths": { "type": "array", "items": { "type": "string" } }, "in_place": { "type": "boolean" } }, "required": ["project_paths"] } } } },
                "responses": { "200": { "description": "Batch apply report" }, "422": { "$ref": "#/components/responses/Unprocessable" } }
            }
        },
        "/api/v1/projects/migration/restore": {
            "post": {
                "tags": ["projects"],
                "summary": "Restore a migration backup",
                "requestBody": { "required": true, "content": { "application/json": { "schema": { "type": "object", "properties": { "project_path": { "type": "string" }, "source_path": { "type": "string" }, "backup_path": { "type": "string" } }, "required": ["project_path"] } } } },
                "responses": { "200": { "description": "Restore result" }, "422": { "$ref": "#/components/responses/Unprocessable" } }
            }
        },
        "/api/v1/projects/rotation/save": {
            "post": {
                "tags": ["projects"],
                "summary": "Save a rotation plan",
                "requestBody": { "required": true, "content": { "application/json": { "schema": { "type": "object", "properties": {
                    "project_path": { "type": "string" }, "rotation_plan": { "type": "object" },
                    "draft_ids": { "type": "array", "items": { "type": "string" } }, "output_name": { "type": "string" }
                }, "required": ["project_path", "rotation_plan"] } } } },
                "responses": { "200": { "description": "Rotation save response" }, "400": { "$ref": "#/components/responses/InvalidInput" } }
            }
        },
        "/api/v1/projects/rotation/load": { "post": project_op("rotation load", "Load a rotation plan") },
        "/api/v1/projects/rotation/group-register": {
            "post": {
                "tags": ["projects"],
                "summary": "Download the group register (html or csv)",
                "requestBody": { "required": true, "content": { "application/json": { "schema": { "type": "object", "properties": {
                    "project_path": { "type": "string" }, "artifact_path": { "type": "string" },
                    "format": { "type": "string", "enum": ["html", "csv"] }, "locale": { "type": "string" }
                }, "required": ["project_path", "format"] } } } },
                "responses": {
                    "200": { "description": "Group register file" },
                    "400": { "description": "Unknown format" },
                    "404": { "$ref": "#/components/responses/NotFound" }
                }
            }
        },
        "/api/v1/projects/rotation/group-register/preview": { "post": project_op("group register preview", "Preview period groups") },
        "/api/v1/projects/rotation/group-register/save": {
            "post": {
                "tags": ["projects"],
                "summary": "Save group assignments (JSON or multipart)",
                "requestBody": { "required": true, "content": { "application/json": { "schema": { "type": "object", "properties": { "project_path": { "type": "string" }, "groups": { "type": "object" } }, "required": ["project_path"] } } } },
                "responses": { "200": { "description": "Save result" }, "400": { "$ref": "#/components/responses/InvalidInput" } }
            }
        },
        // Documented gaps: the React client calls these, the Rust server 404s
        // them today (Python-only; see docs/v2-parity-ledger.md §3.2).
        "/api/v1/classes/rotation": {
            "post": {
                "tags": ["classes"],
                "summary": "Generate a multi-period rotation plan",
                "x-implemented": false,
                "description": "NOT IMPLEMENTED in the Rust server (Python-only). Rotation \
    generation is M4-04 work.",
                "responses": { "404": { "$ref": "#/components/responses/NotFound" } }
            }
        },
        "/api/v1/projects/artifacts/compare": {
            "post": {
                "tags": ["projects"],
                "summary": "Compare two project artifacts",
                "x-implemented": false,
                "description": "NOT IMPLEMENTED in the Rust server (Python-only).",
                "responses": { "404": { "$ref": "#/components/responses/NotFound" } }
            }
        },
        "/api/v1/projects/artifacts/restore": {
            "post": {
                "tags": ["projects"],
                "summary": "Restore a project artifact",
                "x-implemented": false,
                "description": "NOT IMPLEMENTED in the Rust server (Python-only).",
                "responses": { "404": { "$ref": "#/components/responses/NotFound" } }
            }
        }
    })
}

fn path_param(name: &str) -> Value {
    json!({ "name": name, "in": "path", "required": true, "schema": { "type": "string" } })
}

fn project_body() -> Value {
    json!({
        "required": true,
        "content": { "application/json": { "schema": {
            "type": "object",
            "properties": { "project_path": { "type": "string" }, "include_outputs": { "type": "boolean" } },
            "required": ["project_path"]
        } } }
    })
}

fn project_op(summary: &str, description: &str) -> Value {
    json!({
        "tags": ["projects"],
        "summary": summary,
        "description": description,
        "requestBody": project_body(),
        "responses": {
            "200": { "description": "Result document" },
            "400": { "$ref": "#/components/responses/InvalidInput" },
            "404": { "$ref": "#/components/responses/NotFound" },
            "422": { "$ref": "#/components/responses/Unprocessable" },
            "500": { "$ref": "#/components/responses/InternalError" }
        }
    })
}

fn schemas() -> Value {
    json!({
        "SessionResponse": {
            "type": "object",
            "required": ["api_version", "session_token"],
            "properties": {
                "api_version": { "type": "integer" },
                "session_token": { "type": "string", "description": "256-bit hex token" }
            },
            "additionalProperties": false
        },
        "HealthResponse": {
            "type": "object",
            "required": ["api_version", "service", "status"],
            "properties": {
                "api_version": { "type": "string" },
                "service": { "type": "string" },
                "status": { "type": "string" }
            },
            "additionalProperties": false
        },
        "CoreSolveRequest": {
            "type": "object",
            "required": ["api_version", "student_count", "seat_positions"],
            "properties": {
                "api_version": { "type": "integer" },
                "student_count": { "type": "integer" },
                "seat_positions": { "type": "array", "items": { "type": "array", "items": { "type": "number" }, "minItems": 2, "maxItems": 2 } },
                "edges": { "type": "array", "items": { "type": "array", "items": { "type": "integer" }, "minItems": 2, "maxItems": 2 } },
                "fixed_seats": { "type": "array", "items": { "type": "array", "items": { "type": "integer" }, "minItems": 2, "maxItems": 2 } },
                "must_be_adjacent": { "type": "array", "items": { "type": "array", "items": { "type": "integer" }, "minItems": 2, "maxItems": 2 } },
                "cannot_be_adjacent": { "type": "array", "items": { "type": "array", "items": { "type": "integer" }, "minItems": 2, "maxItems": 2 } },
                "min_distance": { "type": "array", "items": { "type": "object" } },
                "seed": { "type": "integer" },
                "students": { "type": "array", "items": { "type": "object" } },
                "rules": { "type": "object" },
                "layout": { "type": "object" },
                "history": { "type": "object" },
                "pair_history": { "type": "object" }
            },
            "additionalProperties": true
        },
        "GenerateClassRequest": {
            "type": "object",
            "required": ["draft"],
            "properties": {
                "draft": {
                    "type": "object",
                    "required": ["students", "room", "goal"],
                    "properties": {
                        "students": { "type": "array", "items": { "type": "object" } },
                        "room": { "type": "object", "properties": { "template_id": { "type": "string" }, "layout": { "type": "object" } } },
                        "goal": { "type": "object", "properties": { "goal_id": { "type": "string" }, "rules_overlay": { "type": "object" }, "hard_rules": { "type": "object" }, "custom": { "type": "object" } } },
                        "history_snapshots": { "type": "array", "items": { "type": "object" } }
                    }
                },
                "options": { "type": "object", "properties": { "seed": { "type": "integer" }, "candidate_count": { "type": "integer" } } }
            },
            "additionalProperties": true
        },
        "GenerateClassResponse": {
            "type": "object",
            "required": ["class_name", "goal", "warnings", "recommended_candidate_id", "candidates", "editor"],
            "properties": {
                "class_name": { "type": "string" },
                "goal": { "type": "string" },
                "warnings": { "type": "array", "items": { "type": "string" } },
                "recommended_candidate_id": { "type": "string" },
                "candidates": { "type": "array", "items": {
                    "type": "object",
                    "required": ["candidate_id", "recommended", "total_score"],
                    "properties": {
                        "candidate_id": { "type": "string" },
                        "recommended": { "type": "boolean" },
                        "total_score": { "type": "number" }
                    },
                    "additionalProperties": true
                } },
                "editor": { "$ref": "#/components/schemas/EditorState" }
            },
            "additionalProperties": true
        },
        "PlanNotFound": {
            "type": "object",
            "required": ["error", "status", "message"],
            "properties": {
                "error": { "type": "string", "const": "plan_not_found" },
                "status": { "type": "string", "enum": ["Solved", "ProvenInfeasible", "Timeout", "Unknown", "InvalidInput", "Cancelled", "InternalError"] },
                "message": { "type": "string" }
            },
            "additionalProperties": false
        },
        "EditorState": {
            "type": "object",
            "required": ["kind", "protocol_version", "draft_id", "revision", "undo_depth", "redo_depth", "students", "seats"],
            "properties": {
                "kind": { "type": "string" },
                "protocol_version": { "type": "string" },
                "draft_id": { "type": "string" },
                "revision": { "type": "integer" },
                "candidate_id": { "type": "string" },
                "undo_depth": { "type": "integer" },
                "redo_depth": { "type": "integer" },
                "students": { "type": "array", "items": { "type": "object", "required": ["student_key", "display_name", "locked"], "properties": {
                    "student_key": { "type": "string" }, "display_name": { "type": "string" },
                    "seat_id": { "type": "string" }, "locked": { "type": "boolean" }
                }, "additionalProperties": true } },
                "seats": { "type": "array", "items": { "type": "object" } }
            },
            "additionalProperties": true
        },
        "EditorCommandEnvelope": {
            "type": "object",
            "required": ["kind", "protocol_version", "command_id", "draft_id", "base_revision", "action"],
            "properties": {
                "kind": { "type": "string" },
                "protocol_version": { "type": "string" },
                "command_id": { "type": "string" },
                "draft_id": { "type": "string" },
                "base_revision": { "type": "integer" },
                "action": { "type": "string" },
                "operations": { "type": "array", "items": { "type": "object" } }
            },
            "additionalProperties": true
        },
        "ExportDraftRequest": {
            "type": "object",
            "required": ["draft_id", "format"],
            "properties": {
                "draft_id": { "type": "string" },
                "format": { "type": "string", "enum": ["svg", "html", "print-html", "png", "pdf", "excel", "docx", "pptx"] },
                "template": { "type": "string" },
                "privacy": { "type": "string" },
                "orientation": { "type": "string" },
                "page_scale": { "type": "number" },
                "locale": { "type": "string" },
                "show_student_ids": { "type": "boolean" }
            },
            "additionalProperties": true
        },
        "RosterDraftResponse": {
            "type": "object",
            "required": ["draft_id", "source_format", "headerless", "row_count", "column_count", "columns", "preview_rows", "suggested_mapping", "mapping_issues"],
            "properties": {
                "draft_id": { "type": "string" },
                "source_format": { "type": "string" },
                "headerless": { "type": "boolean" },
                "row_count": { "type": "integer" },
                "column_count": { "type": "integer" },
                "columns": { "type": "array", "items": { "type": "object" } },
                "preview_rows": { "type": "array", "items": { "type": "object" } },
                "suggested_mapping": { "type": "array", "items": { "type": "object" } },
                "mapping_issues": { "type": "array", "items": { "type": "object" } }
            },
            "additionalProperties": true
        },
        "RosterUpdatePreviewRequest": {
            "type": "object",
            "required": ["mapping", "mode", "current_students", "current_revision", "updated_fields"],
            "properties": {
                "mapping": { "type": "array", "items": { "type": "object" } },
                "mode": { "type": "string" },
                "current_students": { "type": "array", "items": { "type": "object" } },
                "current_revision": { "type": "integer" },
                "updated_fields": { "type": "array", "items": { "type": "string" } }
            },
            "additionalProperties": true
        },
        "CreateLayoutDraftRequest": {
            "type": "object",
            "properties": {
                "name": { "type": "string" },
                "rows": { "type": "integer" },
                "columns": { "type": "integer" },
                "template_id": { "type": "string" }
            },
            "additionalProperties": true
        },
        "LayoutCommand": {
            "type": "object",
            "required": ["command_id", "draft_id", "base_revision", "action"],
            "properties": {
                "command_id": { "type": "string" },
                "draft_id": { "type": "string" },
                "base_revision": { "type": "integer" },
                "action": { "type": "string" },
                "operation": { "type": "object" }
            },
            "additionalProperties": true
        },
        "ErrorEnvelope": {
            "type": "object",
            "required": ["error"],
            "properties": {
                "error": { "type": "string" },
                "status": { "type": "string" },
                "message": { "type": "string" }
            },
            "additionalProperties": true
        }
    })
}

/// The response components referenced across paths.
pub fn responses() -> Value {
    json!({
        "InvalidHost": {
            "description": "Host header is not the loopback address the server is bound to (DNS rebinding guard)",
            "content": { "application/json": { "schema": { "$ref": "#/components/schemas/ErrorEnvelope" } } }
        },
        "InvalidInput": {
            "description": "Malformed request or validation failure",
            "content": { "application/json": { "schema": { "$ref": "#/components/schemas/ErrorEnvelope" } } }
        },
        "NotFound": {
            "description": "Unknown draft, project or artifact",
            "content": { "application/json": { "schema": { "$ref": "#/components/schemas/ErrorEnvelope" } } }
        },
        "Conflict": {
            "description": "Revision conflict, stale base, or already-applied command",
            "content": { "application/json": { "schema": { "$ref": "#/components/schemas/ErrorEnvelope" } } }
        },
        "Unprocessable": {
            "description": "Semantic validation failure (422)",
            "content": { "application/json": { "schema": { "$ref": "#/components/schemas/ErrorEnvelope" } } }
        },
        "InternalError": {
            "description": "Internal fault",
            "content": { "application/json": { "schema": { "$ref": "#/components/schemas/ErrorEnvelope" } } }
        }
    })
}
