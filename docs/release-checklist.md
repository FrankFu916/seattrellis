# v0.2.2 Release Checklist

## Local Verification

- [ ] Create a clean virtual environment.
- [ ] Run `python -m pip install --upgrade pip`.
- [ ] Run `python -m pip install -e .`.
- [ ] Run `seattrellis --help`.
- [ ] Run `seattrellis validate --students examples/students.csv --layout examples/classroom.json --rules examples/rules.json`.
- [ ] Run `seattrellis history-report --students examples/students.csv --layout examples/classroom.json --history-dir examples/history`.
- [ ] Run `seattrellis pair-report --students examples/students.csv --layout examples/classroom.json --history-dir examples/history`.
- [ ] Run `seattrellis solve --students examples/students.csv --layout examples/classroom.json --rules examples/rules_neighbor_avoidance.json --history-dir examples/history --output outputs/neighbor-aware.snapshot.json`.
- [ ] Run `seattrellis export --snapshot outputs/neighbor-aware.snapshot.json --format html`.
- [ ] Run `seattrellis solve --students examples/students.csv --layout examples/classroom.json --rules examples/rules_multi_candidate.json --history-dir examples/history --candidates 3 --output outputs/candidates.json --report outputs/plan-report.json`.
- [ ] Run `seattrellis export --snapshot outputs/candidates.json --candidate recommended --format html --output outputs/recommended.html`.
- [ ] Run `pytest tests/test_minimal_install.py`.
- [ ] Run `python -m pip install -e ".[all,dev]"`.
- [ ] Run `pytest`.
- [ ] Run `python -m build`.

## README Command Verification

- [ ] Run `seattrellis --help`.
- [ ] Run `seattrellis init-demo`.
- [ ] Run `seattrellis validate --students examples/students.csv --layout examples/classroom.json --rules examples/rules.json`.
- [ ] Run `seattrellis history-report --students examples/students.csv --layout examples/classroom.json --history-dir examples/history`.
- [ ] Run `seattrellis pair-report --students examples/students.csv --layout examples/classroom.json --history-dir examples/history`.
- [ ] Run `seattrellis solve --students examples/students.csv --layout examples/classroom.json --rules examples/rules_neighbor_avoidance.json --history-dir examples/history --output outputs/neighbor-aware.snapshot.json`.
- [ ] Run `seattrellis export --snapshot outputs/neighbor-aware.snapshot.json --format html`.
- [ ] Run `seattrellis solve --students examples/students.csv --layout examples/classroom.json --rules examples/rules_multi_candidate.json --history-dir examples/history --candidates 5 --output outputs/candidates.json --report outputs/plan-report.json`.
- [ ] Run `seattrellis export --snapshot outputs/candidates.json --candidate recommended --format html --output outputs/recommended.html`.
- [ ] Run `seattrellis solve --students examples/students.csv --layout examples/classroom.json --rules examples/rules.json --history-dir examples/history`.
- [ ] Run `seattrellis export --snapshot outputs/latest.snapshot.json --format html`.
- [ ] With `excel` and `image` extras installed, run `seattrellis solve --students examples/students.xlsx --layout examples/classroom.json --rules examples/rules.json --history-dir examples/history`.
- [ ] With `excel` extra installed, run `seattrellis export --snapshot outputs/latest.snapshot.json --format excel`.
- [ ] With `image` extra installed, run `seattrellis export --snapshot outputs/latest.snapshot.json --format png`.

## Privacy And Packaging

- [ ] Confirm `examples/` contains fictional data only.
- [ ] Confirm `examples/history/` contains fictional snapshots only.
- [ ] Confirm no real student names, IDs, school names, class names, grades, notes, historical snapshots, API keys, `.env`, or private exports are tracked.
- [ ] Confirm `outputs/`, `exports/`, `snapshots/`, `private/`, `data/`, `real_students/`, and `real_classes/` remain ignored.
- [ ] Confirm no real candidate reports or candidate-set snapshots are tracked.
- [ ] Confirm `pyproject.toml` version is `0.2.2`.
- [ ] Confirm `git status --short` has no suspicious generated files.
- [ ] Confirm `git ls-files` does not include ignored real-data directories.
- [ ] Confirm CI passes on GitHub Actions.

## Release

- [ ] Review `CHANGELOG.md`.
- [ ] Create and push the tag:

```bash
git tag -a v0.2.2 -m "SeatTrellis v0.2.2"
git push origin v0.2.2
```

- [ ] Create a GitHub Release for `v0.2.2`.
- [ ] Include a short privacy note in the release description.
