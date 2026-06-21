# Input Formats

SeatTrellis reads three inputs: a student list, a classroom layout, and a rules file. Files in `examples/` are fictional only.

## Student List

The minimal install supports CSV. Install the `excel` extra for Excel `.xlsx` / `.xlsm` support:

```bash
python -m pip install -e ".[excel]"
```

Save legacy `.xls` files as `.xlsx` or CSV first.

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

Run a lightweight preflight before solving:

```bash
seattrellis validate --students examples/students.csv --layout examples/classroom.json --rules examples/rules.json
```

`validate` checks inputs and obvious conflicts only; it does not generate a seating plan. With `--strict`, warnings also fail the command.

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

## Rules JSON

See [rules.en.md](rules.en.md).
