# Release checklist

Use this checklist for every public v2 release. Replace `<version>` with the
exact crate version (for example `2.0.0`) and `<tag>` with `v<version>` (for
example `v2.0.0`). A release candidate must use its own version, for example
`2.0.0rc1`; crates.io does not allow files to be replaced.

## Scope and repository state

- [ ] Every item promised by the milestone is implemented, documented and
      covered by an acceptance test, or has been moved explicitly to a later
      milestone.
- [ ] The pull request is reviewable as a sequence of focused commits and has
      no unresolved review threads.
- [ ] `CHANGELOG.md` describes user-visible additions, changes and fixes.
- [ ] Schema changes include migration behavior, committed schemas and fixtures.
- [ ] `git status --short` is clean in the release checkout.
- [ ] `python3 scripts/check_repository_hygiene.py` passes (Python is only a
      dev/test runner; the release tree itself contains no Python runtime).

## Release quality gate

Run this gate once after the version scope is frozen, rather than repeating the
entire matrix after every small change.

- [ ] `cargo test --locked -p seattrellis_core`
- [ ] `cargo test --locked -p seattrellis_cli`
- [ ] `cargo test --locked -p seattrellis_app`
- [ ] `cargo clippy --all-targets -p seattrellis_core -p seattrellis_cli -- -D warnings`
- [ ] `cargo clippy --all-targets -p seattrellis_app -- -D warnings`
- [ ] `cargo build --locked -p seattrellis_desktop` (Tauri shell, needs the
      pinned 1.88 toolchain)
- [ ] React workbench: `cd clients/web && npm test && npm run typecheck && npm run build`
- [ ] Run the Python↔Rust oracle differential on the frozen fixture corpus:
      `python3 scripts/rust_python_diff.py --fixtures` (mismatch exits non-zero).
- [ ] Run the documented release benchmark matrix for 40, 50 and 60 students,
      light and dense rules, and 1, 5 and 20 candidates. Archive both JSON and
      Markdown reports.
- [ ] Run the browser-level E2E acceptance suite from a clean install
      (`web-e2e-rust` CI job covers the workbench with Python as runner only).
- [ ] Launch `seattrellis_app` on `127.0.0.1` and complete one manual
      import → solve → adjust → export workflow in the React workbench.
- [ ] Confirm GitHub Actions passes on Linux, Windows and macOS, including
      clippy, security, hygiene and Web E2E jobs.

## Functional acceptance

- [ ] `seattrellis_cli doctor`, `seattrellis_cli validate --problem`,
      a `solve` and an `export` complete successfully on a sample problem.
- [ ] Generate several candidates and inspect the recommendation, score
      differences, hard-constraint summary and history explanation.
- [ ] Exercise manual swap, move, unseat, lock and constrained repair via
      `edit` / `repair`; confirm `metadata.lock_state` and provenance are saved.
- [ ] Run the project lifecycle: `project-init`, `project-info`,
      `project-validate`, `project-solve`, `project-export`, `project-rotate`,
      `project-edit`, `project-repair`, `project-privacy`, `project-pack`,
      `project-restore`.
- [ ] Export teacher and public outputs in Chinese and English, including A4
      portrait and landscape layouts where supported.
- [ ] Confirm a public export contains no scores, notes, special needs, height,
      vision data or un-anonymized student identifiers.
- [ ] Export and validate the committed JSON Schemas. Run `schema-migrate`
      dry-run and write modes, including `.bak` backup creation for an existing
      destination.
- [ ] Verify the seven solve statuses (`Solved`, `ProvenInfeasible`, `Timeout`,
      `Unknown`, `InvalidInput`, `Cancelled`, `InternalError`) and the frozen
      exit codes 0/2/3/4/5/70/130. A timeout or unknown status must not be
      described as proven infeasibility.

## Packaging and privacy

- [ ] Build from a clean checkout so ignored or untracked files cannot enter
      the release.
- [ ] Run `python3 scripts/check_no_python_runtime.py --tree --expect-retired`
      and the binary scan on the release CLI/App server.
- [ ] Confirm examples contain fictional data only and no real names, IDs,
      school details, notes, snapshots, exports, keys or environment files are
      tracked.
- [ ] Confirm generated output and private-data directories remain ignored.
- [ ] Confirm `Cargo.toml` crate versions equal `<version>` and match the
      `v<version>` release tag.

## Release candidate

- [ ] Set crate versions to a unique release candidate version and merge the
      reviewed candidate commit.
- [ ] Publish the candidate to crates.io and verify a clean
      `cargo install seattrellis_cli --version <candidate>` works, plus
      `seattrellis_cli --version` / `seattrellis_cli doctor` and one
      validate/solve/export smoke workflow.

## Public release

- [ ] Restore crate versions to the final `<version>` and rerun the release
      quality gate for the reviewed commit.
- [ ] Create a draft GitHub Release targeting the reviewed commit with tag
      `<tag>` and title `SeatTrellis <tag>`.
- [ ] Write concise release notes with upgrade, compatibility, privacy and known
      limitation sections.
- [ ] Publish the GitHub Release and confirm the `build-binaries` and
      `publish-assets` jobs succeed, including the 6 platform binaries and the
      `SHA256SUMS` attachment.
- [ ] For a desktop release, run `Tauri desktop bundles` (or publish a
      `desktop-v*` preview release) and inspect the unsigned
      `.app`/`.dmg`, `.msi`/NSIS, and `.deb` assets before enabling signing or
      notarisation. Attach `DESKTOP-SHA256SUMS` alongside the bundles.
- [ ] Publish the CLI to crates.io and verify a clean
      `cargo install seattrellis_cli` plus one validate/solve/export smoke
      workflow.
- [ ] Confirm the v1 line stays frozen: `v1.*` tags and the
      `v1.x-maintenance` branch continue to serve the legacy 1.9.0 package and
      do not receive Rust binaries.

If publication fails after a version has reached crates.io, follow
`docs/publishing.md`: do not replace the files or rewrite the tag. Yank the
affected release when appropriate and publish a new patch version.
