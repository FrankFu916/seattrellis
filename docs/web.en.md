# Web UI Guide

## Recommended entry point: React workbench

```bash
python -m pip install -e ".[web,excel,image]"
seattrellis workspace
```

The React workbench is the default path for ordinary teachers. It covers roster
import and mapping, inline student editing, room templates, custom rows and columns, aisles and
unavailable seats, common seating goals, combined preferences, adjacency and
fixed-seat requests, generation, visual classroom editing, seat swaps, undo/redo,
export, and class-project backups. Its **Advanced settings** section contains
solver controls plus import/download actions for complete rules/layout JSON and
historical snapshot files. The separate **Detailed seating rules**
panel exposes the implemented history, neighbor, cooling, score, and peer-support
rules with ordinary form controls. Common named groups can be configured directly
as together/apart requests; the JSON field remains available for more complex
group relationships and is compiled as hard pair constraints.

## Streamlit compatibility and advanced tools

```bash
python -m pip install -e ".[web,excel,image,pdf,docx]"
streamlit run src/seattrellis/web/app.py --server.address 127.0.0.1
```

The rest of this page primarily documents the Streamlit compatibility surface.
It still exposes presets, rules overlays, history directories, candidate count,
seed, time limits, backend selection, and detailed export privacy controls.
Existing JSON, Project, and CLI workflows remain valid.

The sidebar language switch changes the interface between Simplified Chinese
and English. It does not clear loaded data, the current step, or solve results.

## Teacher workspace

The default workspace keeps the ordinary path focused on classroom tasks:

1. Enter a class name and import a CSV, XLSX, or XLSM roster. Common headerless
   exports keep their first data row and ask you to confirm the name or ID column.
   A name column is enough to begin; you can also add or correct student records
   directly in the roster editor.
2. Accept the recommended 30-, 48-, or 60-seat room, or set custom rows, seats
   per row, aisle positions, and unavailable seats.
3. Choose Daily rotation, Quick shuffle, or Peer support, combine preferences,
   and add keep-apart, keep-together, fixed-seat, minimum-distance, or named
   group requests.
4. Review the recommended map, then swap, move, lock, undo, or redo as needed.
5. Choose a public handout, teacher copy, or plan report, review the privacy
   fields and A4 scale, then preview and download the result.

Visiting Advanced tools and returning restores the parsed roster, room, goal,
and generated plan without retaining the original upload bytes. **Start over
and clear student list** clears only the teacher workspace.

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
lookback, recent-neighbor and cooling relation types/distance, high-score front/back
placement, row or group score distribution, and mentor/learner percentiles.
Weights are soft objectives, so hard requests such as “keep these two students
apart” still take priority. Group score balancing requires `group_id` on the
layout seats. The raw rules JSON field remains available for compatibility, while
the detailed panel covers the active cooling objective as well.

### React workbench project panel

`seattrellis workspace` also shows a Project panel beside the classroom flow.
Enter a local folder and refresh it to find `*.project.json` and
`*.seattrellis.json` files. Selecting a class shows history and generated-file
metadata only; student records and scores are not sent to the browser as part
of this view.

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
The classroom editor supports clicking cells to create seats, aisles, platforms,
or empty space, changing the grid, moving or mirroring the layout, and saving
the result for generation. Student editing is available in the roster step. The
detailed rules panel covers the implemented soft rules, and custom RuleSet JSON
now reports field-level errors before generation, including unknown fields,
malformed hard rules, and roster or seat references that do not exist. The visual
RuleSet editor covers the four hard-rule lists, active soft rules, and named groups,
while preserving the raw JSON compatibility field. Multiple historical snapshot
files can be loaded for the next solve; common group relationships are already
available in the ordinary goal step.
When a rotation has been generated and a class project is selected, the panel
also offers **Save current rotation**. It writes every period's current seats,
locks, and editing commands as a new rotation-plan output without replacing the
source artifact. Existing rotation outputs can be opened with **Continue a
rotation**, which recreates the period drafts so the plan can be adjusted again.
Migration previews also show privacy-safe field paths and type changes, before
and after validation, and the available backup or rollback path without
returning original student values.
For a saved rotation plan, the Project panel can also download a group register as
printable HTML or CSV. Each period lists the group, student, seat, and status, while
retaining empty groups, unseated students, and members missing from the roster.
Before downloading, use the membership preview to review group sizes, seated and
unseated counts, and additions/removals between adjacent periods. The preview uses
anonymous references and does not return names or student IDs to the browser.

## Advanced tools

The sidebar's Advanced tools choice retains Quick Solve and Project workspace
for users who need file-level configuration, complete candidate comparison, or
project paths.

### Quick solve

The Quick solve tab follows three steps:

1. Load the fictional Demo or upload a student list and classroom layout.
2. Choose a preset, optionally add a rules overlay, inspect history, and set
   candidate count, seed, and time limit.
3. Compare candidates, inspect the seating map and scores, then download the
result.

### Lock and repair

After solving, expand **Lock & repair** to keep students in their current
seats, lock seats, or select the students that may be rearranged. Leaving the
affected-student selection empty performs a global re-solve while preserving
the locks. Quick solve reuses the history loaded for the current session;
Project workspace uses the project's history directory. The resulting snapshot
records lock state, repair provenance, and the students whose seats changed.

The repair backend can be `auto`, `fallback`, `ortools`, or the experimental
`native` validator. The native extension is built from a matching source
checkout and is not installed by the main PyPI package or any extra. `auto`
never selects it. Regular installations should use `auto`, `fallback`, or
`ortools`.

### Manual swaps and history

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

The resolved rules panel shows the exact `RuleSet` used by the solver. History
inspection reports student coverage, stale references, disabled seats, and
layout differences before a solve begins.

Web settings can be downloaded and restored later. They include the preset,
rules overlay, candidate count, seed, and time limit, but not the student list,
layout, history, paths, or results. A rules overlay can still contain student
identifiers, so the page warns when the settings file should be treated as
sensitive.

### Project workspace

The legacy Project workspace can open a local project path or accept an uploaded
project JSON file. A local path supports validation, solving, and export because
its referenced files remain available. The React workbench project panel adds
recent-project discovery, history metadata, privacy scanning, and `.seattrellis.zip`
backup/restore for ordinary browser use.

Path mode intentionally accesses the local path entered by the user. Do not
expose this Streamlit service to untrusted network users.

## Downloads

The page includes export template and privacy controls for public,
teacher-internal, and candidate explanation output. It also supports
anonymization, field hiding, A4 orientation, scaling, and Chinese or English
content. Safe template defaults can be tightened but not loosened.

The page can download snapshot or candidate-set JSON, plan reports, HTML,
print-friendly HTML, PDF, PNG, Excel, and Word files. If an optional export
dependency is missing, its installation hint appears without blocking other
formats.

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

Streamlit tables may still need horizontal scrolling on very narrow phones.
The interface currently supports Simplified Chinese and English.
Seat-map clicking and accessible form controls are available. Box selection and
drag-and-drop layout editing remain future work.

## Privacy

Solving happens on the local computer. Registered teacher working files are
removed when their plan is replaced or cleared; remaining Web working
directories are cleaned up when the process exits. Do not commit real student
data, screenshots, or exports to a public repository.
