# SeatTrellis

**[简体中文](README.md)｜English**

A privacy-first classroom seating planner for fair, constraint-based, and reproducible seat arrangements.

## Overview

SeatTrellis is a local-first seating planner for teachers, homeroom advisors, school staff, and developers who need reproducible classroom seating workflows. It generates seating plans from student lists, classroom layouts, and rule files, then saves the result as a JSON snapshot for review and export.

By default, SeatTrellis processes data locally. It does not upload real student names, grades, seating preferences, or seating snapshots to a cloud service.

## Features

- Local-first and privacy-friendly;
- imports student lists from CSV and Excel;
- models classrooms with seat nodes and adjacency graphs;
- supports hard constraints and soft constraints;
- uses OR-Tools CP-SAT for automatic seating;
- saves reproducible JSON snapshots;
- exports Excel, PNG, and HTML files;
- provides a CLI;
- includes a local Streamlit web UI;
- ships fictional examples, pytest coverage, and GitHub Actions.

## Use Cases

- Regular classroom seating;
- exam seating;
- group collaboration and desk pairing;
- fixed seats, separation rules, and distance rules;
- fair rotation and balanced assignment;
- reproducible seating records for later review.

## Quick Start

```bash
python -m pip install -e ".[dev]"
seattrellis init-demo
seattrellis solve --students examples/students.xlsx --layout examples/classroom.json --rules examples/rules.json
seattrellis export --snapshot outputs/latest.snapshot.json --format excel
seattrellis export --snapshot outputs/latest.snapshot.json --format png
seattrellis export --snapshot outputs/latest.snapshot.json --format html
```

Run the local web UI:

```bash
python -m pip install -e ".[web]"
streamlit run src/seattrellis/web/app.py
```

## Installation

```bash
git clone https://github.com/FrankFu916/seattrellis.git
cd seattrellis
python -m venv .venv
source .venv/bin/activate
python -m pip install -e ".[dev,web]"
pytest
```

On Windows PowerShell:

```powershell
git clone https://github.com/FrankFu916/seattrellis.git
cd seattrellis
python -m venv .venv
.\.venv\Scripts\Activate.ps1
python -m pip install -e ".[dev,web]"
pytest
```

## Usage

Create fictional demo data:

```bash
seattrellis init-demo
```

Generate a seating snapshot:

```bash
seattrellis solve \
  --students examples/students.xlsx \
  --layout examples/classroom.json \
  --rules examples/rules.json
```

Export the snapshot:

```bash
seattrellis export --snapshot outputs/latest.snapshot.json --format excel
seattrellis export --snapshot outputs/latest.snapshot.json --format png
seattrellis export --snapshot outputs/latest.snapshot.json --format html
```

The legacy `seatplanner` command is still available as a compatibility alias. New documentation uses `seattrellis`.

## Input Formats

Student files can be CSV or Excel. At least one of `student_id` or `name` is required.

```csv
student_id,name,gender,height_cm,score,vision,tags,needs,notes
STU001,Student001,F,154,92,poor,leader,vision_front,
```

Classroom layouts are JSON files based on seat nodes rather than plain matrices.

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

Rule files separate `hard` constraints from weighted `soft` preferences.

```json
{
  "seed": 42,
  "hard": {
    "fixed_seats": [{"student": "STU001", "seat_id": "R1C1"}],
    "must_be_adjacent": [{"students": ["STU002", "STU003"]}],
    "cannot_be_adjacent": [{"students": ["STU004", "STU005"]}],
    "min_distance": []
  },
  "soft": {
    "vision_front": {"enabled": true, "weight": 20},
    "height_back": {"enabled": true, "weight": 1},
    "randomize": {"enabled": true, "weight": 1},
    "score_balance": {"enabled": true, "weight": 1}
  }
}
```

See `examples/students.csv`, `examples/students.xlsx`, `examples/classroom.json`, and `examples/rules.json` for complete working examples.

## Outputs

SeatTrellis currently supports:

- JSON snapshots, written to `outputs/latest.snapshot.json` by default;
- Excel seating charts and assignment tables;
- PNG seating chart images;
- HTML seating charts that can be opened locally.

## Project Structure

```text
.
├── src/seattrellis/
│   ├── models/      # Data models
│   ├── solver/      # Adjacency graph and CP-SAT solver
│   ├── io/          # CSV, Excel, and JSON import/persistence
│   ├── exporters/   # Excel, PNG, and HTML export
│   ├── web/         # Streamlit local UI
│   ├── cli.py       # CLI entry point
│   └── demo.py      # Fictional demo data
├── examples/        # Fictional examples only
├── tests/           # pytest coverage
└── .github/workflows/tests.yml
```

## Privacy Notice

- Do not commit real student names, grades, seating preferences, class information, or historical seating snapshots.
- `examples/` must contain fictional data only.
- `outputs/`, `exports/`, `snapshots/`, `private/`, `data/`, `real_students/`, `real_classes/`, and `.env` are ignored by Git.
- SeatTrellis processes data locally by default and does not upload data to the cloud.
- Remove names, IDs, grades, notes, class names, school names, and any identifying details before sharing examples publicly.

## Roadmap

Completed:

- CSV/Excel student import;
- JSON layout, rules, and snapshots;
- seat nodes and adjacency graphs;
- OR-Tools CP-SAT seating;
- fixed-seat, adjacency, non-adjacency, and distance rules;
- front seating for vision needs, back seating for taller students, randomization, and score pairing;
- Excel, PNG, and HTML export;
- CLI, local Streamlit UI, examples, tests, and CI.

Planned:

- SQLite history storage;
- stronger historical deskmate avoidance and fair rotation;
- group balance, tag distribution, and zone preferences;
- Word/PDF export;
- Word/PDF/image import and optional OCR;
- interactive classroom layout editor;
- stable JSON schema and PyPI release.

## Contributing

Issues and pull requests are welcome. Please run `pytest`, add tests for new rules or import/export behavior, and never submit real student data or private classroom material.

## License

SeatTrellis is licensed under the Apache License 2.0. See [LICENSE](LICENSE).
