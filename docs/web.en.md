# Web UI Guide

## Start the app

```bash
python -m pip install -e ".[web,excel,image,pdf,docx]"
streamlit run src/seattrellis/web/app.py --server.address 127.0.0.1
```

The sidebar language switch changes the interface between Simplified Chinese
and English. It does not clear loaded data, the current step, or solve results.

## Teacher workspace

The default workspace keeps the ordinary path focused on classroom tasks:

1. Enter a class name and import a CSV, XLSX, or XLSM roster. A name column is
   enough to begin.
2. Accept the recommended 30-, 48-, or 60-seat room, or set custom rows, seats
   per row, and aisle positions.
3. Choose Daily rotation, Fair shuffle, or Peer support and generate three
   seating options.
4. Review the recommended map, then swap, move, lock, undo, or redo as needed.
5. Prepare and download either a public print or a teacher print.

Visiting Advanced tools and returning restores the parsed roster, room, goal,
and generated plan without retaining the original upload bytes. **Start over
and clear student list** clears only the teacher workspace.

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

The Project workspace can open a local project path or accept an uploaded
project JSON file. A local path supports validation, solving, and export because
its referenced files remain available. A standalone upload is validated and
displayed without resolving its server-side paths; solving and export remain
disabled until bundled Project upload is available.

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
drag-and-drop are not implemented yet.

## Privacy

Solving happens on the local computer. Registered teacher working files are
removed when their plan is replaced or cleared; remaining Web working
directories are cleaned up when the process exits. Do not commit real student
data, screenshots, or exports to a public repository.
