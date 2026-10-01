# seattrellis (CLI)

Native command-line interface and automation tool for [SeatTrellis (席序)](https://github.com/FrankFu916/seattrellis). Built in pure Rust with zero runtime dependencies.

---

## 🚀 Key Commands

- **Solving & Auditing**: `solve`, `validate`, `precheck`, `audit`, `score`, `candidates`
- **Interactive Editing & Repair**: `edit`, `repair` (anchor-aware local solving)
- **History & Pair Analytics**: `history-report`, `pair-report`
- **Class Project Lifecycle**: `project-init`, `project-list`, `project-info`, `project-validate`, `project-solve`, `project-export`, `project-rotate`, `project-edit`, `project-repair`, `project-privacy`, `project-pack`, `project-restore`
- **Schema & Migration**: `schema-list`, `schema-export`, `schema-migrate`
- **Multi-Format Export**: `export` (SVG, HTML, print-HTML, PNG, PDF, XLSX, DOCX, PPTX)

---

## 📦 Installation

```bash
cargo install seattrellis
# or download prebuilt binaries from GitHub Releases
```

---

## 💡 Quick Example

```bash
# 1. Solve a seating problem
seattrellis solve --problem problem.json --output plan.json

# 2. Export the chart as a high-resolution PNG
seattrellis export --problem problem.json --solution plan.json --format png --output plan.png
```

---

## Project artifacts

Project commands honor `outputs_dir`, `default_candidates`,
`default_candidate`, and all eight export formats. `excel` remains an alias
for `xlsx`. A single candidate saves `latest.snapshot.json`; multiple
candidates save `latest.candidates.json`. Edit, repair, and export can open
the latest saved artifact without an explicit `--snapshot`.

Saved project artifacts include the full roster, layout, rules and original
solve request. Editing and repair preserve provenance and student/seat locks
across subsequent commands. A comparison report requested with `--report`
is committed in the same file transaction as its candidate set.

`schema-migrate --dry-run` validates both legacy inputs and current v2
envelopes without creating or replacing files. Legacy JSON rosters and
layouts need no `kind` field. Migrated roster/layout/project files remain
usable by project commands.

## 📄 License

Licensed under [Apache-2.0](../../LICENSE).
