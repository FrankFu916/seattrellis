# Input Formats

SeatTrellis reads a student list, a classroom layout, and a rules file. A local project file can store their relative paths and common defaults. Files in `examples/` are fictional only.

In v2 these files are consumed through the project workflow (`project-*`
commands, the workbench's project panel); the standalone CLI instead embeds
students, seats, and rules in one problem JSON (`CoreSolveRequest`) — see the
[quick start](quickstart.en.md). v1-era file formats remain readable and are
migrated automatically.

## Student List

CSV and Excel `.xlsx` / `.xlsm` are both handled by the local Rust importer —
no optional installs are needed:

```bash
seattrellis_cli project-init --dir my-class   # create a project in a directory with students.csv
seattrellis_cli project-validate --project my-class/seattrellis.project.json
```

Save legacy `.xls` files as `.xlsx` or CSV first.

### Excel (.xlsx / .xlsm) import boundaries

Excel import reads the workbook's **first worksheet** under these bounds:

- Supported cell values: shared strings, inline strings, formula cached results
  (`str`), numbers, and booleans (with cached values); a formula cell
  **without a cached value is an error**;
- Text cells are read as text, so leading zeros such as `001` are preserved;
- Limits: files up to 20 MiB (decompressed XML parts are capped the same way),
  up to 10,000 data rows, and up to 256 columns; oversized or encrypted
  workbooks fail with an explicit error.

At least one of `student_id` or `name` is required. Other fields are optional:

| Field | Description |
| --- | --- |
| `student_id` | Stable student identifier, optional but recommended |
| `name` | Display name |
| `gender` | Gender or grouping metadata |
| `height_cm` | Height, must be positive |
| `score` | Score, must be a finite number |
| `vision` | Vision info such as `poor` or `0.8` |
| `tags` | Tags separated by comma, semicolon, Chinese comma, dunhao, or pipe |
| `needs` | Special needs using the same separators |
| `notes` | Notes |

The importer validates:

- the file is not empty;
- headers include at least one of `student_id` or `name`;
- each row has `student_id` or `name`;
- if a `name` column is present, non-empty student rows must not have an empty `name`;
- `student_id` values are unique;
- `height_cm` and `score` values are valid numbers, with errors pointing to the column and row when possible;
- unknown columns are preserved in `attributes`.
- students without `student_id` use `name` as their stable internal ID and produce a `validate` warning.

Run a lightweight preflight before solving (through the project workflow, or
inline the data in a problem JSON and run `seattrellis_cli validate --problem`):

```bash
seattrellis_cli project-validate --project my-class/seattrellis.project.json --strict
```

`project-validate` checks inputs and obvious conflicts only; it does not generate a seating plan. With `--strict`, warnings also fail the command.

## Classroom Layout JSON

Layouts are seat-node based and do not need to be complete matrices.

```json
{
  "layout_id": "fictional-room",
  "name": "Fictional Classroom",
  "seats": [
    {"seat_id": "R1C1", "row": 1, "col": 1, "enabled": true},
    {"seat_id": "R1C2", "row": 1, "col": 2, "enabled": false, "zone": "aisle"}
  ],
  "adjacency": {
    "include_horizontal": true,
    "include_vertical": false,
    "include_diagonal": false,
    "custom_edges": []
  }
}
```

Seat fields:

| Field | Description |
| --- | --- |
| `seat_id` | Required unique seat ID |
| `row` / `col` | Required positive integers |
| `x` / `y` | Optional coordinates; default to `col` / `row` |
| `enabled` | Optional; `false` marks an unavailable seat |
| `zone` | Optional zone label |
| `near_window` / `near_door` / `near_platform` / `near_ac` | Optional booleans |
| `tags` | Optional tag list |
| `attributes` | Optional extension attributes |

Layout validation checks empty `seat_id` values, duplicate `seat_id` values, `row` / `col` types, empty layouts, no enabled seats, and `custom_edges` pointing to unknown or disabled seats. Cross-file preflight also checks whether the student count exceeds enabled seats and whether rules fix students to `enabled=false` seats.

Invalid examples include `examples/invalid/duplicate_student_id.csv`, `examples/invalid/duplicate_seat_id.json`, and `examples/invalid/not_enough_seats.json`.

## Project Workspace JSON

A project file is the configuration entry point for a local file-based workflow. Recommended names include `seattrellis.project.json` and `project.seattrellis.json`:

```json
{
  "kind": "seattrellis_project",
  "schema_version": 1,
  "name": "Demo Class",
  "students": "students.csv",
  "layout": "classroom.json",
  "rules": "rules_multi_candidate.json",
  "history_dir": "history",
  "outputs_dir": "outputs",
  "default_candidates": 5,
  "default_candidate": "recommended",
  "default_export_format": "html"
}
```

`students`, `layout`, and `rules` are required. `history_dir` may be omitted, and the remaining fields have defaults. Every path must be relative and is resolved from the directory containing the project file, not from the package installation directory. `project-solve` creates `outputs_dir` when needed, but it never creates or invents student, layout, rules, or history inputs.

```bash
seattrellis_cli project-info --project examples/project.seattrellis.json
seattrellis_cli project-validate --project examples/project.seattrellis.json
seattrellis_cli project-solve --project examples/project.seattrellis.json
seattrellis_cli project-export --project examples/project.seattrellis.json
```

The project file stores paths and defaults only. It does not contain student lists, grades, notes, seating preferences, or snapshot contents. Keep real inputs and outputs under private ignored directories; a shareable project file does not make the private data it references safe to commit.

## Historical Snapshots

`history-report`, `pair-report`, and `validate --history` / `--history-dir` read
SeatTrellis JSON snapshots (v1-era snapshots are migrated automatically).
Historical analysis depends only on JSON snapshots. It does not require Excel,
PNG, Streamlit, SQLite, or any database.

```bash
seattrellis_cli history-report --problem problem.json --history-dir examples/history
seattrellis_cli pair-report --problem problem.json --history-dir examples/history
seattrellis_cli validate --problem problem.json --history-dir examples/history --preset daily
```

Historical snapshots are interpreted against the current student list and current layout:

- multiple snapshots form a history sequence in the order passed, or by sorted file name for `--history-dir`;
- if a historical snapshot is missing a current student, that student is skipped for that snapshot and a warning is recorded;
- if a historical snapshot references a seat that does not exist in the current layout, the record is marked `unknown` and a warning is recorded;
- if a historical snapshot references an `enabled=false` seat in the current layout, the seat record is retained but excluded from position category counts;
- pair history uses the current layout `row` / `col`, adjacency graph, and custom edges to detect `desk_mate`, `horizontal`, `vertical`, `diagonal`, `adjacent_any`, and `within_distance`;
- if pair history references an `enabled=false` seat, that seat remains unavailable for new solving, but historical relationships are counted from row/column coordinates when possible and a warning is recorded;
- `within_distance` uses Chebyshev distance with a default threshold of `2`;
- v0.1.0 / v0.1.1 / v0.1.2 / v0.2.0 / v0.2.1 snapshots still load; v0.2.2 does not change the ordinary snapshot schema.

`examples/history/` contains fictional history only. Real historical seating records should be de-identified and stored in ignored directories, not committed to a public repository.

## Candidate Set JSON

v2 multi-candidate generation uses the `candidates` command and writes an
`api_version: 2` candidate report: each candidate carries a `candidate_id`, an
assignment, a plan-score breakdown, and a hard-constraint summary; the
recommended plan is the highest-scoring hard-valid candidate:

```bash
seattrellis_cli candidates --problem problem.json --count 5 > outputs/candidates.json
```

v1-era candidate sets (`kind: "candidate_set"`, `schema_version: "0.2.2"`, each
candidate embedding a full snapshot) remain readable. The project workflow
(`project-export --candidate <id>`) selects a plan from such an artifact by
`candidate_id` and exports it, defaulting to `recommended_candidate_id`.
Unknown IDs produce a friendly error listing available candidates.

The `kind: "plan_comparison_report"` file written by `project-solve --report`
is a comparison report, not an exportable seating snapshot. Keep real candidate
sets, reports, and exports under ignored private paths such as `outputs/`; do
not commit them to a public repository.

## Seat Position Categories

Position categories power `history-report` and `fair_rotation`. Current rules:

- smaller `row` values are closer to the front;
- if `zone` is explicitly `front`, `middle`, or `back`, that explicit zone wins; otherwise, SeatTrellis infers from enabled-seat rows: minimum row is `front`, maximum row is `back`, other rows are `middle`; a one-row layout is inferred as `middle`;
- `side` means minimum or maximum enabled-seat `col` in the current layout;
- `corner` means both a row boundary and a column boundary among enabled seats;
- `near_window`, `near_door`, `near_platform`, and `near_ac` come only from explicit boolean fields and default to `false` when absent;
- irregular classrooms are handled as actual seat nodes, not filled into a complete matrix;
- `enabled=false` seats do not participate in allocation statistics or boundary inference.

## Rules JSON

The rules JSON (`RuleSet`) is embedded in the problem JSON's `rules` field or
referenced by the project's `rules` path. In v2, `validate --preset <name>`
performs scenario data-missing checks (history/score/height/vision warnings)
only; it does not merge preset rules.

See [rules.en.md](rules.en.md) for rules and preset behavior.

Note: the native solve path consumes only the **top-level index-pair form** of
hard constraints — `fixed_seats` / `must_be_adjacent` / `cannot_be_adjacent` /
`min_distance`, with students referenced by list index (see the problem.json
example in the [quick start](quickstart.en.md)). The string-reference form in
`rules.hard` is not consumed by the native path; a non-empty `rules.hard`
block is rejected with an explicit error pointing at the top-level form. The
workbench/server resolves the string form into top-level index pairs before
generating a problem.
