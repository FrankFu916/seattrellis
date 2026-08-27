# Web Workbench Guide

[English](web.md) / [简体中文](web.zh.md)

**SeatTrellis v2.0.0 is released.** The web workbench is a local React
application served by the pure-Rust `seattrellis_app` server.

## Start the workbench

The server binds to the loopback address only (default `127.0.0.1:8765`),
generates a 256-bit session token at startup, and opens the workbench:

```bash
seattrellis_app --open-browser
# or, from a source checkout:
cargo run -p seattrellis_app -- --open-browser
```

The desktop Tauri shell starts the same server and loads the workbench in a
native window. During development, `SEATTRELLIS_WEB_STATIC` overrides the
embedded frontend assets. Do not expose this service to a LAN or an untrusted
network.

## Teacher workflow

The workbench is the default entry point for ordinary classroom use. It covers
roster import and mapping, inline student editing, room templates, custom rows
and columns, aisles and unavailable seats, common seating goals, combined
preferences, adjacency and fixed-seat requests, generation, visual classroom
editing, swaps, undo/redo, export, and class-project backups. **Advanced
settings** can import and download complete rules/layout JSON and historical
snapshots. **Detailed seating rules** exposes the implemented history, neighbor,
cooling, score, and peer-support objectives as form controls.

1. Enter a class name and import a CSV, XLSX, or XLSM roster. Headerless input
   preserves its first data row and asks you to confirm the name or ID column.
   A name column is enough to begin; records can also be edited directly.
2. Accept the recommended 30-, 48-, or 60-seat room, or define custom rows,
   seats per row, aisles, and unavailable seats.
3. Choose Daily rotation, Quick shuffle, Fair shuffle, or Peer support, combine
   preferences, and add keep-apart, keep-together, fixed-seat,
   minimum-distance, or named-group requests.
4. Review the recommended map, then swap, move, lock, undo, or redo as needed.
5. Select a public handout, teacher copy, or plan report, review privacy fields
   and page settings, then preview and download.

The sidebar language switch changes interface text between Simplified Chinese and
English. It does not clear loaded data, the current step, or solve results. The
workbench retains parsed data rather than the original upload bytes; **Start
over and clear student list** clears only the teacher workspace.

Before generation, the page explains missing optional history, score, height, or
vision information. Quick Shuffle remains available for a names-only roster.

The Generate step can create several future rotation periods. Enter a count and
optional labels separated by commas or new lines. Each period has an independent
editing draft; selecting a period loads it into the normal editing and export
flow while the summary shows repeated-neighbor metrics.

### Detailed seating rules

Open **Detailed seating rules** when the common preference cards are not precise
enough. The panel configures historical lookback, recent-neighbor and cooling
relation types/distance, high-score front/back placement, row or group score
distribution, and mentor/learner percentiles. Weights are soft objectives, so a
request such as keeping two students apart remains absolute. Group score
balancing requires `group_id` on layout seats. Raw rules JSON remains available
for compatibility.

## Project panel

The workbench's Project panel finds `*.project.json` and `*.seattrellis.json`
files under a local folder. Selecting a class shows history and generated-file
metadata only; student records and scores are not returned as part of this view.

The panel can scan sensitive fields, compare history or output artifacts, create
a current-plan snapshot, download a `.seattrellis.zip` backup, and restore an
uploaded bundle to a local folder. Comparisons return counts plus anonymous
student references and before/after seat IDs; names and scores do not enter the
browser response. Recovery writes a new output and never overwrites the selected
history artifact.

The **Project format migration** area validates the selected project or artifact
against the current schema. A normal write creates a sibling `*.migrated.json`
file. An explicit in-place option replaces the source only after creating a
`.bak` backup. The same path-safety and manifest checks used by the CLI apply to
browser uploads.

The classroom editor supports clicking cells to create seats, aisles, platforms,
or empty space, changing the grid, and saving the result for generation. The
rules editor covers the four hard-rule lists, active soft objectives, and named
groups while retaining the raw JSON compatibility field. Custom rules report
field-level errors before generation. Multiple history snapshots can be loaded
for fair rotation and recent-neighbor calculations.

When a class project is selected after generating a rotation, **Save current
rotation** writes every period's seats, locks, and editing commands as a new
rotation-plan output. **Continue a rotation** reloads an existing plan into
period drafts without replacing its source.

Saved rotation plans can produce a printable HTML or CSV group register. Each
period retains empty groups, unseated students, and members missing from the
current roster. The membership preview shows group sizes, seated/unseated
counts, and additions/removals between adjacent periods without returning names
or IDs to the browser.

## Rules preview and history quality

After **Settings & Solve**, the page shows the complete merged `RuleSet` from the
preset and overlay. Review hard rules, weights, and seed before solving, and
download the merged JSON for records.

After uploading history, the quality check reports current student coverage,
missing or extra students, unknown or disabled seats, and whether the snapshot
layout matches the current layout. Demo mode loads fictional history from
`examples/history/`.

## Project path mode

Entering a local project path supports reading configuration, validating
referenced files, solving candidates, and exporting. An uploaded project JSON
contains only the manifest; the browser cannot access the files it references,
so upload mode validates and displays configuration but does not enable solve or
export. Path mode and the Project panel are local filesystem features.

## Settings, results, and export

**Download current web config** saves the preset, rules overlay, candidate count,
seed, and time limit. It does not contain the roster, layout, history, paths, or
results. Rules may still reference student IDs, so treat such settings as
sensitive. Restoring settings requires loading the data files again.

Candidate results can be previewed side by side and compared by total score,
hard constraints, and score dimensions. The seating map and assignment table
follow the selected candidate.

The page offers public, teacher-internal, and candidate-explanation templates,
with anonymization, field hiding, A4 orientation, scaling, and Chinese or
English content. Safe template defaults can be tightened but not loosened.

The workbench can download snapshot or candidate-set JSON, plan reports, HTML,
print-friendly HTML, PDF, PNG, Excel, and Word files. All formats are rendered
locally by Rust; no optional conversion install is required.

## Lock, repair, and manual adjustment

**Lock & repair** keeps selected students or seats fixed and optionally bounds
the set of students that may move. An empty affected-student selection performs a
global re-solve while preserving locks. The resulting snapshot records lock
state, repair provenance, and changed students.

**Manual adjustment** supports swapping students, moving to an empty seat,
unseating, and reseating. It never silently displaces a third student. Every
change immediately reevaluates hard constraints; the current draft can be
undone/redone, exported, or passed to Lock & repair. Manual edits do not rewrite
the source candidate set or rules file.

The Locks area can lock or unlock any seated student or enabled seat. Locked
students and seats are unavailable in move mode, and lock operations participate
in undo/redo. Batch move pairs selected students and target seats in selection
order, previews the mapping, and records one atomic operation.

The interactive seating map supports direct clicks. In **Move / swap** mode,
click an occupied source and then an empty or occupied target. In **Lock / unlock
seats** mode, click an enabled seat to toggle its lock. All map actions use the
shared command log and hard-constraint diagnostics.

## Privacy

Solving happens on the local computer. Registered teacher working files are
removed when their plan is replaced or cleared; remaining working directories
are cleaned up when the process exits. Project path mode accesses the path
entered by the user, so do not expose the service to untrusted users. Do not
commit real student data, screenshots, or exports.

## Accessibility and small screens

- Tab reaches upload, selection, solve, and download controls in order, with a
  visible focus outline.
- A skip link at the start of the page moves directly to the main content.
- Enabled seats are keyboard-focusable and expose seat, student, and location
  details to assistive technology.
- Side-by-side controls stack on narrow screens, and buttons keep a touch target
  of at least 44 pixels.
- Non-essential motion is disabled when the operating system requests reduced
  motion.

## Current limitations

- Layout editing uses clicks and toolbar actions; drag-and-drop and box selection
  are not available yet.
- History comparison and restoring a snapshot are supported; per-plan visual
  diffing is still being refined.
- The interface supports Simplified Chinese and English.

## Related documents

- [Quick start](quickstart.md)
- [Project workflow](project.md)
- [Export formats](export.md)
