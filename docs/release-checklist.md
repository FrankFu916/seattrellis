# v0.1.0 Release Checklist

## Local Verification

- [ ] Create a clean virtual environment.
- [ ] Run `python -m pip install --upgrade pip`.
- [ ] Run `python -m pip install -e ".[dev,web]"`.
- [ ] Run `pytest`.
- [ ] Run `python -m build`.

## README Command Verification

- [ ] Run `seattrellis --help`.
- [ ] Run `seattrellis init-demo`.
- [ ] Run `seattrellis solve --students examples/students.xlsx --layout examples/classroom.json --rules examples/rules.json`.
- [ ] Run `seattrellis export --snapshot outputs/latest.snapshot.json --format excel`.
- [ ] Run `seattrellis export --snapshot outputs/latest.snapshot.json --format png`.
- [ ] Run `seattrellis export --snapshot outputs/latest.snapshot.json --format html`.

## Privacy And Packaging

- [ ] Confirm `examples/` contains fictional data only.
- [ ] Confirm no real student names, IDs, school names, class names, grades, notes, snapshots, API keys, `.env`, or private exports are tracked.
- [ ] Confirm `outputs/`, `exports/`, `snapshots/`, `private/`, `data/`, `real_students/`, and `real_classes/` remain ignored.
- [ ] Confirm `pyproject.toml` version is `0.1.0`.
- [ ] Confirm CI passes on GitHub Actions.

## Release

- [ ] Review `CHANGELOG.md`.
- [ ] Create and push the tag:

```bash
git tag v0.1.0
git push origin v0.1.0
```

- [ ] Create a GitHub Release for `v0.1.0`.
- [ ] Include a short privacy note in the release description.
