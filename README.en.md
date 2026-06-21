# SeatTrellis

[![Tests](https://github.com/FrankFu916/seattrellis/actions/workflows/tests.yml/badge.svg)](https://github.com/FrankFu916/seattrellis/actions/workflows/tests.yml)

**[简体中文](README.md) | English**

SeatTrellis is a local-first classroom seating planner for reproducible seating workflows with fictional demo data. It reads a student list, a seat-node classroom layout, and a rules file, then writes a JSON snapshot that can be exported to Excel, PNG, or HTML.

SeatTrellis processes data locally by default. Do not commit real student names, IDs, grades, class names, school names, seating preferences, or historical seating snapshots to a public repository.

![Demo seating chart](docs/assets/demo-seating.png)

## Quick Start

The minimal install includes only the core models, CLI, and fallback solver:

```bash
python -m pip install -e .
seattrellis --help
seattrellis init-demo
seattrellis solve --students examples/students.csv --layout examples/classroom.json --rules examples/rules.json
seattrellis export --snapshot outputs/latest.snapshot.json --format html
```

Exported files are written to `outputs/`, which is ignored by Git.

## Installation Tiers

### Minimal Install

```bash
python -m pip install -e .
seattrellis --help
```

The minimal install supports CLI help, CSV input, JSON layout/rules/snapshot files, the deterministic built-in fallback solver, and HTML export without heavy optional libraries.

### Common Local Install

```bash
python -m pip install -e ".[excel,image]"
```

Use this for CSV/Excel input and Excel, PNG, or HTML output.

### Full Development Install

```bash
python -m pip install -e ".[all,dev]"
pytest
```

The `all` extra includes OR-Tools, Excel, PNG, and Streamlit dependencies. The `dev` extra includes test and build tools.

### Web UI

```bash
python -m pip install -e ".[web,excel,image]"
streamlit run src/seattrellis/web/app.py
```

The web UI depends on Streamlit. Install `excel` and `image` too if you want Excel upload or PNG/Excel downloads in the web UI.

## CLI

```bash
seattrellis --help
seattrellis init-demo --force
seattrellis solve --students examples/students.csv --layout examples/classroom.json --rules examples/rules.json --output outputs/demo.snapshot.json
seattrellis export --snapshot outputs/demo.snapshot.json --format html --output outputs/demo.html
```

After installing the `excel` and `image` extras, you can also run:

```bash
seattrellis solve --students examples/students.xlsx --layout examples/classroom.json --rules examples/rules.json
seattrellis export --snapshot outputs/latest.snapshot.json --format excel
seattrellis export --snapshot outputs/latest.snapshot.json --format png
```

`init-demo` keeps existing files by default. Use `--force` to overwrite generated demo files. Minimal installs generate CSV/JSON demo files; installing the `excel` extra also enables `examples/students.xlsx` generation. The legacy `seatplanner` command remains available as a compatibility alias; new docs use `seattrellis`.

## Inputs And Rules

- Student lists support CSV; installing the `excel` extra enables `.xlsx` and `.xlsm`. Save legacy `.xls` files as `.xlsx` or CSV first.
- Classroom layouts are JSON seat-node graphs and support `enabled=false` unavailable seats.
- Rule files separate `hard` constraints from `soft` preferences.
- See [input formats](docs/input-format.en.md) and [rules](docs/rules.en.md).

## Solver

SeatTrellis uses a deterministic built-in fallback solver by default so the demo and small seating workflows run without heavy solver dependencies. Optional OR-Tools CP-SAT support is available through the `solver` extra:

```bash
python -m pip install -e ".[solver]"
SEATTRELLIS_USE_ORTOOLS=1 seattrellis solve --students examples/students.csv --layout examples/classroom.json --rules examples/rules.json
```

SeatTrellis tries to import OR-Tools only when `SEATTRELLIS_USE_ORTOOLS=1` is set. If the `solver` extra is missing, the CLI prints the install command and exits with a non-zero status.

## Current Support

- CSV student import, with Excel import available through the `excel` extra;
- JSON classroom layouts, rules, and snapshots;
- seat nodes and adjacency graphs;
- fixed seats, must-adjacent, cannot-adjacent, and minimum-distance rules;
- vision-front, height-back, randomization, and score-balance preferences;
- HTML export, with Excel / PNG export available through the `excel` / `image` extras;
- CLI, local Streamlit UI, fictional examples, pytest, and GitHub Actions.

## Privacy

- `examples/` must contain fictional data only.
- `outputs/`, `exports/`, `snapshots/`, `private/`, `data/`, `real_students/`, `real_classes/`, and `.env` are ignored.
- Before sharing Issues, PRs, screenshots, or test data, remove names, IDs, grades, notes, class names, school names, and any identifying information.

## Release

See the [release checklist](docs/release-checklist.md) and [CHANGELOG.md](CHANGELOG.md) for v0.1.1 preparation.

## License

Apache License 2.0. See [LICENSE](LICENSE).
