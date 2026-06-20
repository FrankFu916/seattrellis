# Contributing to SeatTrellis

Thank you for helping improve SeatTrellis / 席序.

## Development Setup

```bash
git clone https://github.com/<your-username>/seattrellis.git
cd seattrellis
python -m venv .venv
source .venv/bin/activate
python -m pip install -e ".[dev,web]"
```

On Windows PowerShell:

```powershell
python -m venv .venv
.\.venv\Scripts\Activate.ps1
python -m pip install -e ".[dev,web]"
```

## Running Tests

```bash
pytest
```

Please add or update tests for any new rule, importer, exporter, or CLI behavior.

## Issues

When opening an Issue:

- describe the expected behavior and actual behavior;
- include a minimal fictional example when possible;
- mention your Python version and operating system;
- do not paste real student names, IDs, grades, class names, school names, or seating snapshots.

## Pull Requests

Before opening a Pull Request:

- run `pytest`;
- keep core solving logic independent from CLI and Streamlit UI;
- update README or examples if user-facing behavior changes;
- keep examples fictional;
- do not commit generated outputs, snapshots, `.env` files, or private classroom data.

## Privacy

SeatTrellis is designed for local-first classroom data processing. Public contributions must not contain real student data. Use fictional IDs such as `STU001` and names such as `Student001` in tests and examples.
