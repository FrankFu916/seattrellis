# SeatTrellis

[![Tests](https://github.com/FrankFu916/seattrellis/actions/workflows/tests.yml/badge.svg)](https://github.com/FrankFu916/seattrellis/actions/workflows/tests.yml)

**[简体中文](README.md) | English**

SeatTrellis is a local-first classroom seating planner for reproducible seating workflows with fictional demo data. It can write one JSON snapshot or generate multiple explainably scored candidate plans, then export Excel, PNG, or HTML.

SeatTrellis processes data locally by default. Do not commit real student names, IDs, grades, class names, school names, seating preferences, or historical seating snapshots to a public repository.

![Demo seating chart](docs/assets/demo-seating.png)

## Quick Start

The minimal install includes only the core models, CLI, and fallback solver:

```bash
python -m pip install -e .
seattrellis --help
seattrellis init-demo
seattrellis presets list
seattrellis presets show daily
seattrellis validate --students examples/students.csv --layout examples/classroom.json --preset daily --history-dir examples/history
seattrellis solve --students examples/students.csv --layout examples/classroom.json --preset daily --history-dir examples/history --output outputs/daily.snapshot.json
seattrellis project-info --project examples/project.seattrellis.json
seattrellis project-validate --project examples/project.seattrellis.json
seattrellis project-solve --project examples/project.seattrellis.json --candidates 3 --output outputs/project.candidates.json
seattrellis project-export --project examples/project.seattrellis.json --snapshot outputs/project.candidates.json --candidate recommended --format html --output outputs/project-recommended.html
seattrellis validate --students examples/students.csv --layout examples/classroom.json --rules examples/rules.json
seattrellis history-report --students examples/students.csv --layout examples/classroom.json --history-dir examples/history
seattrellis pair-report --students examples/students.csv --layout examples/classroom.json --history-dir examples/history
seattrellis solve --students examples/students.csv --layout examples/classroom.json --rules examples/rules_neighbor_avoidance.json --history-dir examples/history --output outputs/neighbor-aware.snapshot.json
seattrellis solve --students examples/students.csv --layout examples/classroom.json --rules examples/rules_multi_candidate.json --history-dir examples/history --candidates 5 --output outputs/candidates.json --report outputs/plan-report.json
seattrellis export --snapshot outputs/candidates.json --candidate recommended --format html --output outputs/recommended.html
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

The minimal install supports CLI help, CSV input, JSON layout/rules/snapshot/candidate-set files, built-in rules presets, local project workspaces, the deterministic fallback solver, multi-candidate scoring, and HTML export without heavy optional libraries.

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

The web UI can use a built-in preset, optional rules JSON overlay, multiple history snapshots, and 1-20 generated candidates. It shows the recommended plan, score details, hard-rule checks, and downloads for JSON, reports, HTML, PNG, or Excel. It can also read a local project file and reuse the project-info, validate, solve, and export workflow.

## CLI

```bash
seattrellis --help
seattrellis init-demo --force
seattrellis presets list
seattrellis presets show daily
seattrellis presets export daily --output outputs/daily.rules.json
seattrellis validate --students examples/students.csv --layout examples/classroom.json --preset daily --history-dir examples/history
seattrellis solve --students examples/students.csv --layout examples/classroom.json --preset daily --history-dir examples/history --output outputs/daily.snapshot.json
seattrellis project-init --project examples/project.seattrellis.json --name "Demo Class" --students students.csv --layout classroom.json --rules rules_multi_candidate.json --history-dir history --outputs-dir outputs --candidates 5 --force
seattrellis project-info --project examples/project.seattrellis.json
seattrellis project-validate --project examples/project.seattrellis.json
seattrellis project-solve --project examples/project.seattrellis.json --candidates 3 --output outputs/project.candidates.json --report outputs/project-plan-report.json
seattrellis project-export --project examples/project.seattrellis.json --snapshot outputs/project.candidates.json --candidate recommended --format html --output outputs/project-recommended.html
seattrellis validate --students examples/students.csv --layout examples/classroom.json --rules examples/rules.json
seattrellis solve --students examples/students.csv --layout examples/classroom.json --rules examples/rules.json --output outputs/demo.snapshot.json
seattrellis solve --students examples/students.csv --layout examples/classroom.json --rules examples/rules.json --history-dir examples/history --output outputs/fair.snapshot.json
seattrellis solve --students examples/students.csv --layout examples/classroom.json --rules examples/rules_neighbor_avoidance.json --history-dir examples/history --output outputs/neighbor-aware.snapshot.json
seattrellis solve --students examples/students.csv --layout examples/classroom.json --rules examples/rules_multi_candidate.json --history-dir examples/history --candidates 5 --output outputs/candidates.json --report outputs/plan-report.json
seattrellis history-report --students examples/students.csv --layout examples/classroom.json --history-dir examples/history
seattrellis pair-report --students examples/students.csv --layout examples/classroom.json --history-dir examples/history
seattrellis export --snapshot outputs/demo.snapshot.json --format html --output outputs/demo.html
seattrellis export --snapshot outputs/candidates.json --candidate recommended --format html --output outputs/recommended.html
```

After installing the `excel` and `image` extras, you can also run:

```bash
seattrellis solve --students examples/students.xlsx --layout examples/classroom.json --rules examples/rules.json
seattrellis export --snapshot outputs/latest.snapshot.json --format excel
seattrellis export --snapshot outputs/latest.snapshot.json --format png
```

`init-demo` keeps existing files by default. Use `--force` to overwrite generated demo files. Minimal installs generate CSV/JSON demo files; installing the `excel` extra also enables `examples/students.xlsx` generation. The legacy `seatplanner` command remains available as a compatibility alias; new docs use `seattrellis`.

`presets list` shows eight built-in scenarios: `random`, `exam`, `daily`, `fair-rotation`, `neighbor-aware`, `balanced`, `height-aware`, and `vision-friendly`. `presets show <name>` displays metadata and generated standard rules JSON; `presets export <name>` writes a normal rules file. `solve` and `validate` may use `--preset` alone or combine it with `--rules`: the preset is the base, explicitly supplied user JSON fields recursively override it, and hard rules continue through the existing absolute-priority validation and solving path. Missing history, score, height, or vision data produces a warning and degrades only the affected soft rule or score dimension.

`project-init` creates a lightweight local project file; `project-info` checks its settings and path status; `project-validate`, `project-solve`, and `project-export` reuse the existing validation, solving, and export logic. A project file stores relative paths and defaults only—it does not embed student lists or seating data. Relative paths are resolved from the project file's directory. The existing `--students` / `--layout` / `--rules` workflow remains supported.

`validate` checks input files and obvious rule conflicts only; it does not generate a seating snapshot. `solve` validates first, then writes the snapshot. Error messages try to include the file, field, row number, and hard-rule conflict. With `--strict`, warnings also make the command exit with a non-zero status.

`solve` accepts repeated `--history` snapshot paths or `--history-dir examples/history` for a directory of `*.snapshot.json` files. `history-report` summarizes each student's front, back, side, corner, near-window, near-door, near-platform, and near-AC counts. Add `--output outputs/history-report.json` to write a JSON report. `pair-report` summarizes pair-level desk-mate, horizontal, vertical, diagonal, any-adjacent, and within-distance counts. Add `--top 10` to limit displayed high-frequency pairs, or `--output outputs/pair-report.json` to write JSON.

## Multiple Candidates And Scoring

`--candidates 1` preserves the existing single-snapshot behavior. `--candidates N` repeatedly solves with a deterministic seed sequence and an exact-assignment exclusion constraint, then writes a separate `kind: "candidate_set"` JSON artifact. Candidate generation is heuristic, but every candidate must still satisfy every hard constraint. If the feasible space cannot supply enough distinct plans, SeatTrellis keeps the plans it found and records a warning. Both the fallback and OR-Tools backends support assignment exclusion.

Each candidate contains its snapshot, seed, solver backend, total score, hard-constraint summary, and score breakdown. Current dimensions are:

- `fair_rotation_score`, when fair rotation and history are available;
- `avoid_recent_neighbors_score`, when relationship avoidance and pair history are available;
- `score_balance_score`, `height_preference_score`, and `vision_preference_score`, when their rules and required input fields are available;
- `diversity_score`, based on assignment differences among candidates;
- `stability_score`, based on unchanged seats versus the latest historical snapshot;
- `hard_constraint_summary`, covering fixed seats, adjacency rules, minimum distance, and assignment completeness.

Missing history, disabled rules, or insufficient fields produce `not_available` instead of an invented score. The total is a 0–100 weighted average of available dimensions using rule weights. The recommended candidate is the highest-scoring hard-valid plan, with `candidate_id` as a deterministic tie-breaker. Scores support comparison and explanation; they do not claim global optimality.

Snapshots and candidate sets are different formats, and old snapshots remain readable. When `export` receives a candidate set, it exports the recommended candidate by default or a selected ID such as `--candidate candidate_03`. HTML includes the candidate ID and total score; Excel and PNG continue to use their optional extras.

## Inputs And Rules

- Student lists support CSV; installing the `excel` extra enables `.xlsx` and `.xlsm`. Save legacy `.xls` files as `.xlsx` or CSV first.
- Classroom layouts are JSON seat-node graphs and support `enabled=false` unavailable seats.
- Rule files separate `hard` constraints from `soft` preferences.
- Built-in presets generate the same standard rules JSON; they are not a separate solver or rule format.
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
- JSON classroom layouts, rules, snapshots, candidate sets, and local project workspaces;
- eight discoverable, exportable scenario presets that can be layered with user rules;
- seat nodes and adjacency graphs;
- fixed seats, must-adjacent, cannot-adjacent, and minimum-distance rules;
- vision-front, height-back, randomization, score-balance, fair-rotation, and recent-neighbor avoidance heuristic preferences;
- historical snapshot statistics, the local `history-report` fairness summary, and `pair-report` relationship-history summary;
- multi-candidate generation, explainable scoring, comparison reports, and recommended-candidate selection;
- portable relative-path project configuration with `project-init`, `project-info`, `project-validate`, `project-solve`, and `project-export`;
- HTML export, with Excel / PNG export available through the `excel` / `image` extras;
- validation preflight and conflict diagnostics, CLI, local Streamlit UI, fictional examples, pytest, and GitHub Actions.

## Privacy

- `examples/` must contain fictional data only.
- `examples/history/` contains fictional history snapshots only for fair-rotation and relationship-avoidance demos.
- Project files store paths and defaults only; they must not embed or replace private student data files.
- `outputs/`, `exports/`, `snapshots/`, `private/`, `data/`, `real_students/`, `real_classes/`, and `.env` are ignored.
- Before sharing Issues, PRs, screenshots, test data, or historical seating records, remove names, IDs, grades, notes, class names, school names, and any identifying information. Do not commit real historical seating snapshots to a public repository.
- Do not commit real candidate reports or candidate-set snapshots to a public repository; keep them under ignored paths such as `outputs/`.

Current fair rotation and relationship avoidance use heuristic scoring from historical counts. They do not guarantee absolute fairness or global optimality.

## Release

The current stable release is v0.3.0. See the [release checklist](docs/release-checklist.md) and [CHANGELOG.md](CHANGELOG.md).

## License

Apache License 2.0. See [LICENSE](LICENSE).
