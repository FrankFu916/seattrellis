# Changelog

## 0.3.1 - 2026-06-27

### Fixed
- Fixed fallback solver vision-front and height-back preferences being neutralized by incorrect row bounds (`_fallback_individual_cost` now uses actual seat-row range instead of per-seat bounds).
- Fixed `_score_recent_neighbors` using all seats (`layout.seats`) instead of only enabled seats (`layout.enabled_seats`), consistent with other scoring dimensions.
- Fixed duplicate `_needs_front` logic — consolidated into `student_needs_front()` on the Student model, eliminating the maintenance risk of two identical copies.
- Fixed `classify_seat_position` crash on layouts with zero enabled seats (added empty-seats guard).

### Improved
- Added duplicate student-key detection to file-level validation so the error surfaces at validate time rather than only at solve time.
- Added comprehensive test coverage for scoring module (48 new tests covering all 7 dimensions, edge cases, hard-constraint evaluation, diversity scoring, recommendation logic, and `student_needs_front`).
- Removed unused `ProjectPaths` import in `cli.py` and unused `classify_seat_position` import in `scoring.py`.
- Added CLAUDE.md for repository-level developer guidance.

## 0.3.0 - 2026-06-26

### Added
- Added a fuller local Streamlit workflow for presets, optional rules overlays, history snapshot uploads, and 1-20 generated candidates.
- Added web display for the recommended candidate, score breakdowns, hard-rule checks, candidate warnings, assignment rows, and JSON/report/export downloads.
- Added a web project workspace flow that reuses `project-info`, `project-validate`, `project-solve`, and `project-export`.
- Added a Streamlit-free web workflow helper layer so the UI can reuse existing CLI/core behavior without copying solver, scoring, preset, or project logic.
- Added web workflow tests and a Streamlit app smoke test.

### Improved
- Kept web exports on the existing snapshot/candidate export path, including friendly optional-extra errors for PNG and Excel.
- Kept project paths relative to the project file and avoided embedding private classroom data in project files.
- Updated README and the release checklist for the completed web workflow.

## 0.2.3 - 2026-06-25

### Added
- Added eight built-in rules presets: `random`, `exam`, `daily`, `fair-rotation`, `neighbor-aware`, `balanced`, `height-aware`, and `vision-friendly`.
- Added `presets list`, `presets show`, and `presets export` CLI commands with Typer and argparse support.
- Added `solve --preset` and `validate --preset`, including optional user-rules overlays where explicit user fields override preset defaults.
- Added preset metadata and graceful-degradation warnings for missing history, score, height, or vision data.
- Added portable local project workspace files with relative students, layout, rules, history, and output paths.
- Added `project-init`, `project-info`, `project-validate`, `project-solve`, and `project-export` CLI commands with Typer and argparse support.
- Added project defaults for candidate generation, candidate selection, and export format.
- Added a fictional `examples/project.seattrellis.json` workspace plus preset and project workflow tests.

### Improved
- Kept presets as a thin layer over the existing `RuleSet`, validation, solving, candidate generation, scoring, and export paths.
- Preserved absolute hard-constraint priority when presets are used alone or combined with user rules.
- Kept ordinary rules files, snapshots, candidate sets, and existing CLI commands backward compatible.
- Reused the existing validation, solving, candidate scoring, persistence, and export paths for project workflows.
- Added clear project-file, referenced-path, and output-directory diagnostics without introducing a database or new dependency.
- Updated bilingual documentation and the release checklist for v0.2.3.

## 0.2.2 - 2026-06-24

### Added
- Added multi-candidate seating generation for the fallback and OR-Tools solvers.
- Added explainable candidate scoring with fair rotation, recent-neighbor avoidance, score balance, height, vision, diversity, stability, and hard-constraint summaries.
- Added candidate-set JSON output and plan-comparison report output.
- Added recommended-candidate selection and candidate-set export support.
- Added fictional multi-candidate examples and CLI smoke coverage.

### Improved
- Improved solver output for decision-making workflows while preserving single-snapshot compatibility.
- Expanded documentation for heuristic score-based plan comparison.
- Added tests for deterministic candidate generation, scoring, hard-rule preservation, persistence, and HTML / Excel / PNG candidate export.

## 0.2.1 - 2026-06-23

### Added
- Added pair-history analysis for historical seating snapshots.
- Added an `avoid_recent_neighbors` soft rule for reducing repeated desk-mate and neighbor relationships.
- Added a `pair-report` CLI command for local relationship-history summaries.
- Added fictional examples for neighbor-history avoidance.

### Improved
- Improved history-based scoring by combining seat-category fairness with pair-history awareness.
- Expanded tests for relation detection, pair-history reports, fallback and OR-Tools scoring, and hard-rule priority.
- Updated documentation for relationship-aware seating.

## 0.2.0 - 2026-06-22

### Added
- Added historical snapshot loading for seat rotation analysis.
- Added fair rotation soft rule based on recent seat categories.
- Added seat history statistics for front, back, side, corner, and tagged seat locations.
- Added a `history-report` CLI command for local fairness summaries.
- Added fictional history examples.

### Improved
- Improved snapshot metadata for fairness-related runs.
- Updated documentation for history-based seating and fair rotation.
- Expanded tests for historical seating behavior.

## 0.1.2 - 2026-06-21

### Added
- Added a `validate` command for input and rule preflight checks.
- Added stronger validation for students, classroom layouts, and rules.
- Added clearer hard-constraint conflict diagnostics.
- Added small fictional invalid examples for common validation failures.

### Improved
- Improved CLI error messages for invalid files and infeasible seating plans.
- Expanded tests for invalid inputs and conflicting rules.
- Updated documentation for validation behavior.

## 0.1.1 - 2026-06-21

### Improved
- Split heavy dependencies into optional extras.
- Kept the core package lighter for CLI and fallback-solver usage.
- Added lazy imports for optional solver, Excel, image, and web features.
- Improved missing-extra error messages.
- Added minimal-install and full-feature test coverage.
- Updated README and documentation to match the dependency model.

## 0.1.0 - 2026-06-20

Initial open-source MVP:

- Local-first classroom seating workflow with fictional demo data.
- Pydantic models for students, seat-node classroom layouts, rules, and snapshots.
- CSV and `.xlsx` student import with validation.
- JSON layout, rules, and portable snapshot files.
- Hard rules for fixed seats, adjacency, non-adjacency, and minimum distance.
- Soft preferences for vision-front, height-back, reproducible randomization, and score balance.
- Deterministic fallback solver, with optional OR-Tools CP-SAT support via the `solver` extra.
- CLI commands for demo generation, solving, and Excel / PNG / HTML export.
- Local Streamlit web UI.
- Pytest coverage and GitHub Actions CI.
