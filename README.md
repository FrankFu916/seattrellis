<div align="center">
  <img src="docs/assets/logo.svg" width="128" alt="SeatTrellis logo" />

  # **SeatTrellis**

  **Classroom seating arrangements made scientific, fair, and effortless.**

  A privacy-focused, local-first intelligent seating arrangement tool.<br />
  Import rosters, configure rules, solve in seconds, fine-tune interactively, and export or print.<br />
  **No accounts, no cloud sync — student data stays strictly on your machine.**

  [![Tests](https://github.com/FrankFu916/seattrellis/actions/workflows/tests.yml/badge.svg)](https://github.com/FrankFu916/seattrellis/actions/workflows/tests.yml)
  [![Rust](https://github.com/FrankFu916/seattrellis/actions/workflows/rust.yml/badge.svg)](https://github.com/FrankFu916/seattrellis/actions/workflows/rust.yml)
  [![Release](https://img.shields.io/github/v/release/FrankFu916/seattrellis)](https://github.com/FrankFu916/seattrellis/releases)
  [![License](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)
  [![Platform](https://img.shields.io/badge/platform-macOS%20%7C%20Windows%20%7C%20Linux-lightgrey)](https://github.com/FrankFu916/seattrellis/releases)

  [📥 Download Desktop App](https://github.com/FrankFu916/seattrellis/releases) · `cargo install seattrellis`

  [Latest Release](https://github.com/FrankFu916/seattrellis/releases) · [Quick Start](docs/quickstart.md) · [Rule Reference](docs/rules.md) · [简体中文](README.zh-CN.md)
</div>

---

Arranging classroom seating is an intricate, recurring challenge for every teacher:
- Near-sighted students need front seats; tall students shouldn't block the board.
- Specific peers benefit from study partnerships, while others distract each other and must be separated.
- Fair rotation across terms is essential to prevent students from being stuck in corners.
- Manual planning easily takes hours every term, yet explaining the rationale to parents or administrators remains difficult.

**SeatTrellis solves this in a single click**: define your educational requirements and preferences, and the solver delivers mathematically verifiable, fully explainable seating plans with multi-term history tracking.

![Seating Chart Demo](docs/assets/demo-seating.png)

## ✨ Key Features

| Feature | Description |
| :--- | :--- |
| 🧩 **Strict Hard Constraints** | Guarantees satisfaction of mandatory rules: fixed seats, required neighbors, forbidden pairs, minimum distances, and group isolation. Every solved plan is independently validated with zero violations. |
| 🎯 **Explainable Soft Preferences** | Intelligently balances vision needs, height gradients, academic diversity, fair seat rotations, and recent-neighbor avoidance. Every rule provides explicit scoring breakdowns. |
| 🔀 **Multi-Candidate Comparison & Determinism** | Generates multiple high-quality candidate plans with clear trade-off metrics. Pinning the random seed ensures 100% reproducible results anytime. |
| ✋ **Interactive Hand Tuning** | Easily swap seats, drag and drop, lock specific assignments, undo/redo, and apply intelligent constraint-aware repairs with real-time rule validation. |
| 📅 **Multi-Term Fair Rotation** | Tracks historical seating, generates multi-period rotations, and visualizes seat churn and adjacent-period movement distances with a local heatmap. |
| 🖨️ **8 Standard Export Formats** | High-fidelity export to SVG, standalone HTML, printable HTML, PNG images, PDF, Excel (XLSX), Word (DOCX), and PowerPoint (PPTX), with one-click toggles for teacher records vs. anonymized student postings. |
| 🔒 **Local-First & Privacy by Design** | 100% offline computation without accounts, telemetry, or third-party servers. Public exports automatically anonymize sensitive identifiers. |

---

## 🚀 Quick Start

### 1. Desktop App (Recommended for Teachers)

Download the installer for your operating system from [GitHub Releases](https://github.com/FrankFu916/seattrellis/releases):

| Platform | Format |
| :--- | :--- |
| **macOS** (Apple Silicon) | `.dmg` installer or `.app.tar.gz` |
| **Windows** (x64) | `.msi` package or NSIS `.exe` installer |
| **Linux** (amd64) | `.deb` package |

> 💡 **Tip**: Binaries are distributed unsigned. On macOS, right-click the app and choose "Open" on first launch; on Windows, click "More info" → "Run anyway" if Microsoft SmartScreen prompts.

### 2. Command-Line Interface (CLI)

Install directly using Cargo:

```bash
cargo install seattrellis

# 1. Validate problem input and rule definitions
seattrellis validate --problem problem.json

# 2. Solve and output the full seating snapshot
seattrellis solve --problem problem.json --output plan.json

# 3. Export the plan to an image or document
seattrellis export --problem problem.json --solution plan.json --format png --output plan.png
```

### 3. Rules at a Glance

Seating rules are defined in a clean JSON format, clearly separating mandatory hard constraints from weighted soft preferences:

```json
{
  "seed": 42,
  "hard": {
    "fixed_seats":       [{ "student": "STU001", "seat_id": "R1C1" }],
    "cannot_be_adjacent": [{ "students": ["STU004", "STU007"] }]
  },
  "soft": {
    "vision_front": { "enabled": true, "weight": 20 },
    "height_back":  { "enabled": true, "weight": 5 },
    "fair_rotation": { "enabled": true, "weight": 10, "lookback": 4 }
  }
}
```

- Schema & field definitions: [Input Format Guide](docs/input-format.md) and [Rule Handbook](docs/rules.md).
- 14 built-in classroom scenarios (exam mode, daily balanced, rotation, etc.): [Presets Reference](docs/presets.md).

---

## 🔄 Upgrading from v1 (Python)

If you are upgrading from legacy v1 (Python), use `seattrellis schema-migrate` or the built-in migration wizard in the web workbench to automatically upgrade your project files. Backups are created automatically before any file changes.

The legacy Python package is frozen at `1.9.0` for maintenance (`pip install seattrellis==1.9.0`). Version 2 is a pure Rust implementation with zero Python dependencies. See [Migration Guide](docs/rust-migration.md).

---

## 📖 Documentation

| Guides & Usage | Specifications | Deep Dives & Dev |
| :--- | :--- | :--- |
| 📖 [Quick Start Guide](docs/quickstart.md) | 📐 [Rule Handbook](docs/rules.md) | 🏗️ [Architecture Overview](docs/architecture.md) |
| 🖥️ [Web & Desktop Workbench](docs/web.md) | 📄 [Input Format Reference](docs/input-format.md) | ⚙️ [CLI Reference (27 commands)](docs/cli.md) |
| 🖨️ [Export & Printing Guide](docs/export.zh.md) | 🎒 [Class Project Workflow](docs/project.zh.md) | 🔒 [Privacy & Local Boundaries](docs/privacy.md) |

---

## 🛡️ Privacy & Confidentiality

SeatTrellis operates strictly under a local-first paradigm. All computation, file operations, and exports run entirely on your local machine. No student rosters, IDs, academic scores, or classroom layouts are ever transmitted over the network.

---

## 💻 Development & Building

Built on a modern, high-performance tech stack:
- **Core Backend**: Rust 1.88+ with 9 modular crates.
- **Desktop & UI**: Tauri 2, React 19, and TypeScript.
- **Verification**: 690+ Rust tests, 160+ UI tests, end-to-end browser workflows, fuzz testing, and strict CI benchmarks.

```bash
# 1. Build web workbench assets
cd clients/web && npm ci && npm run build && cd ../..

# 2. Run full Rust verification and linting
cargo test --locked --workspace
cargo clippy --locked --all-targets --workspace -- -D warnings
```

---

## 📄 License

Distributed under the [Apache-2.0 License](LICENSE). See [NOTICE](NOTICE) for additional details.
