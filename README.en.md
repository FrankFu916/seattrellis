# SeatTrellis

[![Tests](https://github.com/FrankFu916/seattrellis/actions/workflows/tests.yml/badge.svg)](https://github.com/FrankFu916/seattrellis/actions/workflows/tests.yml)

**[简体中文](README.md) | English**

SeatTrellis is a local-first classroom seating planner for reproducible seating workflows with fictional demo data. It reads a student list, a seat-node classroom layout, and a rules file, then writes a JSON snapshot that can be exported to Excel, PNG, or HTML.

SeatTrellis processes data locally by default. Do not commit real student names, IDs, grades, class names, school names, seating preferences, or historical seating snapshots to a public repository.

![Demo seating chart](docs/assets/demo-seating.png)

## Quick Start

```bash
python -m pip install -e ".[dev,web]"
seattrellis init-demo
seattrellis solve --students examples/students.xlsx --layout examples/classroom.json --rules examples/rules.json
seattrellis export --snapshot outputs/latest.snapshot.json --format excel
seattrellis export --snapshot outputs/latest.snapshot.json --format png
seattrellis export --snapshot outputs/latest.snapshot.json --format html
```

Exported files are written to `outputs/`, which is ignored by Git.

## Installation

macOS / Linux:

```bash
git clone https://github.com/FrankFu916/seattrellis.git
cd seattrellis
python -m venv .venv
source .venv/bin/activate
python -m pip install --upgrade pip
python -m pip install -e ".[dev,web]"
pytest
```

Windows PowerShell:

```powershell
git clone https://github.com/FrankFu916/seattrellis.git
cd seattrellis
python -m venv .venv
.\.venv\Scripts\Activate.ps1
python -m pip install --upgrade pip
python -m pip install -e ".[dev,web]"
pytest
```

## CLI

```bash
seattrellis --help
seattrellis init-demo --force
seattrellis solve --students examples/students.csv --layout examples/classroom.json --rules examples/rules.json --output outputs/demo.snapshot.json
seattrellis export --snapshot outputs/demo.snapshot.json --format html --output outputs/demo.html
```

`init-demo` keeps existing files by default. Use `--force` to overwrite generated demo files. The legacy `seatplanner` command remains available as a compatibility alias; new docs use `seattrellis`.

## Inputs And Rules

- Student lists support CSV, `.xlsx`, and `.xlsm`; at least one of `student_id` or `name` is required.
- Classroom layouts are JSON seat-node graphs and support `enabled=false` unavailable seats.
- Rule files separate `hard` constraints from `soft` preferences.
- See [input formats](docs/input-format.en.md) and [rules](docs/rules.en.md).

## Solver

SeatTrellis uses a deterministic built-in fallback solver by default so the demo and small seating workflows run without heavy solver dependencies. Optional OR-Tools CP-SAT support is available through the `solver` extra:

```bash
python -m pip install -e ".[solver]"
SEATTRELLIS_USE_ORTOOLS=1 seattrellis solve --students examples/students.xlsx --layout examples/classroom.json --rules examples/rules.json
```

If OR-Tools cannot be imported locally, SeatTrellis falls back to the built-in solver.

## Local Web UI

```bash
python -m pip install -e ".[web]"
streamlit run src/seattrellis/web/app.py
```

## Current Support

- CSV / Excel student import;
- JSON classroom layouts, rules, and snapshots;
- seat nodes and adjacency graphs;
- fixed seats, must-adjacent, cannot-adjacent, and minimum-distance rules;
- vision-front, height-back, randomization, and score-balance preferences;
- Excel, PNG, and HTML export;
- CLI, local Streamlit UI, fictional examples, pytest, and GitHub Actions.

## Privacy

- `examples/` must contain fictional data only.
- `outputs/`, `exports/`, `snapshots/`, `private/`, `data/`, `real_students/`, `real_classes/`, and `.env` are ignored.
- Before sharing Issues, PRs, screenshots, or test data, remove names, IDs, grades, notes, class names, school names, and any identifying information.

## Release

See the [release checklist](docs/release-checklist.md) and [CHANGELOG.md](CHANGELOG.md) for v0.1.0 preparation.

## License

Apache License 2.0. See [LICENSE](LICENSE).
