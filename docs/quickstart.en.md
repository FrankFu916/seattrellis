# Quick Start

This document covers installation and CLI usage for SeatTrellis v2 (the Rust-only line). For a brief project overview, see the [documentation home](/).

## Installation

SeatTrellis v2 is a native Rust tool. It does not require Python, Node.js, or any other runtime.

### Desktop app (recommended)

Download the installer for your platform from the [Releases](https://github.com/FrankFu916/seattrellis/releases) page:

- **Windows**: MSI or NSIS installer (x64)
- **macOS**: DMG or app archive (Apple Silicon)
- **Linux**: DEB package

Verify every download against `SHA256SUMS` before installing.

### CLI

```bash
cargo install seattrellis_cli
# or use the prebuilt binaries from Releases
```

After installation, run `seattrellis_cli --help` for the full command list, and `seattrellis_cli doctor` to check the environment (binary version, core API version, writable temp directory).

### Browser workbench

`seattrellis_app` starts a local server bound to the loopback address only (`127.0.0.1:8765`) and opens the React workbench in your browser:

```bash
cargo run -p seattrellis_app -- --open-browser
# or run the prebuilt binary from Releases
seattrellis_app --open-browser
```

The desktop app (Tauri shell) starts the same server and loads the workbench in a native window.

### v1 Python line (legacy)

The v1 (Python) line is frozen at **1.9.0** and maintained on the `v1.x-maintenance` branch. The legacy package is still installable:

```bash
pip install seattrellis==1.9.0
```

New users should use the v2 desktop app or the Rust CLI; v1 exists only as a frozen compatibility package and receives no new features.

## Quick start (CLI)

The v2 CLI works on a single "problem file" (a `CoreSolveRequest` JSON) that holds the students, seats, rules, and solver settings in one place. Example `problem.json`:

```json
{
  "api_version": 2,
  "student_count": 4,
  "seat_positions": [[0.0, 0.0], [1.0, 0.0], [0.0, 1.0], [1.0, 1.0]],
  "edges": [[0, 1], [2, 3], [0, 2], [1, 3]],
  "fixed_seats": [[0, 0]],
  "seed": 42,
  "students": [
    {"key": "STU001", "display_name": "Alice"},
    {"key": "STU002", "display_name": "Bob"},
    {"key": "STU003", "display_name": "Carol"},
    {"key": "STU004", "display_name": "Dave"}
  ],
  "rules": {"seed": 42, "soft": {"randomize": {"enabled": true, "weight": 1}}}
}
```

The three core commands:

```bash
# Validate the problem file without running the search
seattrellis_cli validate --problem problem.json

# Solve and write the full result to plan.json
seattrellis_cli solve --problem problem.json --output plan.json

# Render the saved plan as PNG
seattrellis_cli export --problem problem.json --solution plan.json --format png --output plan.png
```

`export` supports `svg`, `html`, `print-html` (project-export only), `png`, `pdf`, `xlsx`, `docx`, and `pptx`. `solve` uses the frozen v2 exit table: `0` solved, `2` invalid input, `3` proven infeasible, `4` timeout, `5` unknown, `70` internal error, `130` cancelled.

## Demo data

The repository's `examples/` directory carries fictional data only: `students.csv`, `classroom.json`, `rules.json`, `rules_multi_candidate.json`, `rules_neighbor_avoidance.json`, `history/`, and `project.seattrellis.json`. The v2 CLI has no `init-demo` command; grab the sample files from the repository instead.

## Built-in scenario presets

`validate`'s `--preset` flag checks which preferred data (history, score, height, vision) the problem is missing for a scenario and warns accordingly. Built-in presets include `random`, `exam`, `daily`, `fair-rotation`, `neighbor-aware`, `balanced`, `peer-mixing`, `score-high-front`, `score-high-back`, `row-score-balanced`, `group-score-balanced`, `mentor-pairing`, `height-aware`, and `vision-friendly`:

```bash
seattrellis_cli validate --problem problem.json --preset daily --history-dir examples/history
```

`--strict` turns warnings into failures. Presets are a convenience layer over rule JSON; see [Rules](rules.en.md) for the full rule reference.

## Solving

```bash
# Fixed seed for reproducible results
seattrellis_cli solve --problem problem.json --seed 42 --output outputs/latest.snapshot.json

# Wall-clock budget; an exhausted budget reports Timeout (exit 4),
# while a valid incumbent still reports Solved
seattrellis_cli solve --problem problem.json --time-limit 3 --output outputs/latest.snapshot.json
```

## Validation and inspection

```bash
# Validate the problem (student/seat/rule references, fixed-seat and adjacency conflicts)
seattrellis_cli validate --problem problem.json

# Precheck: candidate seat domains and infeasibility reasons
seattrellis_cli precheck --problem problem.json

# Audit a solved plan: hard-rule status + soft breakdown
seattrellis_cli audit --problem problem.json --solution plan.json

# Score a fixed assignment with the PlanScore breakdown
seattrellis_cli score --problem problem.json --assignment '[[0,0],[1,1],[2,2],[3,3]]'
```

## Multi-candidate generation

```bash
seattrellis_cli candidates --problem problem.json --count 5 > outputs/candidates.json
```

`candidates` generates up to N distinct plans (1–20, default 5) that all satisfy every hard constraint. Each candidate carries its own snapshot, total score, and score breakdown; the recommended plan is the highest-scoring hard-valid candidate.

## History analysis

`history-report` summarises each student's front, back, side, corner, near-window, near-door, near-platform, and near-AC counts. `pair-report` summarises pair-level desk-mate, horizontal, vertical, diagonal, any-adjacent, and within-distance counts:

```bash
seattrellis_cli history-report --problem problem.json --history-dir examples/history --output outputs/history-report.json
seattrellis_cli pair-report --problem problem.json --history-dir examples/history --top 10
```

History snapshots are `*.snapshot.json` files (for example under `examples/history/`). Missing history never fails a solve; it only makes dimensions such as `fair_rotation` report `not_available`.

## Manual edits and local repair

`edit` applies command-style adjustments to a saved snapshot or candidate set:

```bash
seattrellis_cli edit \
  --snapshot outputs/latest.snapshot.json \
  --operation swap:STU001:STU002 \
  --operation lock-seat:R1C1 \
  --output outputs/edited.snapshot.json
```

Supported operations: `swap:STU001:STU002`, `move:STU003:R2C2`, `batch-move:STU001=R1C2,STU002=R1C1`, `seat:STU003:R2C2`, `unseat:STU004`, `lock-student:STU001`, `unlock-student:STU001`, `lock-seat:R1C1`, `unlock-seat:R1C1`. By default the command writes a draft and prints hard-constraint diagnostics; with `--strict`, a hard-constraint violation fails the command without writing. Operation groups can also be stored in a JSON file and replayed via `--operations-file`.

`repair` re-solves a small group while preserving locked seats:

```bash
seattrellis_cli repair \
  --problem problem.json \
  --snapshot outputs/edited.snapshot.json \
  --lock-student STU001 \
  --lock-seat R4C3 \
  --affected STU002 \
  --output outputs/repaired.snapshot.json
```

`--affected` bounds the re-solve scope; `--lock-student` / `--lock-seat` keep current seats. Locks saved in the snapshot metadata are reused by default; pass `--ignore-saved-locks` to ignore them.

## Project workflow

A project file organises a local workspace by storing relative paths and defaults for the student list, layout, rules, and history directory. It suits the v1-style file workflow and long-term storage:

```bash
# Create a project file inside a directory that already has students.csv / layout.json / rules.json
seattrellis_cli project-init --dir my-class

# Inspect configuration and path status
seattrellis_cli project-info --project my-class/seattrellis.project.json

# Validate
seattrellis_cli project-validate --project my-class/seattrellis.project.json

# Solve and write the saved plan
seattrellis_cli project-solve --project my-class/seattrellis.project.json --candidates 3 --output outputs/project.plan.json

# Export the saved plan (never re-solves)
seattrellis_cli project-export --project my-class/seattrellis.project.json --snapshot outputs/project.plan.json --format html --output outputs/project.html

# Rotation: generate future periods
seattrellis_cli project-rotate --project my-class/seattrellis.project.json --periods 4

# Backup and restore
seattrellis_cli project-pack --project my-class/seattrellis.project.json --output my-class.seattrellis.zip
seattrellis_cli project-restore --bundle my-class.seattrellis.zip --output-dir restored/

# Privacy scan
seattrellis_cli project-privacy --project my-class/seattrellis.project.json
```

`project-edit` / `project-repair` reuse the same semantics as `edit` / `repair`. See [Project workflow details](project.zh.md) for more.

## Schema tooling

Every v2 artifact (snapshot, candidate set, project, rotation plan, ...) carries a `schema_version`. List the registry, export JSON Schemas, and migrate older files:

```bash
seattrellis_cli schema-list
seattrellis_cli schema-export --kind seatingsnapshot --output seating-snapshot.v2.schema.json
seattrellis_cli schema-migrate --input v1-project.json --output v2-project.json
seattrellis_cli schema-migrate --input v1-rules.json --in-place   # creates a .bak backup first
```

v1-era files (CSV rosters, layout/rules JSON, snapshots, candidate sets, projects) are handled by the v2 migration path, with automatic backups before each migration.

## Next Steps

- [CLI reference](cli.md)
- [Input Formats](input-format.en.md)
- [Rules](rules.en.md)
- [Web UI Guide](web.en.md)
- [Project Workflow Details](project.zh.md)
- [Export Formats](export.zh.md)
