# Web UI Guide

## Starting the workbench

The v2 web workbench is served by the pure-Rust `seattrellis_app` local server.
It binds the loopback address only (default `127.0.0.1:8765`), generates a
256-bit session token at startup, and opens the React workbench in your
browser:

```bash
seattrellis_app --open-browser
# or
cargo run -p seattrellis_app -- --open-browser
```

The desktop app (Tauri shell) starts the same server and loads the workbench in
a native window. During development, `SEATTRELLIS_WEB_STATIC` overrides the
embedded frontend assets. The server listens on the local machine only — do
not expose it to a LAN or an untrusted network.

## React workbench

The React workbench is the default path for ordinary teachers. It covers roster
import and mapping, inline student editing, room templates, custom rows and
columns, aisles and unavailable seats, common seating goals, combined
preferences, adjacency and fixed-seat requests, generation, visual classroom
editing, seat swaps, undo/redo, export, and class-project backups. Its
**Advanced settings** section contains solver controls plus import/download
actions for complete rules/layout JSON and historical snapshot files. The
separate **Detailed seating rules** panel exposes the implemented history,
neighbor, cooling, score, and peer-support rules with ordinary form controls.
Common named groups can be configured directly as together/apart requests; the
JSON field remains available for more complex group relationships.

Teacher workspace flow:

1. Enter a class name and import a CSV, XLSX, or XLSM roster. Common headerless
   exports keep their first data row and ask you to confirm the name or ID
   column. A name column is enough to begin; you can also add or correct
   student records directly in the roster editor.
2. Accept the recommended 30-, 48-, or 60-seat room, or set custom rows, seats
   per row, aisle positions, and unavailable seats.
3. Choose Daily rotation, Quick shuffle, or Peer support, combine preferences,
   and add keep-apart, keep-together, fixed-seat, minimum-distance, or named
   group requests.
4. Review the recommended map, then swap, move, lock, undo, or redo as needed.
5. Choose a public handout, teacher copy, or plan report, review the privacy
   fields and A4 scale, then preview and download the result.

The sidebar language switch changes the interface between Simplified Chinese
and English. It does not clear loaded data, the current step, or solve results.
Returning to the teacher workspace restores the parsed roster, room, goal, and
generated plan without retaining the original upload bytes. **Start over and
clear student list** clears only the teacher workspace.

Before generation, the workspace explains which optional history, score,
height, or vision information is unavailable. Quick shuffle remains available
when the roster contains names only.

The Generate step can also create a future rotation. Choose the number of
periods and optionally provide labels separated by commas or new lines. Each
period has its own editing draft; select a period in the summary to load it
into the normal editing and export flow, while the summary lists all periods
and repeated-neighbor metrics.

### Detailed seating rules

Open **Detailed seating rules** on the Generate step when the common preference
cards are not precise enough. The panel can configure historical position
lookback, recent-neighbor and cooling relation types/distance, high-score
front/back placement, row or group score distribution, and mentor/learner
percentiles. Weights are soft objectives, so hard requests such as "keep these
two students apart" still take priority. Group score balancing requires
`group_id` on the layout seats. The raw rules JSON field remains available for
compatibility.

### Project panel

The workbench also shows a Project panel beside the classroom flow. Enter a
local folder and refresh it to find `*.project.json` and `*.seattrellis.json`
files. Selecting a class shows history and generated-file metadata only;
student records and scores are not sent to the browser as part of this view.

The panel can scan the selected project for sensitive fields, compare two
history or output artifacts, create a new current-plan snapshot from one of
them, download a `.seattrellis.zip` backup, and restore an uploaded bundle to a
local folder. Comparison returns counts plus an expandable list of anonymous
student references and before/after seat IDs; student names and scores never
enter the browser response. Recovery writes a new output file and never
overwrites the selected history artifact. The **Project format migration** area
first validates the selected project or artifact against the current schema.
Writing creates a sibling `*.migrated.json` file by default; an explicit
in-place option replaces the source only after creating a `.bak` backup. The
same path-safety and manifest checks used by the CLI apply to browser uploads.

The classroom editor supports clicking cells to create seats, aisles,
platforms, or empty space, changing the grid, and saving the result for
generation. Student editing is available in the roster step. The detailed
rules panel covers the implemented soft rules, and custom RuleSet JSON reports
field-level errors before generation, including unknown fields, malformed hard
rules, and roster or seat references that do not exist. The visual RuleSet
editor covers the four hard-rule lists, active soft rules, and named groups,
while preserving the raw JSON compatibility field. Multiple historical
snapshot files can be loaded for the next solve.

When a rotation has been generated and a class project is selected, the panel
also offers **Save current rotation**. It writes every period's current seats,
locks, and editing commands as a new rotation-plan output without replacing the
source artifact. Existing rotation outputs can be opened with **Continue a
rotation**, which recreates the period drafts so the plan can be adjusted
again.

For a saved rotation plan, the Project panel can also download a group register
as printable HTML or CSV. Each period lists the group, student, seat, and
status, while retaining empty groups, unseated students, and members missing
from the roster. Before downloading, use the membership preview to review group
sizes, seated and unseated counts, and additions/removals between adjacent
periods. The preview uses anonymous references and does not return names or
student IDs to the browser. Migration previews also show privacy-safe field
paths and type changes, before and after validation, and the available backup
or rollback path without returning original student values.

## Rules preview

After entering **Settings & Solve**, the page shows the full merged `RuleSet`
from the preset and any rules overlay. You can review hard rules, weights, and
seed before solving, and download the merged JSON for records.

## History quality check

After uploading history you can run a quality check. The report shows per
snapshot:

- current student coverage;
- missing students and students not in the current roster;
- unknown seats and disabled seats;
- whether the snapshot layout matches the current layout.

The demo auto-loads the fictional history under `examples/history/`.

## Project workspace

Entering a local project path supports reading configuration, validating
referenced files, solving candidate plans, and exporting. An uploaded project
JSON carries only the manifest — the browser cannot reach the referenced
students, layout, rules, or history files — so the page validates and displays
the configuration safely without enabling solve or export. The workbench's
Project panel adds recent-project discovery, history metadata, privacy
scanning, and `.seattrellis.zip` backup/restore for ordinary browser use.

## Saving and restoring settings

**Download current web config** saves the preset, rules overlay, candidate
count, seed, and time limit. It does not contain the student list, layout,
history, paths, or results. Rules such as fixed seats, pair rules, and groups
can still reference student IDs; the page warns when the settings file should
be treated as sensitive. After restoring settings you must load the data files
again.

## Results and export

Multi-candidate results can be previewed side by side and compared in one
table by total score, hard constraints, and score dimensions. The seating map,
score breakdown, and assignment table update with the selected candidate.

The page offers export templates and privacy controls for public,
teacher-internal, and candidate explanation output, plus anonymization, field
hiding, A4 orientation, scaling, and Chinese or English content. Safe template
defaults can be tightened but not loosened.

The page can download snapshot or candidate-set JSON, plan reports, HTML,
print-friendly HTML, PDF, PNG, Excel, and Word files. All formats are rendered
by the local Rust exporters; no optional installs are needed.

## Lock and repair

After solving, expand **Lock & repair** to keep students in their current
seats, lock seats, or select the students that may be rearranged. Leaving the
affected-student selection empty performs a global re-solve while preserving
the locks. Quick solve reuses the history loaded for the current session;
Project workspace uses the project's history directory. The resulting snapshot
records lock state, repair provenance, and the students whose seats changed.

## Manual swaps and undo

After solving, expand **Manual adjustment** to swap two students, move a
student to an empty seat, move a student to the unseated area, or place an
unseated student in an empty seat. Move targets are limited to empty seats, so
the interface never silently unseats a third student. Every change immediately
reevaluates the current `RuleSet` hard constraints. The result area reports
whether they pass and how many violations remain. Changes can be undone and
redone in order. The current draft can be exported directly or passed to
**Lock & repair** for constrained re-solving.

Manual edits do not overwrite the source candidate set or rules file. The
output snapshot records the operation log, locks, unseated students, and
constraint summary in `metadata.manual_edit`.

The **Locks** area can lock or unlock any seated student or enabled seat.
Locked students are removed from move choices, locked seats are not offered as
targets, and occupants of locked seats cannot be swapped or unseated. Lock and
unlock commands participate in undo/redo and are saved in
`metadata.lock_state`. **Lock & repair** reuses these saved locks by default.

**Batch move** pairs selected students and target seats in selection order and
shows the mapping before execution. Counts must match. Current seats of the
selected students can be targets for a rotation, but the batch cannot displace
anyone outside it. The whole batch creates one operation record and is restored
by one undo.

The **Interactive seating map** supports direct seat clicks. In **Move / swap**
mode, click an occupied source and then an empty target to move, or another
occupied seat to swap. Click the source again to cancel. In **Lock / unlock
seats** mode, one click toggles the seat lock. Locked students and seats are
disabled in move mode. Every map action still uses the shared command log and
hard-constraint diagnostics.

## Privacy

Solving happens on the local computer. Registered teacher working files are
removed when their plan is replaced or cleared; remaining working directories
are cleaned up when the process exits. Project path mode accesses the local
path entered by the user — do not expose this service to untrusted network
users. Do not commit real student data, screenshots, or exports to a public
repository.

## Keyboard and small screens

- Tab reaches upload, selection, solve, and download controls in order, with a
  visible focus outline.
- A skip link at the start of the page moves directly to the main content.
- Enabled seats in the seating map are keyboard-focusable and expose seat,
  student, and location details to assistive technology.
- Side-by-side controls stack vertically on narrow screens. Buttons keep a
  touch target of at least 44 pixels.
- Non-essential motion is disabled when the operating system requests reduced
  motion.

## Current limitations

- The classroom editor uses click and toolbar actions; drag-and-drop or
  box-selection layout editing is not available yet.
- History comparison and creating a new snapshot from history are supported;
  per-plan visual diffing is still being refined.
- Seat-map clicking and form controls are available; box selection and
  drag-and-drop remain future work.
- The interface currently supports Simplified Chinese and English.

## Related documents

- [Quick start](quickstart.en.md)
- [Project workflow details](project.zh.md)
- [Export formats](export.zh.md)
