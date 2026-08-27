# Project Workflow

[English](project.md) / [简体中文](project.zh.md)

**SeatTrellis v2.0.0 is released.** A project is a small local JSON manifest
that keeps the paths and defaults for a seating workspace. It does not embed the
roster or seating data.

## Commands

```bash
seattrellis project-init       # create a manifest in an existing workspace
seattrellis project-list       # list recent projects under a root
seattrellis project-info       # show configuration and path status
seattrellis project-validate   # validate the manifest and referenced files
seattrellis project-solve      # solve, optionally with candidates/report
seattrellis project-rotate     # generate future seating periods
seattrellis project-edit       # apply manual edit operations
seattrellis project-repair     # re-solve while preserving anchors
seattrellis project-export     # render a saved plan; never re-solves
seattrellis project-privacy    # scan for sensitive fields
seattrellis project-pack       # create a .seattrellis.zip backup
seattrellis project-restore    # restore a bundle into a directory
```

`project-init --dir <directory>` expects `students.csv`, `layout.json`, and
`rules.json` or equivalent files to already exist. It creates
`seattrellis.project.json`. `project-list` scans the current directory by
default and lists the 20 most recent projects; use `--root` and `--limit` to
change the search.

## Project file

```json
{
  "kind": "seattrellis_project",
  "schema_version": 1,
  "name": "Demo Class",
  "students": "students.csv",
  "layout": "classroom.json",
  "rules": "rules.json",
  "history_dir": "history",
  "outputs_dir": "outputs",
  "default_candidates": 5,
  "default_candidate": "recommended",
  "default_export_format": "html"
}
```

`students`, `layout`, and `rules` are required. `history_dir` is optional;
`outputs_dir` defaults to `outputs`, `default_candidates` to `5`, and
`default_candidate` to `recommended`. Project defaults accept `html`, `excel`,
or `png`; `project-export --format` can explicitly select any of the eight
export formats described in [Export formats](export.md).

All references must be relative to the directory containing the project file.
Absolute paths, traversal, and symlink escapes are rejected. Moving a project
means moving the manifest and its referenced files while preserving the relative
directory structure.

## Web and CLI

The CLI is suited to scripts and repeatable local runs. The web workbench can
use a project path or accept an uploaded project JSON. An uploaded manifest is
only one JSON file; the browser cannot access the relative files it names. Use
path mode for validation, solving, and export.

The project file does not contain the roster, historical snapshots, or exports.
The Project panel shows metadata and privacy-safe summaries, and can create a
`.seattrellis.zip` backup or restore one locally.

## Validation and output

`project-info` displays resolved path status. `project-validate` checks the
manifest, referenced files, and rule conflicts. `project-solve` writes a saved
plan or candidate set to `outputs_dir`; `--candidates`, `--seed`, `--report`, and
output options override project defaults.

`project-edit` adjusts a saved artifact. `project-repair` re-solves while
preserving saved locks and explicit anchors. `project-export` reads the plan
passed through `--snapshot`, selects the requested candidate when needed, and
renders it without running the solver again:

```bash
seattrellis project-solve \
  --project my-class/seattrellis.project.json \
  --candidates 3 \
  --output outputs/candidates.json

seattrellis project-export \
  --project my-class/seattrellis.project.json \
  --snapshot outputs/candidates.json \
  --candidate candidate_02 \
  --format print-html \
  --template public \
  --output outputs/wall-copy.html
```

`project-rotate --periods N` generates between 1 and 20 sequential periods (4
by default). `project-pack` and `project-restore` apply manifest, path, and
bundle safety checks. `project-privacy` scans the project and, by default, its
outputs before sharing.

## Migration

The current artifact registry uses v2 envelopes. `schema-migrate` has explicit
v1-to-v2 transforms for student rosters, classroom layouts, and project files;
other artifact kinds are rejected when no typed migration step exists. Migration
previews and writes create a backup before replacing an existing source.

## Related documents

- [Quick start](quickstart.md)
- [Web workbench](web.md)
- [Input formats](input-format.md)
- [Export formats](export.md)
