# Quick Start Guide

[English](quickstart.md) · [简体中文](quickstart.zh.md)

Welcome to **SeatTrellis v2.0.0**! This guide walks you through installation, problem validation, solving, candidate comparison, and exporting seating charts in under 5 minutes.

---

## 📦 1. Installation

SeatTrellis v2 is built in pure Rust and requires no external runtimes (no Python or Node.js needed).

### Option A: Desktop Application (Recommended for Teachers)

Download the installer for your operating system from [GitHub Releases](https://github.com/FrankFu916/seattrellis/releases):

- **macOS** (Apple Silicon): `.dmg` image or `.app.tar.gz` archive.
- **Windows** (x64): `.msi` package or NSIS `.exe` installer.
- **Linux** (amd64): `.deb` package.

> 💡 **Operating System Hints**: The release binaries are distributed unsigned. On macOS, right-click the application icon in Finder and select "Open" on first launch. On Windows, if SmartScreen appears, click "More info" followed by "Run anyway".

---

### Option B: Command-Line Tool (CLI)

Install directly via Cargo:

```bash
cargo install seattrellis
```

Verify your installation and environment health:

```bash
seattrellis doctor
```
> `doctor` checks the binary version, Core API level, and temporary directory read/write permissions.

---

### Option C: Launching the Local Web Workbench

If you prefer browser-based planning, launch the lightweight local server bound exclusively to `127.0.0.1:8765`:

```bash
seattrellis_web --open-browser
```
This automatically launches the interactive React workbench in your default web browser.

---

## ⚡ 2. CLI Workflow in 3 Minutes

The CLI operates on a structured **problem definition file** (`problem.json`) combining student information, room topology, constraints, and solver settings.

### Example `problem.json`

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
  "rules": {
    "seed": 42,
    "soft": {
      "randomize": { "enabled": true, "weight": 1 }
    }
  }
}
```

### Three Core Commands

```bash
# 1. Validate problem definition without triggering the solver search
seattrellis validate --problem problem.json

# 2. Solve and write the complete seating snapshot to plan.json
seattrellis solve --problem problem.json --output plan.json

# 3. Render and export the saved solution as a high-resolution PNG
seattrellis export --problem problem.json --solution plan.json --format png --output plan.png
```

---

## 🛠️ 3. Advanced Features & Scenarios

### 3.1 Deterministic Solving & Time Limits

```bash
# Fix the random seed for 100% reproducible results
seattrellis solve --problem problem.json --seed 42 --output outputs/latest.snapshot.json

# Set a wall-clock search budget (in seconds)
seattrellis solve --problem problem.json --time-limit 3 --output outputs/latest.snapshot.json
```

---

### 3.2 Audit & Score Breakdown

```bash
# Inspect candidate seat domains and diagnose infeasibility causes
seattrellis precheck --problem problem.json

# Audit a completed plan against all hard constraints and inspect score breakdown
seattrellis audit --problem problem.json --solution plan.json

# Evaluate a specific assignment matrix on demand
seattrellis score --problem problem.json --assignment '[[0,0],[1,1],[2,2],[3,3]]'
```

---

### 3.3 Multi-Candidate Generation

Generate multiple diverse, hard-valid candidate arrangements for comparison:

```bash
seattrellis candidates --problem problem.json --count 5 > outputs/candidates.json
```
The solver ranks candidates by weighted preference score, marking the recommended choice while reporting stability and diversity metrics.

---

### 3.4 Interactive Hand-Tuning & Local Repair

Adjust individual placements with the `edit` command:

```bash
seattrellis edit \
  --snapshot outputs/latest.snapshot.json \
  --operation swap:STU001:STU002 \
  --operation lock-seat:R1C1 \
  --output outputs/edited.snapshot.json
```

Supported operations:
- `swap:STU001:STU002`: Swap two students.
- `move:STU003:R2C2`: Move a student to an empty seat.
- `lock-student:STU001` / `lock-seat:R1C1`: Lock a student or seat in place.
- `batch-move:STU001=R1C2,STU002=R1C1`: Atomically execute multiple moves.

If manual edits cause rule conflicts, use `repair` to intelligently re-solve only affected students while keeping locked seats untouched:

```bash
seattrellis repair \
  --problem problem.json \
  --snapshot outputs/edited.snapshot.json \
  --lock-student STU001 \
  --affected STU002 \
  --output outputs/repaired.snapshot.json
```

---

### 3.5 Historical Rotation & Pair Reports

```bash
# Generate comprehensive position-distribution statistics for each student
seattrellis history-report --problem problem.json --history-dir examples/history --output outputs/history-report.json

# Analyze recurring desk-mate and neighbor pairings across terms
seattrellis pair-report --problem problem.json --history-dir examples/history --top 10
```

---

## 🎒 4. Class Project Workflow

Manage ongoing classroom files (rosters, layouts, rules, and historical records) in a unified project structure:

```bash
# 1. Initialize a class workspace from existing data files
seattrellis project-init --dir my-class

# 2. Inspect project configuration and file references
seattrellis project-info --project my-class/seattrellis.project.json

# 3. Solve and generate candidate seating charts
seattrellis project-solve --project my-class/seattrellis.project.json --candidates 3 --output outputs/project.plan.json

# 4. Export the saved seating chart to printable HTML
seattrellis project-export --project my-class/seattrellis.project.json --snapshot outputs/project.plan.json --format print-html --output outputs/seat.html

# 5. Generate a fair 4-period rotation plan
seattrellis project-rotate --project my-class/seattrellis.project.json --periods 4

# 6. Create portable backups and restore them across machines
seattrellis project-pack --project my-class/seattrellis.project.json --output my-class.seattrellis.zip
seattrellis project-restore --bundle my-class.seattrellis.zip --output-dir restored/
```

---

## 📖 Deep Dives

- 📐 **[Rule Handbook](rules.en.md)**: Explore hard constraints, weighted soft preferences, and solver mechanics.
- 🖥️ **[Web Workbench Guide](web.en.md)**: Visual classroom editing, seat swapping, and export options.
- 📄 **[Input Formats Reference](input-format.en.md)**: JSON and CSV schemas for rosters, layouts, and snapshots.
- ⚙️ **[CLI Reference Manual](cli.md)**: Comprehensive reference for all 27 subcommands and exit codes.
