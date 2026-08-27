# Input Formats

[English](input-format.md) / [简体中文](input-format.zh.md)

SeatTrellis reads a student roster, a classroom layout, and a rules document.
A local project file can store their relative paths and common defaults. Files
under `examples/` contain fictional data only.

In v2.0.0, a project workflow (`project-*` commands or the workbench project
panel) keeps these files separate. The standalone CLI instead embeds students,
seats, and rules in one `CoreSolveRequest` problem JSON; see the
[quick start](quickstart.md). Supported v1 roster, layout, and project inputs
have explicit migration paths; history readers also accept the documented
legacy snapshot forms, but migration coverage is not automatic for every
artifact kind.

## Student roster

The local Rust importer handles CSV and Excel `.xlsx` / `.xlsm` files without
optional packages:

```bash
seattrellis project-init --dir my-class
seattrellis project-validate --project my-class/seattrellis.project.json
```

Save legacy `.xls` files as `.xlsx` or CSV first.

### Excel import boundaries

Excel import reads the workbook's **first worksheet**:

- Supported values are shared strings, inline strings, formula cached results
  (`str`), numbers, and booleans with cached values. A formula cell **without a
  cached value is an error**.
- Text cells stay text, so leading zeros such as `001` are preserved.
- The file and decompressed XML parts are capped at 20 MiB, with at most 10,000
  data rows and 256 columns. Oversized or encrypted workbooks fail explicitly.

At least one of `student_id` or `name` is required. Other fields are optional:

| Field | Description |
| --- | --- |
| `student_id` | Stable student identifier; optional but recommended |
| `name` | Display name |
| `gender` | Gender or other grouping metadata |
| `height_cm` | Height; must be positive |
| `score` | Score; must be finite |
| `vision` | Vision information such as `poor` or `0.8` |
| `tags` | Tags separated by comma, semicolon, Chinese comma, dunhao, or pipe |
| `needs` | Special needs using the same separators |
| `notes` | Notes |

The importer checks that:

- the file is not empty;
- headers include `student_id` or `name`;
- each row has `student_id` or `name`;
- a present `name` column is not empty for a non-empty student row;
- `student_id` values are unique;
- `height_cm` and `score` are valid numbers, with column and row context when
  possible;
- unknown columns are preserved in `attributes`;
- a student without `student_id` uses `name` as its stable internal ID and
  produces a `validate` warning.

Run a lightweight preflight before solving:

```bash
seattrellis project-validate \
  --project my-class/seattrellis.project.json \
  --strict
```

`project-validate` checks inputs and obvious conflicts; it does not generate a
seating plan. With `--strict`, warnings also fail the command.

## Classroom layout JSON

Layouts are seat-node based. They do not need to be complete rectangular
matrices:

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

Layout validation checks empty or duplicate seat IDs, row/column types, empty
layouts, layouts with no enabled seats, and custom edges that point to unknown
or disabled seats. Cross-file preflight also checks whether the roster is larger
than the enabled-seat count and whether a rule fixes a student to a disabled
seat.

## Project workspace JSON

A project file is the configuration entry point for the local file-based
workflow. Common names are `seattrellis.project.json` and
`project.seattrellis.json`:

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

`students`, `layout`, and `rules` are required. `history_dir` may be omitted;
the remaining fields have defaults. Every path must be relative to the
directory containing the project file. The loader rejects absolute paths,
traversal, and references that escape the project root. `project-solve` creates
`outputs_dir` when needed, but never invents roster, layout, rules, or history
inputs.

```bash
seattrellis project-info --project examples/project.seattrellis.json
seattrellis project-validate --project examples/project.seattrellis.json
seattrellis project-solve --project examples/project.seattrellis.json
seattrellis project-export \
  --project examples/project.seattrellis.json \
  --snapshot outputs/plan.json \
  --output outputs/plan.html
```

The project file stores paths and defaults, not student lists, grades, notes,
preferences, or snapshot contents. Keep real inputs and outputs under private
ignored directories. A shareable project file does not make the files it
references safe to commit.

## Historical snapshots

`history-report`, `pair-report`, and history-aware validation read SeatTrellis
JSON snapshots. Historical analysis uses JSON snapshots only; it does not need
Excel, PNG, Streamlit, SQLite, or a database.

```bash
seattrellis history-report \
  --problem problem.json \
  --history-dir examples/history
seattrellis pair-report \
  --problem problem.json \
  --history-dir examples/history
seattrellis validate \
  --problem problem.json \
  --history-dir examples/history \
  --preset daily
```

Historical snapshots are interpreted against the current roster and layout:

- multiple snapshots form a sequence in the order supplied, or in sorted file
  name order for `--history-dir`;
- a student absent from a historical snapshot is skipped for that snapshot and
  a warning is recorded;
- an unknown historical seat is marked `unknown` and produces a warning;
- a seat that is now `enabled=false` remains in the historical record but is
  excluded from current position counts;
- pair history uses current `row` / `col`, the adjacency graph, and custom edges
  to identify `desk_mate`, `horizontal`, `vertical`, `diagonal`,
  `adjacent_any`, and `within_distance`;
- `within_distance` uses Chebyshev distance with a default threshold of `2`.

`examples/history/` is fictional. De-identify real historical records and keep
them outside the repository.

## Candidate-set JSON

The v2 `candidates` command writes an `api_version: 2` candidate report. Each
candidate includes a `candidate_id`, assignment, score breakdown, and
hard-constraint summary; the recommended plan is the highest-scoring hard-valid
candidate:

```bash
seattrellis candidates \
  --problem problem.json \
  --count 5 \
  > outputs/candidates.json
```

Legacy candidate-set documents remain readable where the project workflow
supports them. `project-export --candidate <id>` selects a candidate by ID and
defaults to `recommended_candidate_id`. Unknown IDs produce an error listing
available candidates.

`project-solve --report` writes a `plan_comparison_report`; that report is not
itself an exportable seating snapshot. Keep candidate sets, reports, and exports
under ignored private paths such as `outputs/`.

## Seat position categories

Position categories power `history-report` and `fair_rotation`:

- smaller `row` values are closer to the front;
- an explicit `zone` of `front`, `middle`, or `back` wins; otherwise the
  minimum enabled row is `front`, the maximum is `back`, and other rows are
  `middle` (a one-row layout is `middle`);
- `side` means the minimum or maximum enabled-seat `col`;
- `corner` means both a row and column boundary among enabled seats;
- `near_window`, `near_door`, `near_platform`, and `near_ac` come only from
  explicit booleans and default to `false`;
- irregular rooms are treated as actual seat nodes, not filled into a matrix;
- disabled seats do not participate in allocation statistics or boundary
  inference.

## Rules JSON

The `RuleSet` is embedded in the problem JSON's `rules` field or referenced by
the project's `rules` path. In v2, `validate --preset <name>` performs scenario
data-missing checks only; it does not merge preset rules. See
[Rules](rules.md).

The native standalone solve path consumes top-level index-pair hard constraints:
`fixed_seats`, `must_be_adjacent`, `cannot_be_adjacent`, and `min_distance`.
Students are referenced by list index in this form; the [quick start](quickstart.md)
shows an example. The string-reference form under `rules.hard` is resolved by
the workbench/server or by the project loader before it reaches the native
solver. A non-empty unresolved `rules.hard` block is rejected with an explicit
error.
