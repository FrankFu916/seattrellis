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
seattrellis validate --students examples/students.csv --layout examples/classroom.json --rules examples/rules.json
seattrellis history-report --students examples/students.csv --layout examples/classroom.json --history-dir examples/history
seattrellis pair-report --students examples/students.csv --layout examples/classroom.json --history-dir examples/history
seattrellis solve --students examples/students.csv --layout examples/classroom.json --rules examples/rules_neighbor_avoidance.json --history-dir examples/history --output outputs/neighbor-aware.snapshot.json
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
seattrellis validate --students examples/students.csv --layout examples/classroom.json --rules examples/rules.json
seattrellis solve --students examples/students.csv --layout examples/classroom.json --rules examples/rules.json --output outputs/demo.snapshot.json
seattrellis solve --students examples/students.csv --layout examples/classroom.json --rules examples/rules.json --history-dir examples/history --output outputs/fair.snapshot.json
seattrellis solve --students examples/students.csv --layout examples/classroom.json --rules examples/rules_neighbor_avoidance.json --history-dir examples/history --output outputs/neighbor-aware.snapshot.json
seattrellis history-report --students examples/students.csv --layout examples/classroom.json --history-dir examples/history
seattrellis pair-report --students examples/students.csv --layout examples/classroom.json --history-dir examples/history
seattrellis export --snapshot outputs/demo.snapshot.json --format html --output outputs/demo.html
```

After installing the `excel` and `image` extras, you can also run:

```bash
seattrellis solve --students examples/students.xlsx --layout examples/classroom.json --rules examples/rules.json
seattrellis export --snapshot outputs/latest.snapshot.json --format excel
seattrellis export --snapshot outputs/latest.snapshot.json --format png
```

`init-demo` keeps existing files by default. Use `--force` to overwrite generated demo files. Minimal installs generate CSV/JSON demo files; installing the `excel` extra also enables `examples/students.xlsx` generation. The legacy `seatplanner` command remains available as a compatibility alias; new docs use `seattrellis`.

`validate` checks input files and obvious rule conflicts only; it does not generate a seating snapshot. `solve` validates first, then writes the snapshot. Error messages try to include the file, field, row number, and hard-rule conflict. With `--strict`, warnings also make the command exit with a non-zero status.

`solve` accepts repeated `--history` snapshot paths or `--history-dir examples/history` for a directory of `*.snapshot.json` files. `history-report` summarizes each student's front, back, side, corner, near-window, near-door, near-platform, and near-AC counts. Add `--output outputs/history-report.json` to write a JSON report. `pair-report` summarizes pair-level desk-mate, horizontal, vertical, diagonal, any-adjacent, and within-distance counts. Add `--top 10` to limit displayed high-frequency pairs, or `--output outputs/pair-report.json` to write JSON.

## Inputs And Rules

- Student lists support CSV; installing the `excel` extra enables `.xlsx` and `.xlsm`. Save legacy `.xls` files as `.xlsx` or CSV first.
- Classroom layouts are JSON seat-node graphs and support `enabled=false` unavailable seats.
- Rule files separate `hard` constraints from `soft` preferences.
- Unknown rule fields are reported as errors so typos are not silently ignored.
- `fair_rotation` is a history-based soft rule. Hard rules still take priority, and missing history does not fail solving.
- `avoid_recent_neighbors` is a history-based soft rule for repeated desk-mate and neighbor relationships. Fixed seats, must-adjacent, cannot-adjacent, and minimum-distance hard rules still take priority, and missing history does not fail solving. The fallback and OR-Tools solvers treat it as heuristic scoring, not a guarantee of global optimality.
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
- vision-front, height-back, randomization, score-balance, fair-rotation, and recent-neighbor avoidance heuristic preferences;
- historical snapshot statistics, the local `history-report` fairness summary, and `pair-report` relationship-history summary;
- HTML export, with Excel / PNG export available through the `excel` / `image` extras;
- validation preflight and conflict diagnostics, CLI, local Streamlit UI, fictional examples, pytest, and GitHub Actions.

## Privacy

- `examples/` must contain fictional data only.
- `examples/history/` contains fictional history snapshots only for fair-rotation and relationship-avoidance demos.
- `outputs/`, `exports/`, `snapshots/`, `private/`, `data/`, `real_students/`, `real_classes/`, and `.env` are ignored.
- Before sharing Issues, PRs, screenshots, test data, or historical seating records, remove names, IDs, grades, notes, class names, school names, and any identifying information. Do not commit real historical seating snapshots to a public repository.

Current fair rotation and relationship avoidance use heuristic scoring from historical counts. They do not guarantee absolute fairness or global optimality.

## Release

See the [release checklist](docs/release-checklist.md) and [CHANGELOG.md](CHANGELOG.md) for v0.2.1 preparation.

## License

Apache License 2.0. See [LICENSE](LICENSE).
