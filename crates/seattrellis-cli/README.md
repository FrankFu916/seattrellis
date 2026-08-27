# seattrellis

Native command-line solver and exporter for [SeatTrellis](https://github.com/FrankFu916/seattrellis) (席序) classroom seating. The release binary is dependency-free and works offline.

## Commands

- `solve` / `validate` / `precheck` / `audit` / `score` / `candidates` — solve a JSON problem, check inputs, diagnose feasibility, audit a plan, score a fixed assignment, generate candidate sets
- `edit` / `repair` — apply manual operations to a snapshot or repair a constrained plan (saved-lock aware)
- `history-report` / `pair-report` — summarize historical snapshots
- `project-*` — project workspace lifecycle: init, list, info, validate, solve, export, rotate, edit, repair, privacy, pack, restore
- `schema-list` / `schema-export` / `schema-migrate` — the 12-kind v2 artifact registry, generated JSON Schemas, and v1→v2 migration
- `export` — render a solved plan as SVG / HTML / print-HTML / PNG / PDF / XLSX / DOCX / PPTX

Exit codes follow the frozen contract: 0 solved, 2 invalid input, 3 proven infeasible, 4 timeout, 5 unknown, 70 internal error, 130 cancelled.

## Install

```bash
cargo install seattrellis
# or use the prebuilt binaries from GitHub Releases
```

## Example

```bash
seattrellis solve --problem problem.json --output plan.json
seattrellis export --problem problem.json --snapshot plan.json --format png --output plan.png
```

License: Apache-2.0.
