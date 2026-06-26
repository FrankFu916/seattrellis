# v0.3.0 Release Checklist

## Local Verification

- [ ] Create a clean virtual environment.
- [ ] Run `python -m pip install --upgrade pip`.
- [ ] Run `python -m pip install -e .`.
- [ ] Run `seattrellis --help`.
- [ ] Run `seattrellis init-demo --force`.
- [ ] Run `seattrellis presets list`.
- [ ] Run `seattrellis presets show daily`.
- [ ] Run `seattrellis presets export daily --output outputs/daily.rules.json`.
- [ ] Run `seattrellis validate --students examples/students.csv --layout examples/classroom.json --preset daily --history-dir examples/history`.
- [ ] Run `seattrellis solve --students examples/students.csv --layout examples/classroom.json --preset daily --history-dir examples/history --output outputs/daily.snapshot.json`.
- [ ] Run `seattrellis project-info --project examples/project.seattrellis.json`.
- [ ] Run `seattrellis project-validate --project examples/project.seattrellis.json`.
- [ ] Run `seattrellis project-solve --project examples/project.seattrellis.json --candidates 3 --output outputs/project.candidates.json --report outputs/project-plan-report.json`.
- [ ] Run `seattrellis project-export --project examples/project.seattrellis.json --snapshot outputs/project.candidates.json --candidate recommended --format html --output outputs/project-recommended.html`.
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
- [ ] Run `pytest tests/test_web_workflow.py`.
- [ ] Launch `streamlit run src/seattrellis/web/app.py` and confirm the quick-solve and project tabs load.
- [ ] Run `python -m build`.

## README Command Verification

- [ ] Run `seattrellis --help`.
- [ ] Run `seattrellis init-demo`.
- [ ] Run `seattrellis presets list`.
- [ ] Run `seattrellis presets show daily`.
- [ ] Run `seattrellis presets export daily --output outputs/daily.rules.json`.
- [ ] Run `seattrellis validate --students examples/students.csv --layout examples/classroom.json --preset daily --history-dir examples/history`.
- [ ] Run `seattrellis solve --students examples/students.csv --layout examples/classroom.json --preset daily --history-dir examples/history --output outputs/daily.snapshot.json`.
- [ ] Run `seattrellis project-info --project examples/project.seattrellis.json`.
- [ ] Run `seattrellis project-validate --project examples/project.seattrellis.json`.
- [ ] Run `seattrellis project-solve --project examples/project.seattrellis.json --candidates 3 --output outputs/project.candidates.json --report outputs/project-plan-report.json`.
- [ ] Run `seattrellis project-export --project examples/project.seattrellis.json --snapshot outputs/project.candidates.json --candidate recommended --format html --output outputs/project-recommended.html`.
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
- [ ] Confirm built-in preset definitions contain rules and metadata only, with no student or classroom records.
- [ ] Confirm project files contain relative paths and defaults only, with no embedded real student data.
- [ ] Confirm `pyproject.toml` version is `0.3.0`.
- [ ] Confirm `git status --short` has no suspicious generated files.
- [ ] Confirm `git ls-files` does not include ignored real-data directories.
- [ ] Confirm CI passes on GitHub Actions.

## Release

- [ ] Review `CHANGELOG.md`.
- [ ] Create and push the tag:

```bash
git tag -a v0.3.0 -m "SeatTrellis v0.3.0"
git push origin v0.3.0
```

- [ ] Create a GitHub Release for `v0.3.0`.
- [ ] Include a short privacy note in the release description.
