---
slug: /
---

# SeatTrellis Documentation

[English](index.md) / [简体中文](index.zh.md)

**SeatTrellis v2.0.0 is released.** SeatTrellis is a local-first classroom
seating planner: import a roster, describe the room and constraints, generate
one or more plans, adjust them, and export a handout.

## Choose an entry point

- **Desktop app:** the recommended Tauri application for teachers. It includes
  the workbench and runs the local Rust service in a native window.
- **Web workbench:** the same React workflow in a browser, served by
  `seattrellis_app` on the loopback interface only.
- **CLI:** `seattrellis_cli` for automation, reproducible solves, project
  folders, reports, migrations, and exports.

## Quick links

| Start here | What it covers |
| --- | --- |
| [Quick start](quickstart.md) | Install v2.0.0, solve a first problem, and export it |
| [Web workbench](web.md) | Teacher workflow, editing, projects, and downloads |
| [CLI reference](cli.md) | Commands, options, statuses, and exit codes |
| [Input formats](input-format.md) | Rosters, layouts, projects, history, and candidate artifacts |
| [Rules](rules.md) | Hard constraints, soft objectives, presets, and scoring |
| [Project workflow](project.md) | Persistent local workspaces and saved plans |
| [Export formats](export.md) | Eight renderers, templates, privacy, and printing |

Developer documentation starts with [Architecture](architecture.md). See
[Privacy](privacy.md) before using real student data.

## How plans are evaluated

**Hard constraints** are requirements. Fixed seats, required or forbidden
adjacency, minimum distances, and group relationships must pass validation; a
plan that cannot satisfy them is not presented as a valid solution.

**Soft objectives** are weighted preferences. Vision, height, score placement or
mixing, history-based rotation, and recent-neighbor avoidance improve the score
when the input supports them, but never override a hard constraint. A missing
input makes the affected dimension `not_available`, not an invented zero score.

With a fixed seed, the Rust solver is reproducible when it completes its fixed
search budget. A wall-clock timeout can stop machines after different numbers
of attempts, so timed-out runs are not promised to be byte-identical.

## Local-first privacy

The v2.0.0 desktop app, browser workbench, and CLI process data on the local
machine. The app server binds to `127.0.0.1` by default, uses a per-process
session token, and is not designed to be exposed to a LAN or an untrusted
network. There are no accounts, cloud sync, or product telemetry.

Public exports fail closed: they anonymize student labels and suppress student
IDs and sensitive detail fields. Keep real rosters, history, screenshots, and
exports in private ignored directories; the repository's examples are fictional.

## Legacy v1 line

The Python line is frozen at **1.9.0** on the `v1.x-maintenance` branch. It is a
legacy compatibility package only (`pip install seattrellis==1.9.0`) and is not
part of the v2.0.0 runtime or release artifacts.
