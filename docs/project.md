# Class Project Workflow Guide

[English](project.md) · [简体中文](project.zh.md)

The **Class Project** workflow is designed for long-term, ongoing classroom management. It utilizes a lightweight JSON manifest (`seattrellis.project.json`) that manages rosters, room layouts, rules, historical snapshots, and output targets with relative paths.

---

## 🎒 1. Workspace Structure

A standard class project directory structure:

```text
my-class/
├── seattrellis.project.json   # Class project manifest
├── students.csv               # Student roster
├── classroom.json             # Classroom layout
├── rules.json                 # Seating rules and weights
├── history/                   # Historical snapshot archives
│   ├── week-01.snapshot.json
│   └── week-02.snapshot.json
└── outputs/                   # Output directory for solutions and exports
```

### Manifest Example (`seattrellis.project.json`)

```json
{
  "kind": "seattrellis_project",
  "schema_version": 1,
  "name": "Grade 10 Class 3",
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

> 🔒 **Relative Path Security**:
> All file references resolve relative to the project manifest's directory. Project files contain configuration metadata rather than sensitive student records, making them safe to share as classroom templates.

---

## ⚙️ 2. Project Subcommand Reference

SeatTrellis CLI provides a complete suite of `project-*` subcommands:

| Subcommand | Description | Example Usage |
| :--- | :--- | :--- |
| `project-init` | Initialize a project manifest in a directory with existing data files. | `seattrellis project-init --dir my-class` |
| `project-list` | Discover and list recent class projects in a directory tree. | `seattrellis project-list --root .` |
| `project-info` | Inspect project configuration and check file path status. | `seattrellis project-info --project my-class/seattrellis.project.json` |
| `project-validate`| Validate manifest syntax, input file integrity, and rule conflicts. | `seattrellis project-validate --project my-class/seattrellis.project.json --strict` |
| `project-solve` | Solve the project problem and generate candidate seating sets. | `seattrellis project-solve --project my-class/seattrellis.project.json --candidates 3` |
| `project-rotate`| Generate multi-period rotation schedules (1–20 terms). | `seattrellis project-rotate --project my-class/seattrellis.project.json --periods 4` |
| `project-edit` | Apply interactive hand-tuning operations to saved plans. | `seattrellis project-edit --project ... --operation swap:STU01:STU02` |
| `project-repair`| Re-solve conflicted students while keeping locked seats intact. | `seattrellis project-repair --project ... --lock-student STU01` |
| `project-export`| Render a saved plan to any format without re-solving. | `seattrellis project-export --project ... --format print-html` |
| `project-privacy`| Scan project files and outputs for unredacted sensitive fields. | `seattrellis project-privacy --project my-class/seattrellis.project.json` |
| `project-pack` | Package the entire class workspace into a `.seattrellis.zip` bundle. | `seattrellis project-pack --project ... --output class-backup.zip` |
| `project-restore`| Restore a packed project bundle into a target directory. | `seattrellis project-restore --bundle class-backup.zip --output-dir restored/` |

---

## 🚀 3. End-to-End Walkthrough

### Step 1: Initialize and Validate
```bash
# Initialize project manifest
seattrellis project-init --dir my-class

# Preflight check
seattrellis project-validate --project my-class/seattrellis.project.json
```

### Step 2: Solve & Compare Candidates
```bash
# Generate 3 candidate plans with a comparison report
seattrellis project-solve \
  --project my-class/seattrellis.project.json \
  --candidates 3 \
  --output outputs/candidates.json \
  --report outputs/plan-report.json
```

### Step 3: Export Print-Ready Sheets
```bash
# Export the recommended candidate as an A4 landscape public handout
seattrellis project-export \
  --project my-class/seattrellis.project.json \
  --snapshot outputs/candidates.json \
  --format print-html \
  --template public \
  --orientation landscape \
  --output outputs/wall-sheet.html
```

### Step 4: Multi-Term Rotation & Packaging
```bash
# Compute a 4-term rotation sequence
seattrellis project-rotate --project my-class/seattrellis.project.json --periods 4

# Create a portable backup
seattrellis project-pack --project my-class/seattrellis.project.json --output class_term1.seattrellis.zip
```

---

## 📖 Related Documentation

- [Quick Start Guide](quickstart.en.md)
- [Export Formats Guide](export.md)
- [Web & Desktop Workbench Guide](web.md)
