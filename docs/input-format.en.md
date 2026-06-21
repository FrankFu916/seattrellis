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
- each row has `student_id` or `name`;
- `student_id` values are unique;
- numeric fields are valid;
- unknown columns are preserved in `attributes`.

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
| `enabled` | Optional; `false` marks an unavailable seat |
| `zone` | Optional zone label |
| `near_window` / `near_door` / `near_platform` / `near_ac` | Optional booleans |
| `tags` | Optional tag list |
| `attributes` | Optional extension attributes |

Layout validation checks duplicate `seat_id` values, empty layouts, no enabled seats, and `custom_edges` pointing to unknown or disabled seats.

## Rules JSON

See [rules.en.md](rules.en.md).
