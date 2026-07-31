# Changelog

## 1.8.2 - 2026-07-31

### User-facing changes

- Added multi-period rotation plans: teachers can generate several future seat
  arrangements at once and see repeated-neighbour and fairness summaries.
- Added local project backup and restore with `.seattrellis.zip`, a recent-projects
  listing, and a privacy scan before sharing a class folder.
- Added an optional desktop preview using pywebview. It opens the same React
  workbench as the browser command and closes its loopback service with the window.

### Engineering and maintenance

- Added a versioned `rotation_plan` JSON artifact, schema export, CLI commands,
  and `/api/v1/classes/rotation` contract.
- Project bundles reject path traversal and symlink entries, validate the project
  manifest before restore, and use a per-session local API bearer token in the
  desktop preview.

## 1.8.1 - 2026-07-31

### User-facing changes

- The `seattrellis workspace` command now works from a pip install: the
  pre-built React workbench is bundled in the package, so no separate
  frontend build step is needed.
- The workbench no longer falls back to demo data when the local service is
  running — room templates, teacher goals, and export formats come from the
  real `/catalogs` API.
- Generating a plan now uses the real solver through `/classes/generate`,
  and manual swaps, locks, and undo are applied through the editing command
  API so the canvas and the exported file always show the same plan.
- The workbench export step is live: teachers can save the current plan as
  HTML, print-html, SVG, PPTX, or any other installed export format with the
  chosen orientation and privacy options.

### Engineering and maintenance

- Bundled the compiled React workbench under `web_static/` and included it in
  the wheel, fixing the workspace command in installed packages.
- Added `/catalogs` and `/exports` API endpoints and tested the full
  generate → edit → export flow over HTTP.
- Migrated the shared models from the pydantic v1 compatibility API to the
  native pydantic v2 API, raised FastAPI to 0.138–0.141, and Starlette to
  1.3.x. This restores the local Web API on Python 3.14 and clears every
  remaining dependency advisory, so the audit no longer needs exclusions.
- Bumped pypdf and lxml to patched releases, added httpx to the dev extra so
  FastAPI TestClient-based API tests run in CI, and declared the PPTX extra
  so PPTX export tests no longer skip.
- Added SVG and PPTX coverage to the CLI smoke suite and listed both formats
  in the export help text.

## 1.8.0 - 2026-07-30

### User-facing changes

- Added SVG vector export for seating plans — self-contained, scalable, and
  suitable for printing or embedding in documents.
- Added PPTX export — a single 16:9 slide with editable text boxes and seat
  shapes that teachers can open in PowerPoint or Keynote and continue editing.
- Both new export formats respect the same privacy controls (public vs teacher
  templates) as existing print and PDF exports.

### Engineering and maintenance

- Introduced a dependency-free seating canvas document model shared by SVG
  and PPTX exporters for consistent seat positions, text, and theme.
- Verified real SVG and PPTX file generation with structural validation and
  path-leak checks.

## 1.7.0 - 2026-07-30

### User-facing changes

- Added three score-based seating goals: score position preference (high
  scores toward front or back), score distribution balance (across rows or
  groups), and mentor pairing (pair high- and low-ranked students nearby).
- New goals work with any grading scale through rank percentiles and appear
  alongside existing presets.

### Engineering and maintenance

- Introduced a shared, backend-neutral soft_objectives module so the fallback
  solver and result scoring use the same normalized losses and explanations.
- Added ScorePositionRule, ScoreDistributionRule, and MentorPairingRule as
  composable weighted soft rules with JSON Schema and preset integration.
- Added validation warnings for group-scoped distribution when seats lack
  group_id, and updated schemas for candidate-set, classroom-layout, ruleset,
  and seating-snapshot.

## 1.6.0 - 2026-07-30

### User-facing changes

- The React classroom workbench now connects to the local SeatTrellis API for
  real roster upload, field mapping, and import preview.
- Teachers can upload a CSV or Excel roster, confirm auto-detected field
  mappings, preview incremental or overwrite import impact, and proceed to
  seating — all from the browser workbench.
- The local browser launcher starts the API and opens the React workbench in
  one step.

### Engineering and maintenance

- Built a TypeScript/React single-page workbench with Vite, sharing design
  tokens for theming and supporting keyboard navigation.
- Added a RosterDraftStore with TTL pruning and size limits for in-memory
  roster upload sessions, plus four API endpoints for draft lifecycle.
- Pinned FastAPI to 0.115.x for pydantic v1 compatibility; migrating API
  models to pydantic v2 is tracked for a future release.
- Added python-multipart to web and all extras for roster file uploads.

## 1.5.0 - 2026-07-30

### User-facing changes

- The Web app now opens in a teacher workspace that goes from CSV or Excel
  roster to a printable seating plan without requiring JSON files or solver
  terminology.
- Teachers can use recommended 30-, 48-, or 60-seat classrooms, or describe a
  custom room with rows, seats per row, and aisle positions.
- Daily rotation, Quick shuffle, and Peer support are presented as familiar
  classroom goals and generate three options automatically.
- The setup now explains when optional history, score, height, or vision data
  is unavailable before a plan is generated.
- The recommended plan can be adjusted with move, swap, lock, undo, and redo,
  then downloaded as a public print or a teacher print.
- Teacher setup and generated plans survive a visit to Advanced tools. Starting
  over clears only the teacher workspace and leaves Quick Solve and Project
  work untouched.
- Public print downloads continue to hide student IDs, scores, height, vision,
  notes, and individual needs by default.
- Teacher print downloads include student IDs for internal class records.

### Engineering and maintenance

- Added task-oriented roster, classroom, goal, and plan-generation application
  services while retaining the existing CLI, Project format, and Python/Rust
  backend selection.
- Added real-browser acceptance coverage for import, generation, manual seat
  adjustment, both print downloads, and public-output privacy.
- Moved Web working-directory ownership outside the Streamlit rerun script so
  sensitive teacher results are deleted when replaced or cleared.
- Stopped retaining original uploaded roster bytes after parsing and stopped
  caching unexpected programming errors as input mistakes.

## 1.4.0 - 2026-07-30

### Added

- Added explicit `auto`, `fallback`, `ortools`, and source-only `native`
  backend selection with shared compiled-problem and result contracts.
- Added repeatable synthetic 40/50/60-student benchmark tooling and reports.
- Added native-extension wheel checks for Python 3.11–3.14 on Linux and for
  Python 3.11 and 3.14 on Windows and macOS, with short-lived CI artifacts for
  inspection.
- Added candidate comparison reports with recommendation, score, constraint,
  and history explanations.
- Candidate reports now include score differences from the recommendation,
  hard-constraint check counts, and fully localized Chinese and English
  explanations without exposing individual violation details.
- Added JSON Schema export and normalization commands for versioned project
  artifacts and editor protocol messages.
- Added RuleSet schema versioning, migration dry runs, and automatic backups
  before an existing migration destination is replaced.
- Added UI-neutral manual editing sessions, replayable operation files, and
  Project commands for editing saved candidates.
- Added lock-aware constrained re-solving for global or student-scoped repair,
  including saved lock state, empty-seat reservation, history-aware fairness,
  and post-solve anchor verification.
- Student-scoped repair now includes one-hop hard-rule and seat-adjacency
  neighbors, records the effective scope, and suggests concrete unlock actions
  when the active restrictions cannot be solved.
- Added Web controls for locking students or seats and running global or
  student-scoped repair with selectable Python, OR-Tools, or native backends.
- Added replayable Web manual swaps with immediate hard-constraint diagnostics
  and undo/redo support; edited drafts remain available to export or repair.
- Added Web controls for moving students to empty seats, placing them in an
  unseated area, and seating them again without implicitly displacing others.
- Added undoable Web student and seat locks that filter invalid manual moves
  and flow directly into saved-lock constrained repair.
- Added atomic batch moves across the domain, CLI operation files and inline
  syntax, and Web multi-selection with one-step undo/redo.
- Added an accessible interactive seating grid for direct click-to-move,
  click-to-swap, and seat-lock toggling through the shared command log.
- Added a versioned editor command/state protocol with atomic command batches,
  optimistic revision checks, duplicate-command protection, minimized frontend
  state, and committed JSON Schema documents.
- Added an experimental Rust validator package and explicit `native` validation
  mode while retaining the Python search backends.
- Expanded the Rust spike to consume one versioned, identity-free DTO for graph
  distances, hard-rule verification, and peer-mixing scoring, with differential
  checks against the Python contract.
- Added a real-browser Playwright acceptance test for the Web Demo-to-export
  workflow, including process cleanup, download and privacy checks, health
  verification, and CI failure artifacts.
- Expanded real-browser coverage to uploaded CSV/JSON inputs and the local
  Project validate/solve/candidate/export workflow.

### Changed

- Solver selection now uses a common backend protocol and capability registry,
  while preserving the existing CLI and service backend names.
- Solver backends now share stable indexes, indexed adjacency, and precomputed
  Euclidean and graph-distance matrices through `CompiledProblem`.
- Multi-candidate generation now compiles and precomputes each problem once,
  reusing a lightweight exclusion view for subsequent candidates.
- Input validation, solver backends, and post-solve checks now share one
  hard-rule resolution layer while preserving existing diagnostics.
- Updated the optional native binding to PyO3 0.29 and maturin 1.14, declared
  Rust 1.83 as its minimum supported toolchain, and added Python 3.13 and 3.14
  package metadata.
- Expanded source-package testing and publish verification to Python 3.11
  through 3.14 while keeping Python 3.11 as the minimum supported version.
- Raised optional dependency floors to versions with verified Python 3.14
  support, and refreshed the browser acceptance environment to Streamlit 1.60.
- Release publishing now verifies the built wheel with all optional features on
  Python 3.11 and 3.14 before either package index can receive it.
- Documentation now distinguishes seeded fallback behavior from reproducible
  runs that complete their fixed attempt budget without a wall-clock cutoff.
- Removed the misleading `native` extra. The experimental Rust extension is
  built separately from a matching source checkout and currently acts only as
  a post-solve validator for the Python fallback backend.
- Moved stateful Web editing and repair controls out of the page entry point
  into a dedicated adapter module while keeping workflow code Streamlit-free.
- Raised the Web extra's minimum Streamlit version to 1.50, matching the
  keyed-widget identity and layout behavior used by the application.
- Quick-solve uploads now survive wizard navigation and invalidate stale
  results when inputs change.
- Standalone Project manifest uploads are validated and previewed without
  resolving server-side paths; full Project operations require a local path.
- Quick solve and Project results no longer appear in the other workspace, and
  preparing a new result or manual edit now discards stale export downloads.
- The checked-in Streamlit configuration and documented commands bind the Web
  app to the local machine by default.
- Project output-directory and JSON write failures now produce user-facing file
  errors instead of uncaught operating-system exceptions.
- Full candidate comparison reports now have an explicit aggregate-only privacy
  invariant backed by sensitive-field regression coverage.
- `doctor` now checks installed package metadata without importing optional
  native or Web runtimes into the diagnostic process.

### Fixed

- Quick Solve now stores uploaded files by role, so identically named layout
  and rules files cannot overwrite each other.
- Project Web sessions now keep solve, edit, repair, and export artifacts in
  separate working directories instead of sharing mutable `latest.*` files.
- Web PDF export now rejects unsupported all-candidate requests instead of
  silently exporting only the recommended plan.
- Sparse layout coordinates no longer expand the Web seat map into thousands
  of empty rows or columns.
- Web repair now starts a fresh editing session immediately, so the first
  post-repair lock or move action is not lost on newer Streamlit versions.
- Manually edited snapshots now use an explicit draft status, retain source
  provenance, and stop presenting stale solver scores, objectives, or metrics
  as current results.
- Schema migrations now replace files atomically and preserve the original
  artifact if the final write cannot be completed.
- Candidate comparison and anonymized reports no longer embed potentially
  identifying free-form warning or rule text.
- Web result tables no longer use the pandas/Arrow conversion path that could
  crash the Streamlit server after a solve.
- Manual edits now inherit persisted student and seat locks while still
  allowing explicit unlock commands.
- Repeated OR-Tools requests consistently report a missing optional dependency
  instead of failing with an internal attribute error.
- The fallback backend now applies its deadline during the deterministic first
  attempt, candidate ranking, and objective calculation.
- OR-Tools timeout and unknown statuses are no longer reported as proof of an
  infeasible seating problem.

## 1.3.0 - 2026-07-25

### Added

- Added the shared `ExportRequest`, `PrivacyOptions`, and `PageOptions`
  application contract, including safe public defaults and A4 page settings.
- Added CLI and Web controls for print templates, field hiding, anonymization,
  page orientation, scaling, and Chinese or English export content.
- Added PDF page-orientation and bilingual exporter regression coverage.
- Added architecture decision records for the Rust native core, the Python
  OR-Tools integration, and the command-log editing model.
- Added a rule capability audit that distinguishes implemented rules from
  model-only declarations.
- Added repository hygiene, dependency-audit, secret-scan, package-validation,
  and Trusted Publishing workflows.
- Added release-tag/package-version consistency checks and an automated
  TestPyPI installation smoke test.
- Added deterministic export regression fixtures and documented failed-publish
  and rollback handling.
- Added a public roadmap for the v1.3–v1.8 milestones.

### Changed

- Print HTML, PDF, and Word exporters now consume the shared export options
  while preserving the legacy exporter arguments.
- Updated the optional OR-Tools range to 9.15.x so its protobuf dependency
  resolves to a release that fixes CVE-2026-0994.
- Documented the release environments and the TestPyPI/PyPI Trusted Publishing
  process.

### Fixed

- `doctor` now checks installed package metadata without importing optional
  native libraries, and reports demo files from the current workspace.

## 1.2.3 - 2026-07-03

### Fixed

- The macOS library-path regression test now uses a platform-neutral mock path,
  avoiding Windows drive-letter semantics.

## 1.2.2 - 2026-07-03

### Fixed

- macOS dynamic-library paths now use the required colon separator even when
  the path setup is exercised by cross-platform tests.

## 1.2.1 - 2026-07-03

### Fixed

- PDF export now discovers Homebrew and MacPorts Pango libraries automatically
  on macOS, including Apple Silicon installations under `/opt/homebrew/lib`.
- The PDF extra now installs CFFI 2 or newer, avoiding native-library cleanup
  warnings seen with older Anaconda environments.

### Documentation

- Added macOS Pango installation and manual dynamic-library path guidance.

## 1.2.0 - 2026-07-03

### Added

- The Web interface can switch between Simplified Chinese and English without
  resetting the current workflow.
- Added a skip link, visible keyboard focus, semantic keyboard-accessible seat
  maps, responsive small-screen layout, and reduced-motion support.
- Added an English Web guide and localized table headings, preset descriptions,
  error guidance, history checks, and privacy notices.

## 1.1.0 - 2026-07-03

### Added

- The quick-solve page can preview and download the rules produced by a preset
  and an optional overlay.
- History checks report student coverage, stale references, disabled seats,
  and layout differences before solving.
- Web settings can be saved to JSON and restored later. The file excludes the
  roster and layout; the page warns when rules contain student identifiers.

### Changed

- Loading the Demo now includes its three fictional history snapshots.
- Updated Streamlit table and button sizing calls to the current `width` API.

## 1.0.1 - 2026-07-02

### Fixed

- Kept web solve artifacts available across Streamlit reruns and made download handling resilient to missing files.
- Prevented partial uploads, invalid candidate selections, stale session state, and duplicate recommended-candidate entries from crashing or confusing the web UI.
- Escaped student names and seat IDs before rendering seat-map HTML.
- Validated candidate counts, time limits, and history path inputs at the service boundary.
- Completed export extension handling and deduplicated overlapping history paths.

### Changed

- Extracted the reusable service layer and API contracts from the CLI while preserving CLI and web behavior.
- Expanded regression coverage to 269 passing tests.

## 1.0.0 - 2026-06-28

### Release summary

- Stable CLI commands: `solve`, `validate`, `export`, `presets`, `project-*`,
  `history-report`, `pair-report`, and `doctor`.
- Local Web interface with seat maps, candidate comparison, Demo data, error
  guidance, and downloads.
- HTML, PDF, PNG, Excel, DOCX, and print-HTML exports.
- Versioned snapshot, candidate, and project files.
- 246 tests, including property tests for hard constraints.
- Wheel and source distributions verified with a clean installation.
- Test matrix for Python 3.11 and 3.12 on Linux, Windows, and macOS.
- Eight rule presets, seven candidate score dimensions, student groups, and
  relationship cooling periods.

### Version Summary (v0.3.2 → v1.0.0)

| Version | Theme |
|---------|-------|
| v0.3.2 | Version consistency, English quickstart, issue templates |
| v0.4.0 | Web usability: seat map, candidate switching, comparison, demo, preset cards, error diagnosis |
| v0.5.0 | Export enhancement: PDF, print HTML, DOCX, privacy options, font strategy |
| v0.6.0 | PyPI prep: --version, doctor, MkDocs, versioning policy, desktop research |
| v0.7.0 | Real classroom: student groups, cooling periods |
| v0.8.0 | Stability: property-based tests, edge-case tests, fuzz tests (246 total) |
| v1.0.0 | Official release |

## 0.8.0 - 2026-06-28

### Added
- **Property-based tests** (73 new tests in `tests/test_property_based.py`): random-rule validation never crashes, fixed-seat conflicts always detected, must+cannot conflicts always detected across 20 random seeds each.
- **Edge-case tests**: zero enabled seats, more students than seats, duplicate keys, fixed-to-disabled seat, unknown student refs, empty student list.
- **Fuzz tests**: malformed JSON, empty CSV, headers-only CSV, extra rule fields, negative row/height/distance — all verify graceful failure without crashes.

### Changed
- Test suite: 173 → 246 tests (73 new property-based, edge-case, and fuzz tests).

## 0.7.0 - 2026-06-28

### Added
- **Student groups** (`GroupRule`): named groups of students with `separate` and `together` flags for group-level constraints. Groups stored in `RuleSet.groups`.
- **Cooling periods** (`CoolingRule`): configurable desk-mate and neighbor cooling — prevent students from being desk-mates or neighbors within N consecutive seatings. Supports `desk_mate`, `adjacent_any`, `horizontal`, `vertical`, `diagonal` relation types.

### Changed
- `RuleSet` now carries an optional `groups` list.
- `SoftRules` now includes optional `cooling` field (`CoolingRule`, disabled by default, weight=5).

## 0.6.0 - 2026-06-28

### Added
- **`--version` / `-V` flag**: displays `seattrellis x.y.z` and exits (Typer callback + argparse `action="version"`).
- **`seattrellis doctor` command**: environment diagnostic checking Python version, platform, all six optional extras (solver/excel/image/web/pdf/docx) with status indicators, presence of example files, outputs directory state, and `SEATTRELLIS_USE_ORTOOLS` env var.
- **MkDocs documentation site skeleton**: `mkdocs.yml` with Material theme, full navigation structure covering quickstart, CLI, Web, input formats, rules, presets, project workflow, candidates/scoring, export, font strategy, history, privacy, troubleshooting, architecture, API reference, and desktop research.
- **`docs/versioning.md`**: SemVer policy, schema version strategy for all file formats, CLI/Python API deprecation policy, compatibility matrix.

- **`docs/desktop-research.zh.md`**: compared Tauri, PySide, NiceGUI, FastAPI
  with pywebview, and Electron for a possible desktop client.

### Changed
- `cli.py`: added `--version` callback on Typer app, `doctor` command (Typer + argparse), `run_doctor()` reusable function.

## 0.5.0 - 2026-06-28

### Added
- **Print-friendly HTML templates**: three scenarios — public notice (class version), teacher internal (rules + warnings + fairness), explanation report (score breakdown + recommendation rationale). A4 portrait, print-optimised CSS.
- **PDF export via WeasyPrint**: new `pdf` optional extra. Shares template logic with print HTML. `seattrellis export --format pdf`.
- **`exporters/print_html.py`**: reusable print HTML renderer with `PrintPrivacyOptions` (hide scores, hide notes, hide special needs, anonymize).

- **Privacy options**: `PrintPrivacyOptions` dataclass controlling score/notes/special-needs/height/vision visibility per template.
- **Three export templates**: `public` (names + seats), `teacher` (full detail table + warnings), `report` (score grid + recommendation text).

- **Word (.docx) export via python-docx**: new `docx` optional extra. Tables with student detail. `seattrellis export --format docx`.
- **Chinese font strategy document**: `docs/font-strategy.zh.md` covering cross-platform CSS font-family fallback chains, WeasyPrint font configuration, and PNG font limitations.
- **Candidate-aware report export**: `--template report` includes score breakdown grid and recommendation rationale for a specific candidate.

### Changed
- `exporters/__init__.py` unified to support all formats (html, excel, png, pdf, docx, print-html) with lazy imports.
- `pyproject.toml` added `pdf` (`weasyprint>=60`) and `docx` (`python-docx>=1.0`) optional extras; both included in `all`.
- `cli.py` updated format help text to list pdf/docx/print-html.
- `web/workflow.py` and `web/app.py` extended with PDF and DOCX download buttons.

### Technical Notes
- WeasyPrint requires system Pango/Cairo libraries (see doc.courtbouillon.org/weasyprint for platform-specific install).
- PDF/DOCX follow the same optional-extra pattern as solver/excel/image/web.

## 0.4.0 - 2026-06-28

### Added
- **Seat map visualization**: HTML/CSS Grid rendering of the classroom layout with student names on seats, disabled-seat styling, and tag-based colour accents (window, door, platform, AC, corner).
- **Candidate interactive switching**: dropdown selector to switch between candidates; seat map, score breakdown, and assignment table update synchronously.
- **Candidate comparison view**: expandable table comparing all candidates across total score, hard constraints, and all seven scoring dimensions.
- **Demo one-click loading**: "🚀 一键加载 Demo" button loads fictional example data without any file preparation.
- **Preset explanation cards**: expandable panel describing each of the eight presets — scenario, required fields, and degradation behaviour.
- **Error diagnosis**: user-readable error categorisation (validation errors, file errors, solve errors, missing dependencies, value errors) with actionable suggestions in Chinese.
- **Step wizard**: three-step horizontal radio guide — load data → configure & solve → view results & export.
- **Privacy notice panel**: green banner at the top of every tab emphasising local-only processing.
- **File format hints**: expandable panel documenting supported formats, size limits, and encoding requirements.
- **Project file upload mode**: project tab now supports file upload in addition to path entry.

### Changed
- `web/app.py` refactored into a thin Streamlit rendering layer; new business logic in `web/components.py`.
- Session-state management unified via `_ss()` helper; solve state resets cleanly across runs.
- `web/workflow.py` extended with `demo_paths()`, `load_demo_layout()`, and `load_demo_snapshot()` helpers.

## 0.3.2 - 2026-06-28

### Fixed
- Fixed version strings across `pyproject.toml`, `src/seattrellis/__init__.py`, `README.md`, `README.en.md`, and `docs/release-checklist.md` to consistently read `0.3.2`.

### Added
- Added `docs/quickstart.en.md` — English-language quick start guide with detailed installation, CLI usage, presets, solving, validation, history analysis, export, project workflow, and scoring dimension documentation.
- Added `.github/ISSUE_TEMPLATE/` with bug report, feature request, and question templates, plus a config linking to the README.
- Added `docs/release-checklist.md` updated to v0.3.2.

## 0.3.1 - 2026-06-27

### Fixed
- Fixed fallback solver vision-front and height-back preferences being neutralized by incorrect row bounds (`_fallback_individual_cost` now uses actual seat-row range instead of per-seat bounds).
- Fixed `_score_recent_neighbors` using all seats (`layout.seats`) instead of only enabled seats (`layout.enabled_seats`), consistent with other scoring dimensions.
- Fixed duplicate `_needs_front` logic — consolidated into `student_needs_front()` on the Student model, eliminating the maintenance risk of two identical copies.
- Fixed `classify_seat_position` crash on layouts with zero enabled seats (added empty-seats guard).

### Improved
- Added duplicate student-key detection to file-level validation so the error surfaces at validate time rather than only at solve time.
- Added 48 scoring tests covering all seven dimensions, edge cases, hard constraints, diversity, recommendation, and `student_needs_front`.
- Removed unused `ProjectPaths` import in `cli.py` and unused `classify_seat_position` import in `scoring.py`.

## 0.3.0 - 2026-06-26

### Added
- Added a fuller local Streamlit workflow for presets, optional rules overlays, history snapshot uploads, and 1-20 generated candidates.
- Added web display for the recommended candidate, score breakdowns, hard-rule checks, candidate warnings, assignment rows, and JSON/report/export downloads.
- Added a web project workspace flow that reuses `project-info`, `project-validate`, `project-solve`, and `project-export`.
- Added a Streamlit-free web workflow helper layer so the UI can reuse existing CLI/core behavior without copying solver, scoring, preset, or project logic.
- Added web workflow tests and a Streamlit app smoke test.

### Improved
- Kept web exports on the existing snapshot/candidate export path, including friendly optional-extra errors for PNG and Excel.
- Kept project paths relative to the project file and avoided embedding private classroom data in project files.
- Updated README and the release checklist for the completed web workflow.

## 0.2.3 - 2026-06-25

### Added
- Added eight built-in rules presets: `random`, `exam`, `daily`, `fair-rotation`, `neighbor-aware`, `balanced`, `height-aware`, and `vision-friendly`.
- Added `presets list`, `presets show`, and `presets export` CLI commands with Typer and argparse support.
- Added `solve --preset` and `validate --preset`, including optional user-rules overlays where explicit user fields override preset defaults.
- Added preset metadata and graceful-degradation warnings for missing history, score, height, or vision data.
- Added portable local project workspace files with relative students, layout, rules, history, and output paths.
- Added `project-init`, `project-info`, `project-validate`, `project-solve`, and `project-export` CLI commands with Typer and argparse support.
- Added project defaults for candidate generation, candidate selection, and export format.
- Added a fictional `examples/project.seattrellis.json` workspace plus preset and project workflow tests.

### Improved
- Kept presets as a thin layer over the existing `RuleSet`, validation, solving, candidate generation, scoring, and export paths.
- Preserved absolute hard-constraint priority when presets are used alone or combined with user rules.
- Kept ordinary rules files, snapshots, candidate sets, and existing CLI commands backward compatible.
- Reused the existing validation, solving, candidate scoring, persistence, and export paths for project workflows.
- Added clear project-file, referenced-path, and output-directory diagnostics without introducing a database or new dependency.
- Updated bilingual documentation and the release checklist for v0.2.3.

## 0.2.2 - 2026-06-24

### Added
- Added multi-candidate seating generation for the fallback and OR-Tools solvers.
- Added explainable candidate scoring with fair rotation, recent-neighbor avoidance, score balance, height, vision, diversity, stability, and hard-constraint summaries.
- Added candidate-set JSON output and plan-comparison report output.
- Added recommended-candidate selection and candidate-set export support.
- Added fictional multi-candidate examples and CLI smoke coverage.

### Improved
- Improved solver output for decision-making workflows while preserving single-snapshot compatibility.
- Expanded documentation for heuristic score-based plan comparison.
- Added tests for deterministic candidate generation, scoring, hard-rule preservation, persistence, and HTML / Excel / PNG candidate export.

## 0.2.1 - 2026-06-23

### Added
- Added pair-history analysis for historical seating snapshots.
- Added an `avoid_recent_neighbors` soft rule for reducing repeated desk-mate and neighbor relationships.
- Added a `pair-report` CLI command for local relationship-history summaries.
- Added fictional examples for neighbor-history avoidance.

### Improved
- Improved history-based scoring by combining seat-category fairness with pair-history awareness.
- Expanded tests for relation detection, pair-history reports, fallback and OR-Tools scoring, and hard-rule priority.
- Updated documentation for relationship-aware seating.

## 0.2.0 - 2026-06-22

### Added
- Added historical snapshot loading for seat rotation analysis.
- Added fair rotation soft rule based on recent seat categories.
- Added seat history statistics for front, back, side, corner, and tagged seat locations.
- Added a `history-report` CLI command for local fairness summaries.
- Added fictional history examples.

### Improved
- Improved snapshot metadata for fairness-related runs.
- Updated documentation for history-based seating and fair rotation.
- Expanded tests for historical seating behavior.

## 0.1.2 - 2026-06-21

### Added
- Added a `validate` command for input and rule preflight checks.
- Added stronger validation for students, classroom layouts, and rules.
- Added clearer hard-constraint conflict diagnostics.
- Added small fictional invalid examples for common validation failures.

### Improved
- Improved CLI error messages for invalid files and infeasible seating plans.
- Expanded tests for invalid inputs and conflicting rules.
- Updated documentation for validation behavior.

## 0.1.1 - 2026-06-21

### Improved
- Split heavy dependencies into optional extras.
- Kept the core package lighter for CLI and fallback-solver usage.
- Added lazy imports for optional solver, Excel, image, and web features.
- Improved missing-extra error messages.
- Added minimal-install and full-feature test coverage.
- Updated README and documentation to match the dependency model.

## 0.1.0 - 2026-06-20

Initial open-source MVP:

- Local-first classroom seating workflow with fictional demo data.
- Pydantic models for students, seat-node classroom layouts, rules, and snapshots.
- CSV and `.xlsx` student import with validation.
- JSON layout, rules, and portable snapshot files.
- Hard rules for fixed seats, adjacency, non-adjacency, and minimum distance.
- Soft preferences for vision-front, height-back, reproducible randomization, and score balance.
- Deterministic fallback solver, with optional OR-Tools CP-SAT support via the `solver` extra.
- CLI commands for demo generation, solving, and Excel / PNG / HTML export.
- Local Streamlit web UI.
- Pytest coverage and GitHub Actions CI.
