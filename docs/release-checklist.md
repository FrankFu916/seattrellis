# Release checklist

Use this checklist for every public release. Replace `<version>` with the exact
PEP 440 package version and `<tag>` with `v<version>`. A release candidate must
use its own version, for example `1.4.0rc1`; package indexes do not allow files
to be replaced.

## Scope and repository state

- [ ] Every item promised by the milestone is implemented, documented and
      covered by an acceptance test, or has been moved explicitly to a later
      milestone.
- [ ] The pull request is reviewable as a sequence of focused commits and has
      no unresolved review threads.
- [ ] `CHANGELOG.md` describes user-visible additions, changes and fixes.
- [ ] Schema changes include migration behavior, committed schemas and fixtures.
- [ ] `git status --short` is clean in the release checkout.
- [ ] `python scripts/check_repository_hygiene.py` passes.

## Release quality gate

Run this gate once after the version scope is frozen, rather than repeating the
entire matrix after every small change.

- [ ] Install the minimal package in a clean Python 3.11 environment and run
      `python -m pytest tests/test_minimal_install.py`.
- [ ] Install `.[all,dev]` and run the complete test suite.
- [ ] Run `cargo test --manifest-path native/Cargo.toml`.
- [ ] Build the optional native wheel and run its Python/Rust differential
      contract tests. The native wheel remains an experimental CI artifact
      until it has a separate public distribution decision.
- [ ] Run `python scripts/smoke_cli.py --optional auto --time-limit 3`.
- [ ] Run the documented release benchmark matrix for 40, 50 and 60 students,
      light and dense rules, and 1, 5 and 20 candidates. Archive both JSON and
      Markdown reports.
- [ ] Run the Playwright Chromium acceptance suite from a clean Web install.
- [ ] Launch the Web application on `127.0.0.1` and complete one manual
      import → solve → adjust → export workflow.
- [ ] Confirm GitHub Actions passes on Python 3.11 and 3.14 on Linux, Windows
      and macOS, with Python 3.12 and 3.13 compatibility lanes on Linux.
- [ ] Confirm the dependency audit, secret scan, package hygiene and Web E2E
      jobs pass.

## Functional acceptance

- [ ] `seattrellis init-demo --force`, `validate` and a one-candidate `solve`
      complete successfully.
- [ ] Generate several candidates and inspect the recommendation, score
      differences, hard-constraint summary and history explanation.
- [ ] Exercise manual swap, move, unseat, lock, undo/redo and constrained repair;
      confirm the operation log and snapshot provenance are saved.
- [ ] Run the Project info, validate, solve, edit, repair and export commands on
      the example project.
- [ ] Export teacher and public outputs in Chinese and English, including A4
      portrait and landscape layouts.
- [ ] Confirm a public export contains no scores, notes, special needs, height,
      vision data or un-anonymized student identifiers.
- [ ] Export and validate the committed JSON Schemas. Run migration dry-run and
      write modes, including backup creation for an existing destination.
- [ ] Verify the fallback, OR-Tools and optional native backend status messages.
      A timeout or unknown status must not be described as proven infeasibility.

## Packaging and privacy

- [ ] Build from a clean checkout so ignored or untracked files cannot enter
      the source distribution.
- [ ] Run `python -m build` and `python -m twine check dist/*`.
- [ ] Run `python scripts/check_repository_hygiene.py --archive <file>` for
      every artifact in `dist/`.
- [ ] Confirm examples contain fictional data only and no real names, IDs,
      school details, notes, snapshots, exports, keys or environment files are
      tracked.
- [ ] Confirm generated output and private-data directories remain ignored.
- [ ] Confirm `pyproject.toml` and `seattrellis.__version__` both equal
      `<version>` by running `python scripts/check_release_version.py --tag <tag>`.

## TestPyPI candidate

- [ ] Confirm the `testpypi` GitHub environment and TestPyPI Trusted Publisher
      still target `.github/workflows/publish.yml`.
- [ ] Set both package version fields to a unique release candidate version and
      merge the reviewed candidate commit.
- [ ] Manually run `Publish distributions` with target `testpypi`.
- [ ] Confirm clean installation verification succeeds on Python 3.11 and 3.14.
- [ ] Install the candidate from TestPyPI independently and run
      `seattrellis --version`, `seattrellis --help` and the CLI smoke workflow.

## Public release

- [ ] Restore both package version fields to the final `<version>` and rerun the
      release quality gate for the reviewed commit.
- [ ] Confirm the `pypi` GitHub environment and PyPI Trusted Publisher still
      target `.github/workflows/publish.yml`; keep environment approval enabled.
- [ ] Create a draft GitHub Release targeting the reviewed `main` commit with
      tag `<tag>` and title `SeatTrellis <tag>`.
- [ ] Write concise release notes with upgrade, compatibility, privacy and known
      limitation sections. Do not claim the experimental native backend is a
      standalone solver.
- [ ] Publish the GitHub Release and confirm every `Publish distributions` job
      succeeds, including PyPI upload and clean Python 3.11/3.14 installation.
- [ ] Confirm the Release contains the wheel, source distribution and
      `PYTHON-SHA256SUMS`；桌面包使用 `DESKTOP-SHA256SUMS`。
- [ ] For a desktop release, run `Tauri desktop bundles` with the existing
      dedicated desktop preview tag and inspect the unsigned
      `.app`/`.dmg`, `.msi`/NSIS, and `.deb`/AppImage assets before enabling
      signing or notarisation. A desktop preview must not be attached to the
      Python-only `v<version>` release.
- [ ] Install `seattrellis==<version>` from PyPI in a clean environment and run
      `seattrellis --version`, `seattrellis --help` and one solve/export smoke
      workflow.

If publication fails after a version has reached an index, follow
`docs/publishing.md`: do not replace the files or rewrite the tag. Yank the
affected release when appropriate and publish a new patch version.
