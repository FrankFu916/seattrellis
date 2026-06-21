# Changelog

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
