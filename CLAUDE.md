# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

SeatTrellis (席序) is a privacy-first classroom seating planner — local-first, constraint-based, reproducible. Core model/data logic is CLI- and UI-agnostic.

## Build & Test Commands

```bash
# Install (see pyproject.toml for extras)
python -m pip install -e ".[dev]"            # minimal + dev
python -m pip install -e ".[all,dev]"        # full install
python -m pip install -e ".[web,excel,image]" # web UI with exports

# Run all tests
pytest

# Run a single test file
pytest tests/test_solver_rules.py

# Run a single test
pytest tests/test_solver_rules.py::test_some_function

# Run with verbose output
pytest -v tests/test_candidates.py

# CLI entrypoint
seattrellis --help
seattrellis init-demo
seattrellis solve --students examples/students.csv --layout examples/classroom.json --preset daily

# Web UI
streamlit run src/seattrellis/web/app.py

# OR-Tools solver (opt-in)
SEATTRELLIS_USE_ORTOOLS=1 seattrellis solve --students ... --layout ... --rules ...
```

## Architecture

### Package layout (`src/seattrellis/`)

- **`models/`** — Pydantic data models: Student, SeatNode, ClassroomLayout, RuleSet (HardRules/SoftRules), SeatingSnapshot, CandidatePlan/CandidateSet, SeatHistory/PairHistory, SeatTrellisProject. Pydantic v1-compatible API (`.dict()`, `.json()` used alongside v2 `.model_dump()`).
- **`solver/`** — `solve_seating()` dispatches to OR-Tools CP-SAT (when env `SEATTRELLIS_USE_ORTOOLS=1`) or deterministic fallback heuristic. `adjacency.py` builds undirected adjacency graphs from seat nodes.
- **`candidates.py`** — `generate_candidate_set()` runs repeated solves with seed stepping and exact-assignment exclusion to produce N distinct plans.
- **`scoring.py`** — Scores each candidate across 7 dimensions (fair rotation, neighbor avoidance, score balance, height, vision, diversity, stability). Unavailable dimensions are marked `not_available`, never fabricated. Weighted total (0–100), highest score is recommended.
- **`history.py`** — Builds SeatHistory and PairHistory from historical snapshots. Computes fair-rotation costs, recent-neighbor costs, position classification, and fairness/pair reports.
- **`presets.py`** — 8 built-in scenario presets (random, exam, daily, fair-rotation, neighbor-aware, balanced, height-aware, vision-friendly). Presets generate standard RuleSets; user rules JSON fields deep-merge as overlays.
- **`io/`** — CSV/Excel student reading, JSON layout/rules/snapshot/project loading, validation diagnostics.
- **`exporters/`** — HTML, Excel, PNG export of seating snapshots. Lazy imports keep heavy deps optional.
- **`web/`** — Streamlit UI with two tabs: quick solve (upload files) and project workspace (read local `.seattrellis.json`). Workflow logic is in `web/workflow.py`; rendering in `app.py`.
- **`cli.py`** — All CLI commands (Typer with argparse fallback). Core solve/validate/export logic lives in top-level functions (`solve_with_report()`, `run_validate()`, `export()`, etc.) reusable from web workflow.
- **`demo.py`** — Fictional demo input generation (8 students, 4×4 irregular classroom, 3-week history).
- **`optional.py`** — `MissingOptionalDependencyError` for soft dependency management.

### Data Flow

1. **Parse inputs** — students (CSV/Excel), layout (JSON seats + adjacency config), rules (JSON or preset), history (`.snapshot.json` files)
2. **Validate** — input file format checks, hard-rule conflict detection (fixed seats × pair rules, must-adjacent × cannot-adjacent, min-distance)
3. **Solve** — `solve_seating()` returns a `SeatingSolution` (assignments + solver metadata). CP-SAT (when enabled) or fallback greedy construction with random restarts.
4. **Multi-plan** (optional) — `generate_candidate_set()` seeds repeated solves, excludes prior full assignments, produces N `CandidatePlan`s with scores.
5. **Score** — 7 dimensions evaluated per candidate; weighted total computed; `refresh_recommendation()` picks the highest-scoring.
6. **Export** — `export_snapshot()` dispatches to HTML/Excel/PNG by format string.

### Key Design Decisions

- **Heavy deps behind extras** — `solver` (ortools), `excel` (openpyxl), `image` (Pillow), `web` (streamlit). Core solve uses fallback unless `SEATTRELLIS_USE_ORTOOLS=1`.
- **Hard vs soft constraints** — strict separation in `RuleSet.hard` / `RuleSet.soft`. Hard constraints are enforced by the solver; soft rules add objective costs or heuristic preference.
- **Local-first, privacy-aware** — no database, no network calls in solve path. Gitignored paths: `outputs/`, `private/`, `data/`, `real_*/`.
- **Seat nodes + adjacency graph** — classrooms are arbitrary seat graphs, not just rectangular matrices. Adjacency edges are built from row/col deltas or xy distance thresholds, with custom overrides.
- **Fictional data only** — `examples/` must never contain real student info. All demo IDs use `STU001`-style identifiers.
- **Pydantic v1/v2 dual-target** — code uses `hasattr(model, "model_dump")` to branch. The `validator` import in project.py uses a pydantic.v1 fallback.

### Test Strategy (`tests/`)

- Tests cover models, solver (basic, rules, adjacency), candidates, scoring, presets, CLIs (via `cli.solve()` etc.), importers/exporters, validation, history, project workflow, and optional dependency guards.
- Fixtures in `tests/fixtures/` (student CSVs, layout JSON, rules JSON) and `tests/conftest.py`.
- Use `pytest -v` for individual test files.
