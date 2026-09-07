# CLI Reference

**SeatTrellis v2.0.0 is released.** Run `seattrellis --help` for the
options supported by the installed binary. `seattrellis doctor` checks the
binary, version, core API version, and temporary-directory writability.

The CLI exposes 27 operational commands plus `help` (28 command entries):

| Command | Purpose |
| --- | --- |
| `doctor` | Check the local CLI environment |
| `validate` | Validate a `CoreSolveRequest` without searching |
| `precheck` | Report candidate seat domains and infeasibility causes |
| `audit` | Audit hard rules and the soft-score breakdown |
| `score` | Score a fixed assignment with the PlanScore breakdown |
| `candidates` | Generate a diverse candidate set |
| `history-report` | Summarize historical seat categories |
| `pair-report` | Summarize historical desk-mate and neighbor pairs |
| `repair` | Re-solve a snapshot while preserving anchors |
| `edit` | Apply manual operations to a snapshot or candidate set |
| `project-init` | Create a project file in an existing workspace |
| `project-list` | List recent projects under a root |
| `project-info` | Show project configuration and path status |
| `project-validate` | Validate a project and its referenced files |
| `project-solve` | Solve a project workspace |
| `project-export` | Render a saved project plan; never re-solves |
| `project-rotate` | Generate future seating periods |
| `project-edit` | Apply edits to a project artifact |
| `project-repair` | Re-solve a project artifact while preserving anchors |
| `project-privacy` | Scan a project for sensitive fields |
| `project-pack` | Back up a project as `.seattrellis.zip` |
| `project-restore` | Restore a project bundle |
| `schema-list` | List the v2 artifact registry |
| `schema-export` | Write a JSON Schema for one artifact kind |
| `schema-migrate` | Validate and rewrite a supported legacy artifact |
| `solve` | Solve a problem and print a result summary |
| `export` | Render a saved result as SVG/HTML/PNG/PDF/XLSX/DOCX/PPTX |
| `help` | Show command help |

`--version` / `-V` prints the CLI version. Each command accepts `--help`.

## Exit statuses

The v2 status and exit table is frozen:

| Exit code | Status | Meaning |
| ---: | --- | --- |
| `0` | `Solved` | A valid plan was produced |
| `2` | `InvalidInput` | Input or command arguments are invalid |
| `3` | `ProvenInfeasible` | Infeasibility was established |
| `4` | `Timeout` | The time limit ended the search without a valid incumbent |
| `5` | `Unknown` | The search ended without proving infeasibility or producing a plan |
| `70` | `InternalError` | An unexpected internal failure occurred |
| `130` | `Cancelled` | The process was cancelled |

Heuristic exhaustion is `Unknown`, never a false `ProvenInfeasible`. If a valid
incumbent exists when a time limit fires, the result is `Solved` (`0`). The
`solve`, `candidates`, `project-solve`, and `project-rotate` paths use the same
infeasibility classification. A Rust panic may still use the language default
exit code `101`; that code is outside the frozen application table.

## Solve

```bash
seattrellis solve \
  --problem problem.json \
  [--seed <n>] \
  [--time-limit <seconds>] \
  [--output <result.json>]
```

`--seed` overrides the problem seed. `--time-limit` is a wall-clock budget.
The summary goes to stdout; `--output` also writes the complete
`CoreSolveResponse` JSON.

## Validate, precheck, audit, and score

```bash
seattrellis validate \
  --problem problem.json \
  [--preset <name>] \
  [--history <snapshot.json>]... \
  [--history-dir <directory>] \
  [--strict]
seattrellis precheck --problem problem.json
seattrellis audit --problem problem.json --solution result.json
seattrellis score \
  --problem problem.json \
  --assignment <json> \
  [--latest-snapshot <file>] \
  [--diversity <number>]
```

`validate` checks input shape and obvious conflicts without generating a plan.
`--preset` adds missing-data warnings; it does not merge preset rules. With
`--strict`, warnings fail validation. `precheck` reports each student's
candidate seats and infeasibility causes. `audit` rechecks hard rules and
prints soft contributions. `--assignment` is an inline JSON array of
`[student_index, seat_index]` pairs.

## Candidates

```bash
seattrellis candidates \
  --problem problem.json \
  [--count <n>] \
  [--latest-snapshot <file>]
```

`--count` accepts 1-20 and defaults to 5. Every returned candidate is
hard-valid and carries an assignment, total score, and breakdown. The
recommendation is the highest weighted-total hard-valid candidate. If the
candidate space is too small, the CLI returns the distinct plans it found and
records a warning rather than duplicating a plan.

## History reports

```bash
seattrellis history-report \
  --problem problem.json \
  [--history <snapshot.json>]... \
  [--history-dir <directory>] \
  [--output <file>]
seattrellis pair-report \
  --problem problem.json \
  [--history <snapshot.json>]... \
  [--history-dir <directory>] \
  [--top <n>] \
  [--within-distance <n>]
```

`--history` is repeatable. `--history-dir` adds sorted `*.snapshot.json` files.
`history-report --output` writes a JSON report. `pair-report --top` defaults to
10 pairs, and `--within-distance` uses a Chebyshev threshold of 2 by default.

## Manual edits

`edit` applies ordered operations to a snapshot or a selected candidate:

```bash
seattrellis edit \
  --snapshot outputs/plan.json \
  --operation swap:STU001:STU002 \
  --operation lock-seat:R4C3 \
  --output outputs/edited.json
```

Supported string operations include `swap:<student>:<student>`,
`move:<student>:<seat>`, `batch-move:<student>=<seat>,...`,
`seat:<student>:<seat>`, `unseat:<student>`, `lock-student:<student>`,
`unlock-student:<student>`, `lock-seat:<seat>`, and `unlock-seat:<seat>`.
Use `--candidate <id>` for a candidate set; the default is the recommended
candidate. Use `--operations-file <file>` for a JSON operation log; file
operations run before inline `--operation` values.

By default, an edit writes a draft and reports hard-rule violations. `--strict`
fails without writing when the edited plan is invalid. Batch moves are atomic:
students and targets must be unique, and an occupied target's current occupant
must also be in the batch. Lock state is recorded in `metadata.lock_state`.

## Repair

```bash
seattrellis repair \
  --problem problem.json \
  --snapshot outputs/edited.json \
  [--affected <student>]... \
  [--lock-student <student>]... \
  [--lock-seat <seat>]... \
  [--ignore-saved-locks] \
  [--output <file>]
```

`--affected` bounds the re-solve. Related students may be added automatically
when they share a hard rule or current-seat adjacency; other seated students
are temporarily fixed. Without `--affected`, all unlocked students can move.
Saved locks are reused by default. `--ignore-saved-locks` disables that reuse.
History options can be supplied when history-dependent objectives must remain
active during repair.

## Project commands

```bash
seattrellis project-init --dir <directory>
seattrellis project-list [--root <directory>] [--limit <n>]
seattrellis project-info --project <project.json>
seattrellis project-validate --project <project.json> [--strict]
seattrellis project-solve --project <project.json> \
  [--candidates <n>] [--report <file>] [--seed <n>] [--output <file>]
seattrellis project-export --project <project.json> \
  --snapshot <saved-plan.json> [--candidate <id>] \
  [--format <format>] [--template <teacher|public>] \
  [--orientation <portrait|landscape|auto>] --output <file>
seattrellis project-rotate --project <project.json> \
  [--periods <n>] [--seed <n>] [--output <file>]
seattrellis project-edit --project <project.json> \
  [--snapshot <file>] --operation <op>... [--output <file>]
seattrellis project-repair --project <project.json> \
  [--snapshot <file>] [--affected <student>]... [--output <file>]
seattrellis project-privacy --project <project.json> [--no-include-outputs]
seattrellis project-pack --project <project.json> --output <bundle.zip> [--force]
seattrellis project-restore --bundle <bundle.zip> \
  --output-dir <directory> [--force]
```

`project-init` creates a manifest in a directory that already contains the
referenced roster, layout, and rules. `project-solve` accepts 1-20 candidates;
`project-rotate --periods` accepts 1-20 periods and defaults to 4.
`project-export` renders exactly the saved plan supplied with `--snapshot` and
never runs the solver again. It accepts `svg|html|print-html|png|pdf|xlsx|docx|pptx`.
`--template public` forces anonymization and suppresses identifying details;
`--orientation auto` uses A4 landscape for `print-html` and portrait for other
document formats. See [Project workflow](project.md).

## Schema commands

```bash
seattrellis schema-list
seattrellis schema-export --kind <kind> --output <file>
seattrellis schema-migrate \
  --input <file> \
  [--output <file> | --in-place] \
  [--dry-run]
```

The registry contains 12 v2 kinds: `student_roster`, `classroom_layout`,
`rule_set`, `seating_snapshot`, `candidate_set`, `plan_comparison`,
`history_archive`, `rotation_plan`, `editing_operation_log`, `project`,
`project_bundle_manifest`, and `export_preset`. `schema-export` requires an
output path. `schema-migrate --dry-run` validates without writing;
`--in-place` creates a unique hidden transaction backup before replacement.

Migration coverage is explicit: current v1-to-v2 transforms cover student
rosters, classroom layouts, and project files. Snapshots, candidate sets, and
rulesets without a registered transform are rejected. Newer schema versions are
never downgraded.

## Export

```bash
seattrellis export \
  --problem problem.json \
  --solution result.json \
  --format <svg|html|png|pdf|xlsx|docx|pptx> \
  --output <file> \
  [--template <public|teacher>]
```

The solution must be a `solve --output` response. An independent validator runs
before every export, and invalid plans are refused. The standalone command has
seven formats; `print-html` is available through `project-export`.

## Related documents

- [Quick start](quickstart.md)
- [Input formats](input-format.md)
- [Rules](rules.md)
- [Project workflow](project.md)
- [Export formats](export.md)
- [Versioning](versioning.md)
