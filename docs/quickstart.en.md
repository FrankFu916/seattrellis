# Quick Start

This document provides detailed installation and CLI usage guidance for SeatTrellis. For a brief project overview, see the [README](../README.en.md).

## Installation

### Minimal Install

```bash
python -m pip install -e .
seattrellis --help
```

The minimal install supports CLI help, CSV input, JSON layout/rules/snapshot/candidate set, built-in rules presets, local project workspaces, the deterministic fallback solver, multi-candidate generation and scoring, and HTML export without heavy optional libraries.

### Common Local Install

```bash
python -m pip install -e ".[excel,image]"
```

Suitable for CSV/Excel input and Excel, PNG, HTML output.

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

## Demo Data

```bash
seattrellis init-demo
```

`init-demo` keeps existing files by default. Use `--force` to overwrite. Minimal installs generate CSV/JSON demo files; installing the `excel` extra also generates `examples/students.xlsx`.

The legacy `seatplanner` command remains available as a compatibility alias; new docs use `seattrellis`.

## Built-in Scenario Presets

```bash
seattrellis presets list
seattrellis presets show daily
seattrellis presets export daily --output outputs/daily.rules.json
```

`presets list` shows eight built-in scenarios: `random`, `exam`, `daily`, `fair-rotation`, `neighbor-aware`, `balanced`, `height-aware`, and `vision-friendly`.

`solve` / `validate` can use `--preset` alone or combine it with `--rules`: the preset is the base, explicitly supplied user JSON fields recursively override it, and hard rules continue through the existing validation and solving path with absolute priority. Missing history, score, height, or vision data produces a warning and degrades only the affected soft rule or score dimension.

## Solving

### Single Snapshot

```bash
# Using a preset
seattrellis solve --students examples/students.csv --layout examples/classroom.json --preset daily --history-dir examples/history --output outputs/daily.snapshot.json

# Using a rules file
seattrellis solve --students examples/students.csv --layout examples/classroom.json --rules examples/rules.json --output outputs/latest.snapshot.json

# With history (fair rotation)
seattrellis solve --students examples/students.csv --layout examples/classroom.json --rules examples/rules.json --history-dir examples/history --output outputs/fair.snapshot.json

# With neighbor avoidance
seattrellis solve --students examples/students.csv --layout examples/classroom.json --rules examples/rules_neighbor_avoidance.json --history-dir examples/history --output outputs/neighbor-aware.snapshot.json
```

### Multi-Candidate Generation and Scoring

```bash
seattrellis solve --students examples/students.csv --layout examples/classroom.json --rules examples/rules_multi_candidate.json --history-dir examples/history --candidates 5 --output outputs/candidates.json --report outputs/plan-report.json
```

`--candidates 1` preserves the old behaviour and writes a normal snapshot. `--candidates N` repeatedly solves with a deterministic seed sequence and an exact-assignment exclusion constraint, writing a `kind: "candidate_set"` JSON artifact. Candidate generation is heuristic, but every candidate must still satisfy every hard constraint. If the feasible space cannot supply enough distinct plans, SeatTrellis keeps the plans it found and records a warning.

### Optional OR-Tools Solver

```bash
python -m pip install -e ".[solver]"
SEATTRELLIS_USE_ORTOOLS=1 seattrellis solve --students examples/students.csv --layout examples/classroom.json --rules examples/rules.json
```

SeatTrellis tries to import OR-Tools only when `SEATTRELLIS_USE_ORTOOLS=1` is set. If the `solver` extra is missing, the CLI prints the install command and exits with a non-zero status.

## Validation

```bash
# Using a preset
seattrellis validate --students examples/students.csv --layout examples/classroom.json --preset daily --history-dir examples/history

# Using a rules file
seattrellis validate --students examples/students.csv --layout examples/classroom.json --rules examples/rules.json
```

`validate` checks input files and obvious rule conflicts only; it does not generate a seating snapshot. `solve` validates first, then writes the snapshot. Error messages try to include the file, field, row number, and hard-rule conflict. With `--strict`, warnings also make the command exit with a non-zero status.

## History Analysis

```bash
# Seat fairness report
seattrellis history-report --students examples/students.csv --layout examples/classroom.json --history-dir examples/history

# Desk-mate / neighbour relationship report
seattrellis pair-report --students examples/students.csv --layout examples/classroom.json --history-dir examples/history
```

`history-report` summarises each student's front, back, side, corner, near-window, near-door, near-platform, and near-AC counts based on the current student list, current layout, and historical snapshots. Add `--output outputs/history-report.json` to write a JSON report.

`pair-report` summarises pair-level desk-mate, horizontal, vertical, diagonal, any-adjacent, and within-distance counts. Add `--top 10` to limit displayed high-frequency pairs, or `--output outputs/pair-report.json` to write JSON.

## Export

```bash
# HTML export (no extras needed)
seattrellis export --snapshot outputs/latest.snapshot.json --format html

# Export recommended candidate
seattrellis export --snapshot outputs/candidates.json --candidate recommended --format html --output outputs/recommended.html
```

After installing the `excel` and `image` extras:

```bash
seattrellis export --snapshot outputs/latest.snapshot.json --format excel
seattrellis export --snapshot outputs/latest.snapshot.json --format png
```

Exported files are written to `outputs/`, which is ignored by Git.

## Project Workflow

```bash
# Create a project file
seattrellis project-init --project examples/project.seattrellis.json --name "Demo Class" --students students.csv --layout classroom.json --rules rules_multi_candidate.json --history-dir history --outputs-dir outputs --candidates 5 --force

# Inspect configuration
seattrellis project-info --project examples/project.seattrellis.json

# Validate
seattrellis project-validate --project examples/project.seattrellis.json

# Solve
seattrellis project-solve --project examples/project.seattrellis.json --candidates 3 --output outputs/project.candidates.json --report outputs/project-plan-report.json

# Export
seattrellis project-export --project examples/project.seattrellis.json --snapshot outputs/project.candidates.json --candidate recommended --format html --output outputs/project-recommended.html
```

`project-init` creates a lightweight local project file; `project-info` checks its settings and path status; `project-validate`, `project-solve`, and `project-export` reuse the existing validation, solving, and export logic. A project file stores relative paths and defaults only — it does not embed student lists or seating data. Relative paths are resolved from the project file's directory.

## Multi-Candidate Scoring Dimensions

Each candidate in a candidate set contains its snapshot, seed, solver backend, total score, hard-constraint summary, and score breakdown. Current explainable dimensions are:

- `fair_rotation_score`, when fair rotation and history are available;
- `avoid_recent_neighbors_score`, when relationship avoidance and pair history are available;
- `score_balance_score`, `height_preference_score`, and `vision_preference_score`, when their rules and required input fields are available;
- `diversity_score`, based on assignment differences among candidates;
- `stability_score`, based on unchanged seats versus the latest historical snapshot;
- `hard_constraint_summary`, covering fixed seats, adjacency rules, minimum distance, and assignment completeness.

Missing history, disabled rules, or insufficient fields produce `not_available` instead of an invented score. The total is a 0–100 weighted average of available dimensions using rule weights. The recommended candidate is the highest-scoring hard-valid plan, with `candidate_id` as a deterministic tie-breaker. Scores support comparison and explanation; they do not claim global optimality.

Snapshots and candidate sets are different formats, and old snapshots remain readable. When `export` receives a candidate set, it exports the recommended candidate by default or a selected ID such as `--candidate candidate_03`.

## Next Steps

- [Input Formats](input-format.en.md)
- [Rules](rules.en.md)
- [Web UI Guide](web.zh.md)
- [Project Workflow Details](project.zh.md)
- [Export Formats](export.zh.md)
