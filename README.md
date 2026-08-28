<div align="center">
  <img src="docs/assets/logo.svg" width="128" alt="SeatTrellis logo" />

  # **SeatTrellis**

  **Classroom seating, solved — not negotiated.**

  A local-first seating planner: import your roster, generate the chart,
  fine-tune by hand, export or print. No accounts, no cloud sync — student
  data never leaves the machine.

  [![Tests](https://github.com/FrankFu916/seattrellis/actions/workflows/tests.yml/badge.svg)](https://github.com/FrankFu916/seattrellis/actions/workflows/tests.yml)
  [![Rust](https://github.com/FrankFu916/seattrellis/actions/workflows/rust.yml/badge.svg)](https://github.com/FrankFu916/seattrellis/actions/workflows/rust.yml)
  [![Documentation](https://github.com/FrankFu916/seattrellis/actions/workflows/docs.yml/badge.svg)](https://github.com/FrankFu916/seattrellis/actions/workflows/docs.yml)
  [![Release](https://img.shields.io/github/v/release/FrankFu916/seattrellis)](https://github.com/FrankFu916/seattrellis/releases)
  [![License](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)
  [![Platform](https://img.shields.io/badge/platform-macOS%20%7C%20Windows%20%7C%20Linux-lightgrey)](https://github.com/FrankFu916/seattrellis/releases)

  [Download the desktop app](https://github.com/FrankFu916/seattrellis/releases) · `cargo install seattrellis`

  [Latest release](https://github.com/FrankFu916/seattrellis/releases) · [Documentation](https://frankfu916.github.io/seattrellis/) · [Quick start](docs/quickstart.en.md) · [Rules](docs/rules.en.md) · [简体中文](README.zh-CN.md)
</div>

---

Every class has its sensitive seats: the near-sighted kid up front, the tall
one in back, two students who cannot sit together, and a parent asking for
"a little attention". Hand-arranging takes an afternoon — every term, again —
and nobody can explain why the chart looks the way it does.

**SeatTrellis turns that into one click**: you set the rules, it solves,
explains, and keeps the record.

![Seating chart demo](docs/assets/demo-seating.png)

## What it does

| | |
|---|---|
| 🧩 **Hard constraints, guaranteed** | Fixed seats, must/cannot sit together, minimum distance, group rules — every plan marked *Solved* is re-verified by an independent validator |
| 🎯 **Soft goals, explainable** | Front seats for poor vision, tall students in back, score balance, fair rotation, recent-neighbor avoidance — per-rule scoring answers "why is this student here?" |
| 🔀 **Candidates & reproducibility** | Generate several candidates with a recommendation; fix the seed and any rerun, any day, is byte-identical |
| ✋ **Hand tuning** | Drag, swap, lock, undo/redo, constraint-aware repair — every edit re-checked against your rules |
| 📅 **Multi-term rotation** | Fair rotation plans with desk-mate repetition summaries for long-running classes |
| 🖨️ **Eight export formats** | SVG / HTML / print HTML / PNG / PDF / XLSX / DOCX / PPTX, with teacher and anonymized public variants |
| 🔒 **Local-first** | Everything computes on your machine. No accounts, no telemetry, no cloud sync; public exports anonymize names and IDs automatically |

## Quick start

### Desktop (for teachers)

Grab an installer from [Releases](https://github.com/FrankFu916/seattrellis/releases):

| Platform | Format |
|---|---|
| macOS (Apple Silicon) | `.dmg` / `.app.tar.gz` |
| Windows (x64) | `.msi` / NSIS `.exe` |
| Linux (amd64) | `.deb` |

Builds ship unsigned by owner decision — verify against `SHA256SUMS` /
`DESKTOP-SHA256SUMS`. On first launch, macOS needs right-click → Open and
Windows may show a SmartScreen prompt.

### Command line (for automation)

```bash
cargo install seattrellis

seattrellis validate --problem problem.json   # precheck rules & data
seattrellis solve    --problem problem.json --output plan.json
seattrellis export   --problem problem.json --solution plan.json --format png --output plan.png
```

### Rules in 30 seconds

A rules file is JSON with `hard` (must hold) and `soft` (weighted goals):

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

Full reference: [input format](docs/input-format.en.md) and
[rule handbook](docs/rules.en.md); 14 built-in scenario presets in
[docs/presets.md](docs/presets.md).

## Upgrading from v1

v1 (Python) project files are migrated automatically — with a backup — by
`seattrellis schema-migrate` or the workbench migration flow. The Python
package is frozen at 1.9.0 on the maintenance line
(`pip install seattrellis==1.9.0`); nothing in v2 depends on it. See
[migrating from v1](docs/rust-migration.md).

## Documentation

Browse the full, searchable site at
[frankfu916.github.io/seattrellis](https://frankfu916.github.io/seattrellis/).

| | | |
|---|---|---|
| [Quick start](docs/quickstart.en.md) | [CLI reference](docs/cli.md) (27 subcommands) | [Input format](docs/input-format.en.md) |
| [Rules](docs/rules.en.md) | [Exports](docs/export.zh.md) (中文) | [Architecture](docs/architecture.md) |
| [Web workbench](docs/web.en.md) | [Privacy](docs/privacy.md) | [Development](docs/development.md) |

## Privacy

Everything runs locally. No accounts, no telemetry, no cloud sync. Never
commit real student names, IDs, scores or school details — the repository
contains fictional examples only. Public exports are anonymized at a single
central policy layer, with release-time sensitive-field scans.

## Development

```bash
# Frontend (embedded into the server at build time)
cd clients/web && npm ci && npm run build && cd ..

# Full Rust test suite + clippy
cargo test --locked --workspace
cargo clippy --locked --all-targets --workspace -- -D warnings
```

Stack: Rust 1.88 (MSRV) · 9 layered crates · Tauri 2 · React 19 ·
698 Rust tests + 167 web tests + browser E2E + fuzzing + performance gates.
See [docs/architecture.md](docs/architecture.md).

## License

Apache-2.0 — see [LICENSE](LICENSE) and [NOTICE](NOTICE).
