# Roadmap

> **Status:** SeatTrellis v2.0.0 is released. The v1 Python line is frozen at
> 1.9.0 on `v1.x-maintenance`; it is not an active product roadmap.

This page separates shipped v2.0.0 capabilities from possible future work. A
future item is not a release commitment until it has an approved milestone.

## v2.0.0 shipped

The released Rust line includes:

- a native `seattrellis_cli` for solving, validation, scoring, candidates,
  reports, manual edits, repair, projects, schema migration, and export;
- a loopback-only `seattrellis_app` server with a React workbench and a Tauri 2
  desktop shell;
- one Rust implementation of rule compilation, hard-rule validation, scoring,
  editing, migration, privacy, and solver status semantics;
- teacher workflows for roster import, room templates, custom layouts, common
  goals, detailed rules, candidate comparison, history, rotation, and manual
  adjustment;
- eight local export formats: SVG, HTML, `print-html`, PNG, PDF, XLSX, DOCX,
  and PPTX;
- public and teacher export templates with fail-closed privacy defaults;
- project backup, restore, privacy scanning, artifact comparison, and explicit
  migration previews;
- committed Rust fixtures, CLI goldens, browser E2E, fuzz targets, candidate
  and rotation gates, and a Rust solver performance regression gate.

See [Architecture](architecture.md), [Testing](testing.md), and
[Publishing](publishing.md) for the implementation and release boundaries.

## Possible post-v2 work

These are intentionally small, user-facing follow-ups rather than a promise to
add a second solver or a second domain model:

- drag-and-drop, box selection, and richer batch layout editing;
- more detailed visual diffs for plans and history comparisons;
- additional group-register fields and classroom reporting options;
- signed desktop bundles, notarization, and a clean-machine installation matrix;
- continued accessibility, keyboard, narrow-screen, and export-layout polish;
- performance tuning based on the committed Rust baseline and real workloads.

Any such work must preserve the local-first boundary, shared Rust semantics,
independent artifact validation, and the seven-state solver contract.

## Frozen v1 roadmap (historical)

The former public roadmap covered v1.3 through v1.8. Those entries describe the
sequence that led to v2 and are retained for context only:

| Version | Historical focus | Outcome |
| --- | --- | --- |
| v1.3.0 | Export privacy, A4 settings, and bilingual output | Superseded by the v2 export layer |
| v1.4.0 | Backend boundary, candidates, benchmarks, and editor protocol | Superseded by the native Rust core |
| v1.5.0 | Simplified teacher workflow and standard rooms | Superseded by the React workbench |
| v1.6.0 | React visual editor and import mapping | Superseded by the v2 workbench |
| v1.7.0 | Projects, history, groups, and future rotation | Shipped in the v2 project workflow |
| v1.8.0 | Desktop packaging and formal distribution | Shipped as the v2 Tauri distribution |
| v1.9.0 | Maintenance baseline | Frozen legacy line |

The Python service, Streamlit entry point, Python fallback solver, OR-Tools
integration, and PyO3 compatibility extension are not v2 components. The
migration-era comparison infrastructure was removed after v2.0.0; the current
regression contract is Rust tests and frozen inputs, not a live Python oracle.

## Product principles

### Start from the teacher's task

The default flow is:

> Open a class -> import a roster -> set up the room -> choose a goal ->
> generate -> adjust -> review -> save and export

The UI presents a class as the main user concept. Project files, snapshots, and
JSON remain available as compatible technical artifacts without requiring every
teacher to understand them.

### Simple by default, detailed on demand

The ordinary flow shows class and data status, a room preview, one goal, a
hard-constraint summary, generate, the seating map, undo/redo, save, and export.
Advanced settings expose candidate count, seed, time limit, history quality,
raw rules JSON, schema details, and export privacy controls.

### One domain model, multiple surfaces

The CLI, browser, and desktop shell share Rust application services and export
renderers. React submits versioned commands and DTOs; it does not implement a
second set of constraint checks or an independent editing state machine.

### Hard requirements before preferences

Hard constraints always take priority over soft objectives. Missing data makes a
soft dimension `not_available`; it does not invent a score. Every accepted solve,
edit, repair, rotation, and export artifact is independently validated.

## Current maintenance boundary

v2.0.0 uses Rust as its only application runtime. Node.js is needed to build the
React frontend but is not needed by a release binary. The Python 1.9.0 package is
maintained only on `v1.x-maintenance` and receives no v2 feature work.

The current quality boundary is:

- Rust unit, integration, property-style, and fuzz tests;
- committed CLI output goldens and fixture inputs;
- browser E2E for import, generation, editing, privacy, projects, and export;
- release-mode candidate/rotation gates and the committed solver baseline;
- no live Python oracle, parity harness, or differential command.

## Not on the roadmap

Accounts, cloud synchronization, telemetry, plugins, and a remote AI assistant
are outside the current product boundary. Any future proposal must first preserve
offline use and the rule that real student data stays on the user's machine.
