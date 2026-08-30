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

## 📄 License

Licensed under [Apache-2.0](../../LICENSE).
