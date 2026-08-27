# Editor Protocol

**SeatTrellis v2.0.0 is released.** The current editor protocol version is
`"1.0"`.

The editor protocol is the transport boundary between the React workbench, the
loopback `seattrellis_app` server, and the desktop shell. Rust
(`seattrellis-domain::editing`) enforces domain rules; clients submit commands
and render the minimal state returned by the server. The CLI `edit` and `repair`
commands reuse the same editing semantics.

The current protocol version is `"1.0"` and exposes two documents:

- `EditorCommandEnvelope`: an apply, undo, or redo command from a client;
- `EditorStateEnvelope`: the current seats and lock state returned by the server.
  A command response also carries a separate `validation` object registered in
  `schemas/editor-state.schema.json`; the state `GET` endpoint does not include
  that object.

The JSON Schemas are `schemas/editor-command.schema.json` and
`schemas/editor-state.schema.json`.

## Command format

Every command explicitly carries its type, protocol version, command ID, draft
ID, and base revision:

```json
{
  "kind": "seattrellis_editor_command",
  "protocol_version": "1.0",
  "command_id": "move-20260718-001",
  "draft_id": "7b7359c6f9cd4e128df8b9145d012ec1",
  "base_revision": 3,
  "action": "apply",
  "operations": [
    {
      "kind": "swap_students",
      "payload": {
        "first_student": "S001",
        "second_student": "S018"
      }
    }
  ]
}
```

An `action` of `"undo"` or `"redo"` must not include `operations`:

```json
{
  "kind": "seattrellis_editor_command",
  "protocol_version": "1.0",
  "command_id": "undo-20260718-001",
  "draft_id": "7b7359c6f9cd4e128df8b9145d012ec1",
  "base_revision": 4,
  "action": "undo"
}
```

Supported operations are:

| Kind | Payload |
| --- | --- |
| `swap_students` | `first_student`, `second_student` |
| `move_student` | `student_key`, `seat_id` |
| `batch_move` | `moves: [{student_key, seat_id}]` |
| `seat_student` | `student_key`, `seat_id` |
| `unseat_student` | `student_key` |
| `lock_student` / `unlock_student` | `student_key` |
| `lock_seat` / `unlock_seat` | `seat_id` |

One command expands to at most 100 operations. Each mapping in `batch_move`
counts as one operation, and students and target seats must each be unique. The
server validates and replays the complete command before writing the draft; a
failure never commits a partial result.

## Revisions and conflicts

A new draft receives a non-reusable `draft_id` and starts at revision 0. Each
successful apply, undo, or redo increments revision exactly once, even when an
apply contains several operations. Undo and redo operate on whole command
batches.

Before writing, the server checks:

1. whether `draft_id` belongs to the current draft;
2. whether `command_id` has already been processed;
3. whether `base_revision` equals the current revision.

Any failure raises `EditorProtocolConflictError` without changing the draft or
output files. After a conflict, a client must fetch the latest
`EditorStateEnvelope` and construct a new command from the user's intent; it
must not silently overwrite newer state.

## State format

State contains only what the editor needs:

- student key, display name, current seat, and lock state;
- seat key, row, column, enabled state, current student key, and lock state;
- undo and redo depths.

Hard-constraint results are not part of the state itself. Apply/undo/redo
responses carry `validation` (`valid`, `hard_constraints_satisfied`, and
`violations`), while `GET .../editing/drafts/{id}` returns state without it.

Scores, notes, special needs, height, vision, tags, and extension attributes do
not enter the state protocol. Seats do not repeat student names; clients join
them to the student list through `student_key`.

This is data minimization, not anonymization. Names, stable student keys, and
constraint diagnostics may still identify students. Do not send state or
commands to remote telemetry or public logs. Escape diagnostic strings as
untrusted plain text and do not treat them as stable machine-readable codes.

`draft_id` is only a concurrency identifier, not an authorization token. Any
future HTTP or WebSocket exposure must also verify session ownership and provide
CSRF, Origin, and access-control protection.

## Validation boundary

JSON Schema validates field types, required fields, operation shapes, and basic
size limits. These cross-field and domain constraints remain owned by the Rust
server model and editing state machine:

- apply must contain operations, while undo/redo must not;
- the expanded operation count must not exceed 100;
- batch sources and targets must not repeat;
- students, seats, locks, and occupancy relationships must be valid in the
  current draft.

Clients may use the Schema for immediate feedback, but cannot skip server
validation.
