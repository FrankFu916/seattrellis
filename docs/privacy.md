# Privacy

SeatTrellis v2.0.0 is local-first. The desktop app, browser workbench, and CLI
process data on the user's machine. There are no accounts, cloud sync, or
product telemetry.

## Data boundary

- Do not commit real student rosters, scores, notes, special needs, or history
  snapshots to a public repository.
- Keep `outputs/`, `exports/`, `snapshots/`, `private/`, and `data/` in ignored
  private directories.
- A project file stores paths and defaults; it does not embed the files it
  references.
- Remove names, IDs, school details, and other identifiers before sharing a
  screenshot, issue, log, or export.
- `examples/` contains fictional students and classrooms only.

## Public and teacher exports

The `teacher` template is for controlled internal use and may retain real names,
student IDs, and explicitly enabled detail fields. The `public` template is
fail-closed: it anonymizes student labels and suppresses student IDs, scores,
notes, special needs, height, vision, and other identifying details. Export
options cannot loosen the public safety boundary.

See [Export formats](export.md) and [Font strategy](font-strategy.md) for
rendering and sharing details.

## Local server boundary

`seattrellis_web` binds to `127.0.0.1` by default and requires a per-process
session token for API requests. Do not expose it to a LAN or an untrusted
network. Project path mode reads the local path explicitly entered by the user;
an uploaded project manifest alone does not grant the browser access to its
referenced files.
