# v1.3.0 Release Checklist

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
- [ ] Run `cargo test --manifest-path native/Cargo.toml`.
- [ ] Run `python scripts/smoke_cli.py --optional auto --time-limit 3 --json-report outputs/cli-smoke.json`.
- [ ] Run `seattrellis schema export --output-dir schemas` and confirm `git diff -- schemas` is empty.
- [ ] Run `seattrellis schema migrate --input examples/history/week1.snapshot.json --output outputs/week1.migrated.snapshot.json`.
- [ ] Run `python scripts/benchmark_solver.py --sizes 40,50,60 --backends fallback --candidates 1 --time-limit 10 --output outputs/benchmark-solver.json --markdown-output outputs/benchmark-solver.md`.
- [ ] Run `pytest tests/test_web_workflow.py`.
- [ ] Run `python -m pip install -e ".[web,e2e]"`.
- [ ] Run `python -m playwright install chromium`.
- [ ] On Linux, run `python -m playwright install --with-deps chromium`.
- [ ] Run `python -m pytest e2e --browser=chromium`.
- [ ] Launch `streamlit run src/seattrellis/web/app.py --server.address 127.0.0.1` and confirm the quick-solve and project tabs load.
- [ ] Build from a clean checkout so untracked files matched by `MANIFEST.in` cannot enter the source distribution.
- [ ] Run `python -m build`.
- [ ] Run `python scripts/check_release_version.py`.

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

- [ ] Run `python scripts/check_repository_hygiene.py`.
- [ ] Run `python -m build` and `python -m twine check dist/*`.
- [ ] Run the hygiene check once for every file in `dist/` with `--archive`.
- [ ] Confirm the dependency audit, secret scan, and package hygiene workflows pass.
- [ ] Confirm `examples/` contains fictional data only.
- [ ] Confirm `examples/history/` contains fictional snapshots only.
- [ ] Confirm no real student names, IDs, school names, class names, grades, notes, historical snapshots, API keys, `.env`, or private exports are tracked.
- [ ] Confirm `outputs/`, `exports/`, `snapshots/`, `private/`, `data/`, `real_students/`, and `real_classes/` remain ignored.
- [ ] Confirm no real candidate reports or candidate-set snapshots are tracked.
- [ ] Confirm built-in preset definitions contain rules and metadata only, with no student or classroom records.
- [ ] Confirm project files contain relative paths and defaults only, with no embedded real student data.
- [ ] Confirm `pyproject.toml` version is `1.3.0`.
- [ ] Confirm `git status --short` has no suspicious generated files.
- [ ] Confirm `git ls-files` does not include ignored real-data directories.
- [ ] Confirm CI passes on GitHub Actions.

## Release

- [ ] Review `CHANGELOG.md`.
- [ ] For the first PyPI release, confirm the TestPyPI and PyPI Trusted Publishers
      described in `docs/publishing.md` are configured.
- [ ] Publish a uniquely versioned candidate such as `1.3.0rc1` to TestPyPI;
      confirm its installation-verification job passed in a clean environment.
- [ ] Confirm the reviewed release commit has restored both package version fields
      to `1.3.0`, then run `python scripts/check_release_version.py --tag v1.3.0`.
- [ ] Create a draft GitHub Release targeting the reviewed `main` commit, with tag
      `v1.3.0` and title `SeatTrellis v1.3.0`.
- [ ] Include a short privacy note in the release description.
- [ ] Publish the GitHub Release.
- [ ] Confirm every `Publish distributions` job passed, including PyPI publication,
      clean installation verification, and Release asset upload.
- [ ] Confirm the GitHub Release contains the wheel, sdist, and `SHA256SUMS`.
- [ ] Confirm `pip install seattrellis==1.3.0`, `seattrellis --version`, and
      `seattrellis --help` work in a clean environment.
