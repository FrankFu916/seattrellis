# SeatTrellis (席序)

[![Tests](https://github.com/FrankFu916/seattrellis/actions/workflows/tests.yml/badge.svg)](https://github.com/FrankFu916/seattrellis/actions/workflows/tests.yml)
[![Rust](https://github.com/FrankFu916/seattrellis/actions/workflows/rust.yml/badge.svg)](https://github.com/FrankFu916/seattrellis/actions/workflows/rust.yml)
[![License](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)
[![Release](https://img.shields.io/github/v/release/FrankFu916/seattrellis?include_prereleases&label=release)](https://github.com/FrankFu916/seattrellis/releases)

**English | [简体中文](README.md)**

SeatTrellis is a **local-first classroom seating tool**: import a roster, generate a seating plan, adjust it by hand, and export it for printing or sharing. It works entirely on your machine — no accounts, no cloud sync, no student data leaves your computer.

- Generate reproducible seating plans (single plan or a diverse candidate set with explainable scores)
- Satisfy hard constraints (fixed seats, adjacency, minimum distance, groups) while optimizing soft preferences (height, vision, score balance, fairness, recent-neighbor avoidance)
- Adjust plans with an interactive editor: drag, swap, lock, undo, repair
- Plan rotation periods with fairness and pair-repeat summaries
- Export to SVG, HTML, print-HTML, PNG, PDF, XLSX, DOCX and PPTX
- Fully offline; public exports are anonymized automatically

![Demo seating chart](docs/assets/demo-seating.png)

## Install

### Desktop (recommended)

Download the installer for your platform from the [Releases](https://github.com/FrankFu916/seattrellis/releases) page:

- **Windows**: MSI or NSIS installer (x64)
- **macOS**: DMG or app archive (Apple Silicon)
- **Linux**: DEB package

Verify every download against `SHA256SUMS` before installing.

### CLI

```bash
cargo install seattrellis_cli
# or use the prebuilt binaries from Releases
```

### v1 Python line

The v1 (Python) line is frozen at **1.9.0** and maintained on the `v1.x-maintenance` branch. If you need the legacy package:

```bash
pip install seattrellis==1.9.0
```

## Quick start (CLI)

```bash
seattrellis_cli validate --problem problem.json
seattrellis_cli solve --problem problem.json --output plan.json
seattrellis_cli export --problem problem.json --snapshot plan.json --format png --output plan.png
```

See the [quick start guide](docs/quickstart.en.md) for full scenarios, the [input format](docs/input-format.en.md) reference, and the [CLI reference](docs/cli.md).

## Migrating from v1

Project files and artifacts from v1.x migrate automatically in the v2 workbench or via `seattrellis_cli schema-migrate`, with automatic backups before each migration.

## Documentation

- [Quick start (zh)](docs/quickstart.zh.md) · [Quick start (en)](docs/quickstart.en.md)
- [Input format](docs/input-format.en.md) · [Rules](docs/rules.en.md) · [Export](docs/export.zh.md) · [Privacy](docs/privacy.md)
- [Development guide](docs/development.md) · [Publishing](docs/publishing.md)

## Privacy

SeatTrellis processes everything locally. Never commit real student rosters, IDs, grades, classes, schools or seating history to a public repository — the repository contains only synthetic example data.

## License

Apache-2.0. See [LICENSE](LICENSE) and [NOTICE](NOTICE).
