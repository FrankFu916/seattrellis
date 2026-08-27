# Quick Start

[English](quickstart.md) / [简体中文](quickstart.zh.md)

**SeatTrellis v2.0.0 is released.** This guide covers installation and the
first CLI workflow. For the product overview, see the [documentation home](index.md).

## Installation

SeatTrellis v2 is a native Rust product. End users do not need Python, Node.js,
or another runtime.

### Desktop app (recommended)

Download the v2.0.0 installer for your platform from the
[Releases](https://github.com/FrankFu916/seattrellis/releases) page:

- **macOS:** `.dmg` or `.app.tar.gz` for Apple Silicon
- **Windows:** `.msi` or NSIS installer for x64
- **Linux:** `.deb` for amd64

Verify each download against `SHA256SUMS`. Desktop bundles are unsigned by the
owner's release policy; macOS may require right-clicking the app and choosing
**Open**, and Windows may show a SmartScreen prompt. See
[Export formats](export.md) for the release and file-integrity notes relevant to
generated documents.

### CLI

Install the CLI from crates.io or use a release binary:

```bash
cargo install seattrellis
```

Run `seattrellis --help` for the complete command list and
`seattrellis doctor` to check the binary version, core API version, and
temporary-directory writability.

### Browser workbench

`seattrellis_web` starts a local server bound to the loopback address
(`127.0.0.1:8765` by default) and opens the React workbench in a browser:

```bash
seattrellis_web --open-browser
# or, from a source checkout:
cargo run -p seattrellis_web -- --open-browser
```

The desktop app starts the same server and loads the workbench in a native
window. Never expose the local server to an untrusted network.

### v1 Python line (legacy)

The v1 Python line is frozen at **1.9.0** on the `v1.x-maintenance` branch:

```bash
pip install seattrellis==1.9.0
```

Use v2 for new work. v1 receives no new features and exists only as a frozen
compatibility package.

## First solve (CLI)

The standalone CLI consumes one problem JSON (`CoreSolveRequest`) containing
the students, seats, hard constraints, and solver settings. For example,
`problem.json` can contain:

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

Validate, solve, and render the saved result:

```bash
# Validate without running the search
seattrellis validate --problem problem.json

# Solve and write the complete response
seattrellis solve --problem problem.json --output plan.json

# Render the saved response as a PNG
seattrellis export \
  --problem problem.json \
  --solution plan.json \
  --format png \
  --output outputs/plan.png
```

Standalone `export` supports seven formats: `svg`, `html`, `png`, `pdf`,
`xlsx`, `docx`, and `pptx`. The eighth format, `print-html`, is available from
`project-export` and is designed for printing. `solve` uses the frozen v2
status/exit table: `Solved`/`0`, `InvalidInput`/`2`, `ProvenInfeasible`/`3`,
`Timeout`/`4`, `Unknown`/`5`, `InternalError`/`70`, and `Cancelled`/`130`.

## Demo data

The repository's `examples/` directory contains fictional data only, including
`students.csv`, `classroom.json`, `rules.json`,
`rules_multi_candidate.json`, `rules_neighbor_avoidance.json`, `history/`, and
`project.seattrellis.json`. The v2 CLI does not create demo files; use the
checked-in examples or provide your own private inputs.

## Scenario presets

`validate --preset <name>` checks whether the problem has the optional history,
score, height, or vision data expected by a scenario and emits warnings. It does
not merge preset rules in the standalone CLI. The built-in scenario names are
`random`, `exam`, `daily`, `fair-rotation`, `neighbor-aware`, `balanced`,
`peer-mixing`, `score-high-front`, `score-high-back`, `row-score-balanced`,
`group-score-balanced`, `mentor-pairing`, `height-aware`, and
`vision-friendly`:

```bash
seattrellis validate \
  --problem problem.json \
  --preset daily \
  --history-dir examples/history
```

Use `--strict` when a warning should fail validation. Presets are a convenience
layer over a `RuleSet`; see [Rules](rules.md) for the full behavior.

## Solving options

```bash
# A fixed seed makes a completed fixed-budget run reproducible
seattrellis solve \
  --problem problem.json \
  --seed 42 \
  --output outputs/latest.snapshot.json

# A wall-clock budget may stop the search; a valid incumbent is still Solved
seattrellis solve \
  --problem problem.json \
  --time-limit 3 \
  --output outputs/latest.snapshot.json
```

## Validation and inspection

```bash
seattrellis validate --problem problem.json
seattrellis precheck --problem problem.json
seattrellis audit --problem problem.json --solution plan.json
seattrellis score \
  --problem problem.json \
  --assignment '[[0,0],[1,1],[2,2],[3,3]]'
```

`validate` checks input shape and obvious conflicts. `precheck` reports each
student's candidate seat domain and infeasibility causes. `audit` rechecks the
hard rules and prints the soft-score breakdown. `score` evaluates a fixed
index-pair assignment without running a search.

## Multiple candidates

```bash
seattrellis candidates \
  --problem problem.json \
  --count 5 \
  > outputs/candidates.json
```

`candidates` generates up to 20 distinct hard-valid plans (five by default).
Each candidate has an assignment, total score, and score breakdown. The
recommended candidate is the highest weighted-total hard-valid plan. Candidate
generation is heuristic and does not promise to enumerate every feasible plan
or find a global optimum.

## History reports

`history-report` summarizes each student's front, back, side, corner,
near-window, near-door, near-platform, and near-AC counts. `pair-report`
summarizes desk-mate, horizontal, vertical, diagonal, any-adjacent, and
within-distance relationships:

```bash
seattrellis history-report \
  --problem problem.json \
  --history-dir examples/history \
  --output outputs/history-report.json

seattrellis pair-report \
  --problem problem.json \
  --history-dir examples/history \
  --top 10
```

History files are `*.snapshot.json` files. Missing history never makes a solve
fail; it makes history-dependent dimensions such as `fair_rotation`
`not_available`.

## Manual edits and repair

`edit` applies command-style changes to a saved snapshot or candidate set:

```bash
seattrellis edit \
  --snapshot outputs/latest.snapshot.json \
  --operation swap:STU001:STU002 \
  --operation lock-seat:R1C1 \
  --output outputs/edited.snapshot.json
```

Supported operations include `swap:STU001:STU002`, `move:STU003:R2C2`,
`batch-move:STU001=R1C2,STU002=R1C1`, `seat:STU003:R2C2`,
`unseat:STU004`, `lock-student:STU001`, `unlock-student:STU001`,
`lock-seat:R1C1`, and `unlock-seat:R1C1`. By default, an edited draft is
written even when it violates a hard constraint, with diagnostics. `--strict`
instead fails without writing. An operation log can be supplied through
`--operations-file`.

`repair` re-solves a bounded group while preserving locks:

```bash
seattrellis repair \
  --problem problem.json \
  --snapshot outputs/edited.snapshot.json \
  --lock-student STU001 \
  --lock-seat R4C3 \
  --affected STU002 \
  --output outputs/repaired.snapshot.json
```

`--affected` bounds the re-solve scope. `--lock-student` and `--lock-seat` keep
the selected assignments; locks stored in snapshot metadata are reused by
default. Use `--ignore-saved-locks` to ignore them.

## Project workflow

A project file keeps relative paths and defaults for a roster, layout, rules,
history directory, and outputs. It is useful for repeatable local work:

```bash
seattrellis project-init --dir my-class
seattrellis project-info --project my-class/seattrellis.project.json
seattrellis project-validate --project my-class/seattrellis.project.json
seattrellis project-solve \
  --project my-class/seattrellis.project.json \
  --candidates 3 \
  --output outputs/project.plan.json
seattrellis project-export \
  --project my-class/seattrellis.project.json \
  --snapshot outputs/project.plan.json \
  --format html \
  --output outputs/project.html
seattrellis project-rotate \
  --project my-class/seattrellis.project.json \
  --periods 4
seattrellis project-pack \
  --project my-class/seattrellis.project.json \
  --output my-class.seattrellis.zip
seattrellis project-restore \
  --bundle my-class.seattrellis.zip \
  --output-dir restored/
seattrellis project-privacy --project my-class/seattrellis.project.json
```

`project-init` expects the directory to contain the referenced roster, layout,
and rules files. `project-export` renders a saved plan and never re-solves it;
see [Project workflow](project.md) and [Export formats](export.md).

## Schema tooling

Long-lived v2 artifacts carry a schema version. List the registry, write a JSON
Schema, or migrate a supported legacy input:

```bash
seattrellis schema-list
seattrellis schema-export \
  --kind seatingsnapshot \
  --output seating-snapshot.v2.schema.json
seattrellis schema-migrate \
  --input roster-v1.json \
  --output roster-v2.json
```

The explicit v1-to-v2 migration steps currently cover student rosters, classroom
layouts, and project files. Snapshots, candidate sets, and rulesets without a
registered migration step are rejected with an explicit error. A source with a
newer schema version is never downgraded.

## Next steps

- [CLI reference](cli.md)
- [Input formats](input-format.md)
- [Rules](rules.md)
- [Web workbench](web.md)
- [Project workflow](project.md)
- [Export formats](export.md)
